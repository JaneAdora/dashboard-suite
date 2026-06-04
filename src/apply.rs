//! Build selected launchers from their repos, install bins to ~/.local/bin
//! (recording what we install and refusing to clobber non-suite files), and
//! write glance's panels.toml. Honors --dry-run.
use crate::manifest::Launcher;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Plan {
    pub launchers: Vec<Launcher>,
    pub panels: Vec<String>,
    pub dry_run: bool,
}

pub fn projects_dir() -> PathBuf {
    if let Ok(p) = std::env::var("RSUITE_PROJECTS") {
        return PathBuf::from(p);
    }
    dirs::home_dir().unwrap_or_default().join("projects")
}
fn bin_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".local/bin")
}

/// Resolve a `~`/`~/...` path against $HOME; pass other paths through unchanged.
fn expand_tilde(p: &str) -> PathBuf {
    if p == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = p.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(p)
}

/// Install dir for a launcher: its `prefix` override if set, else the suite bin dir.
fn launcher_bin_dir(l: &Launcher) -> PathBuf {
    match l.prefix.as_deref() {
        Some(p) => expand_tilde(p),
        None => bin_dir(),
    }
}
fn data_dir() -> PathBuf {
    std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("dashboard-suite")
}
fn config_dir() -> PathBuf {
    std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join(".config"))
}

#[derive(Default, Serialize, Deserialize)]
struct Installed {
    #[serde(default)]
    bin: Vec<Record>,
}
#[derive(Serialize, Deserialize, Clone)]
struct Record {
    name: String,
    path: String,
    repo: String,
}

/// First match for `cmd` on $PATH.
pub fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
}

fn is_elf(p: &Path) -> bool {
    use std::io::Read;
    let mut f = match std::fs::File::open(p) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).is_ok() && &magic == b"\x7fELF"
}

#[derive(Debug, PartialEq, Eq)]
enum GuardDecision {
    /// Safe to install over (nothing there, or the suite already owns it).
    Proceed,
    /// Not ours and not an ELF: a user shell wrapper (the op-wrapper lesson) —
    /// leave it untouched and skip installing this component.
    Skip,
    /// Not ours but an ELF: a same-named binary the user placed here. Preserve
    /// it (rename to a backup) before installing ours over the path.
    BackupThenInstall,
}

/// Decide how to treat an existing destination before installing over it.
fn guard_decision(exists: bool, elf: bool, is_ours: bool) -> GuardDecision {
    if !exists || is_ours {
        GuardDecision::Proceed
    } else if !elf {
        GuardDecision::Skip
    } else {
        GuardDecision::BackupThenInstall
    }
}

#[cfg(test)]
mod guard_tests {
    use super::{guard_decision, GuardDecision};

    #[test]
    fn proceeds_when_absent_or_ours() {
        assert_eq!(guard_decision(false, false, false), GuardDecision::Proceed);
        assert_eq!(guard_decision(true, true, true), GuardDecision::Proceed);
        assert_eq!(guard_decision(true, false, true), GuardDecision::Proceed);
    }

    #[test]
    fn skips_non_suite_non_elf() {
        // op-wrapper lesson: never clobber a user's shell wrapper.
        assert_eq!(guard_decision(true, false, false), GuardDecision::Skip);
    }

    #[test]
    fn backs_up_non_suite_elf() {
        // A user's same-named ELF binary must be preserved, not destroyed.
        assert_eq!(guard_decision(true, true, false), GuardDecision::BackupThenInstall);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CloneAction {
    /// Repo already present; build it in place.
    UseExisting,
    /// Repo absent but a clone URL is known; clone it first.
    Clone,
    /// Repo absent and no URL to clone from; skip this component.
    SkipNoUrl,
}

/// Decide whether to build in place, clone first, or skip a component.
fn clone_action(exists: bool, url: Option<&str>) -> CloneAction {
    if exists {
        CloneAction::UseExisting
    } else if url.is_some() {
        CloneAction::Clone
    } else {
        CloneAction::SkipNoUrl
    }
}

#[cfg(test)]
mod clone_tests {
    use super::{clone_action, CloneAction};

    #[test]
    fn uses_existing_repo_regardless_of_url() {
        assert_eq!(clone_action(true, Some("u")), CloneAction::UseExisting);
        assert_eq!(clone_action(true, None), CloneAction::UseExisting);
    }

    #[test]
    fn clones_when_absent_with_url() {
        assert_eq!(clone_action(false, Some("u")), CloneAction::Clone);
    }

    #[test]
    fn skips_when_absent_without_url() {
        assert_eq!(clone_action(false, None), CloneAction::SkipNoUrl);
    }
}

pub fn run(plan: &Plan) -> Result<()> {
    let projects = projects_dir();
    let bin = bin_dir();
    let mut installed = load_installed();
    if !plan.dry_run {
        std::fs::create_dir_all(&bin).ok();
    }

    for l in &plan.launchers {
        let missing: Vec<&str> = l.requires.iter().map(|s| s.as_str()).filter(|r| which(r).is_none()).collect();
        if !missing.is_empty() {
            eprintln!("  ! {}: missing dependency: {}", l.name, missing.join(", "));
        }
        let repo = projects.join(&l.repo);
        match clone_action(repo.is_dir(), l.url.as_deref()) {
            CloneAction::UseExisting => {}
            CloneAction::SkipNoUrl => {
                eprintln!("  ! {}: repo not found at {} and no clone url; skipping", l.name, repo.display());
                continue;
            }
            CloneAction::Clone => {
                let url = l.url.as_deref().unwrap_or_default();
                if plan.dry_run {
                    println!("  would clone {} -> {}", url, repo.display());
                } else {
                    println!("==> cloning {} ({}) from {}", l.name, l.repo, url);
                    std::fs::create_dir_all(&projects).ok();
                    let status = Command::new("git")
                        .arg("clone")
                        .arg(url)
                        .arg(&repo)
                        .status()
                        .with_context(|| format!("git clone {url}"))?;
                    if !status.success() {
                        bail!("{}: git clone failed ({})", l.name, url);
                    }
                }
            }
        }
        let artifact = repo.join("target/release").join(l.artifact());
        let dest_dir = launcher_bin_dir(l);
        let dest = dest_dir.join(&l.bin);

        if plan.dry_run {
            let how = match &l.package {
                Some(p) => format!("cargo build --release -p {p}"),
                None => "cargo build --release".to_string(),
            };
            println!("  would build {:<8} ({}: {})  ->  {}", l.name, l.repo, how, dest.display());
            continue;
        }

        println!("==> building {} ({})", l.name, l.repo);
        let mut cmd = Command::new("cargo");
        cmd.arg("build").arg("--release").current_dir(&repo);
        if let Some(p) = &l.package {
            cmd.arg("-p").arg(p);
        }
        let status = cmd.status().with_context(|| format!("run cargo for {}", l.name))?;
        if !status.success() {
            bail!("build failed for {}", l.name);
        }
        if !artifact.is_file() {
            bail!("{}: artifact missing at {}", l.name, artifact.display());
        }
        std::fs::create_dir_all(&dest_dir).ok();
        // Clobber guard (the op-wrapper lesson): protect files we did not install.
        let dest_s = dest.to_string_lossy().into_owned();
        let is_ours = installed.bin.iter().any(|r| r.path == dest_s);
        match guard_decision(dest.exists(), is_elf(&dest), is_ours) {
            GuardDecision::Skip => {
                eprintln!("  ! refusing to overwrite non-suite file {} (not an ELF binary)", dest.display());
                continue;
            }
            GuardDecision::BackupThenInstall => {
                let bak = PathBuf::from(format!("{dest_s}.pre-suite.bak"));
                match std::fs::rename(&dest, &bak) {
                    Ok(()) => eprintln!(
                        "  ! {} was not installed by the suite; backed it up to {}",
                        dest.display(),
                        bak.display()
                    ),
                    Err(e) => {
                        eprintln!(
                            "  ! refusing to overwrite non-suite binary {} (backup failed: {e})",
                            dest.display()
                        );
                        continue;
                    }
                }
            }
            GuardDecision::Proceed => {}
        }
        std::fs::copy(&artifact, &dest).with_context(|| format!("install {}", dest.display()))?;
        set_exec(&dest);
        // If this component was recorded at a different path before (e.g. moved
        // from ~/.local/bin into a prefix dir), drop the stale shadowed copy.
        if let Some(old) = installed
            .bin
            .iter()
            .find(|r| r.name == l.bin && r.path != dest_s)
            .map(|r| r.path.clone())
        {
            let oldp = PathBuf::from(&old);
            if oldp.is_file() && is_elf(&oldp) {
                std::fs::remove_file(&oldp).ok();
                println!("  (removed stale copy at {old})");
            }
        }
        record(&mut installed, &l.bin, &dest, &l.repo);
        println!("  installed {} -> {}", l.bin, dest.display());
    }

    if !plan.dry_run {
        save_installed(&installed);
    }

    if !plan.panels.is_empty() {
        let cfg = config_dir().join("glance/panels.toml");
        if plan.dry_run {
            println!("  would write {} ({} panels)", cfg.display(), plan.panels.len());
        } else {
            write_panels(&cfg, &plan.panels)?;
            println!("  wrote {} ({} panels)", cfg.display(), plan.panels.len());
        }
    }

    if plan.dry_run {
        println!("\n(dry run — nothing was built, installed, or written)");
    }
    Ok(())
}

fn write_panels(cfg: &Path, panels: &[String]) -> Result<()> {
    if let Some(parent) = cfg.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if cfg.exists() {
        let bak = cfg.with_extension("toml.bak");
        std::fs::copy(cfg, &bak).ok();
        eprintln!("  (backed up existing config -> {})", bak.display());
    }
    let mut s = String::from(
        "# Written by rsuite. Order sets slots: first 9 -> keys 1-9, 10th -> 0, rest n/p.\npanels = [\n",
    );
    for p in panels {
        s.push_str(&format!("  \"{p}\",\n"));
    }
    s.push_str("]\n");
    std::fs::write(cfg, s).with_context(|| format!("write {}", cfg.display()))
}

fn load_installed() -> Installed {
    std::fs::read_to_string(data_dir().join("installed.toml"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}
fn save_installed(i: &Installed) {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).ok();
    if let Ok(s) = toml::to_string(i) {
        std::fs::write(dir.join("installed.toml"), s).ok();
    }
}
fn record(i: &mut Installed, name: &str, path: &Path, repo: &str) {
    let path = path.to_string_lossy().into_owned();
    i.bin.retain(|r| r.name != name);
    i.bin.push(Record { name: name.into(), path, repo: repo.into() });
}

#[cfg(unix)]
fn set_exec(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(m) = std::fs::metadata(p) {
        let mut perm = m.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(p, perm).ok();
    }
}
#[cfg(not(unix))]
fn set_exec(_p: &Path) {}


fn mark(ok: bool) -> char {
    if ok { '✓' } else { '✗' }
}

/// Pull panel names out of a glance panels.toml (quoted tokens).
fn parse_panel_names(p: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(p).unwrap_or_default();
    let mut out = Vec::new();
    let mut in_q = false;
    let mut buf = String::new();
    for c in text.chars() {
        if c == '"' {
            if in_q {
                out.push(std::mem::take(&mut buf));
            }
            in_q = !in_q;
        } else if in_q {
            buf.push(c);
        }
    }
    out
}

/// Read-only health check: PATH, per-launcher install + deps, glance config.
pub fn doctor(m: &crate::manifest::Manifest) -> Result<()> {
    let bin = bin_dir();
    println!("rsuite doctor\n");

    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d == bin))
        .unwrap_or(false);
    println!("  [{}] ~/.local/bin on $PATH", mark(on_path));
    println!("  [{}] cargo available", mark(which("cargo").is_some()));

    println!("\nLaunchers:");
    for l in &m.launchers {
        let dir = launcher_bin_dir(l);
        let inst = dir.join(&l.bin).is_file();
        let miss: Vec<&str> = l.requires.iter().map(|s| s.as_str()).filter(|r| which(r).is_none()).collect();
        let needs = if miss.is_empty() { String::new() } else { format!("  needs: {}", miss.join(", ")) };
        let at = if l.prefix.is_some() { format!(" -> {}", dir.display()) } else { String::new() };
        println!(
            "  [{}] {:<8} {}{}{}",
            mark(inst),
            l.name,
            if inst { "installed" } else { "not installed" },
            at,
            needs
        );
    }

    println!("\nGlance config:");
    let cfg = config_dir().join("glance/panels.toml");
    if cfg.is_file() {
        let names = parse_panel_names(&cfg);
        let known: HashSet<&str> = m.panels.iter().map(|p| p.name.as_str()).collect();
        let unknown: Vec<&str> = names.iter().map(|s| s.as_str()).filter(|n| !known.contains(n)).collect();
        println!("  [{}] {} ({} panels)", mark(true), cfg.display(), names.len());
        if !unknown.is_empty() {
            println!("  [!] unknown panels: {}", unknown.join(", "));
        }
    } else {
        println!("  [!] no panels.toml — glance shows its default registry");
    }
    Ok(())
}

/// Rebuild + reinstall every launcher currently recorded in installed.toml.
pub fn update(m: &crate::manifest::Manifest) -> Result<()> {
    let installed = load_installed();
    let names: HashSet<String> = installed.bin.iter().map(|r| r.name.clone()).collect();
    let launchers: Vec<Launcher> = m.launchers.iter().filter(|l| names.contains(&l.bin)).cloned().collect();
    if launchers.is_empty() {
        println!("nothing installed to update (run `rsuite` first).");
        return Ok(());
    }
    println!("updating {} installed launcher(s)...", launchers.len());
    run(&Plan { launchers, panels: Vec::new(), dry_run: false })
}

/// Remove every bin we installed (per installed.toml); only deletes ELF files we
/// recorded, never a stray wrapper. With `remove_config`, also drops panels.toml.
pub fn uninstall(remove_config: bool) -> Result<()> {
    let mut installed = load_installed();
    if installed.bin.is_empty() {
        println!("nothing recorded as installed.");
    }
    for r in &installed.bin {
        let p = PathBuf::from(&r.path);
        if p.is_file() && is_elf(&p) {
            std::fs::remove_file(&p).ok();
            println!("  removed {}", r.path);
        } else if p.is_file() {
            eprintln!("  ! left {} (not an ELF we recognize)", r.path);
        }
    }
    installed.bin.clear();
    save_installed(&installed);
    if remove_config {
        let cfg = config_dir().join("glance/panels.toml");
        if cfg.is_file() {
            std::fs::remove_file(&cfg).ok();
            println!("  removed {}", cfg.display());
        }
    }
    println!("done.");
    Ok(())
}


/// Current glance panel list: the user's panels.toml if present, else the
/// manifest defaults (what a fresh picker run would write).
fn current_panels(m: &crate::manifest::Manifest) -> Vec<String> {
    let cfg = config_dir().join("glance/panels.toml");
    if cfg.is_file() {
        parse_panel_names(&cfg)
    } else {
        m.panels.iter().filter(|p| p.default).map(|p| p.name.clone()).collect()
    }
}

/// Split names into known launchers, known panels, and unknowns.
fn partition(m: &crate::manifest::Manifest, names: &[String]) -> (Vec<Launcher>, Vec<String>, Vec<String>) {
    let (mut ls, mut ps, mut un) = (Vec::new(), Vec::new(), Vec::new());
    for n in names {
        if let Some(l) = m.launchers.iter().find(|l| &l.name == n || &l.bin == n) {
            ls.push(l.clone());
        } else if m.panels.iter().any(|p| &p.name == n) {
            ps.push(n.clone());
        } else {
            un.push(n.clone());
        }
    }
    (ls, ps, un)
}

/// Install named launchers and/or merge named panels into panels.toml.
pub fn add(m: &crate::manifest::Manifest, names: &[String]) -> Result<()> {
    if names.is_empty() {
        println!("usage: rsuite add <launcher|panel>...");
        return Ok(());
    }
    let (launchers, panels, unknown) = partition(m, names);
    for u in &unknown {
        eprintln!("  ! unknown component: {u}");
    }
    if !launchers.is_empty() {
        run(&Plan { launchers, panels: Vec::new(), dry_run: false })?;
    }
    if !panels.is_empty() {
        let cfg = config_dir().join("glance/panels.toml");
        let mut cur = current_panels(m);
        let mut added = Vec::new();
        for n in &panels {
            if !cur.contains(n) {
                cur.push(n.clone());
                added.push(n.clone());
            }
        }
        write_panels(&cfg, &cur)?;
        let what = if added.is_empty() { "(already present)".to_string() } else { added.join(", ") };
        println!("  panels: added {} -> {} ({} total)", what, cfg.display(), cur.len());
    }
    Ok(())
}

/// Uninstall named launchers and/or drop named panels from panels.toml.
pub fn remove(m: &crate::manifest::Manifest, names: &[String]) -> Result<()> {
    if names.is_empty() {
        println!("usage: rsuite remove <launcher|panel>...");
        return Ok(());
    }
    let (launchers, panels, unknown) = partition(m, names);
    for u in &unknown {
        eprintln!("  ! unknown component: {u}");
    }
    if !launchers.is_empty() {
        let mut installed = load_installed();
        for l in &launchers {
            let dest = launcher_bin_dir(l).join(&l.bin);
            if dest.is_file() && is_elf(&dest) {
                std::fs::remove_file(&dest).ok();
                println!("  removed {}", dest.display());
            } else if dest.is_file() {
                eprintln!("  ! left {} (not an ELF we recognize)", dest.display());
            }
            installed.bin.retain(|r| r.name != l.bin);
        }
        save_installed(&installed);
    }
    if !panels.is_empty() {
        let cfg = config_dir().join("glance/panels.toml");
        let drop: HashSet<&str> = panels.iter().map(|s| s.as_str()).collect();
        let cur: Vec<String> = current_panels(m).into_iter().filter(|p| !drop.contains(p.as_str())).collect();
        write_panels(&cfg, &cur)?;
        println!("  panels: removed {} -> {} ({} left)", panels.join(", "), cfg.display(), cur.len());
    }
    Ok(())
}
