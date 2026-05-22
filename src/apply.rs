//! Build selected launchers from their repos, install bins to ~/.local/bin
//! (recording what we install and refusing to clobber non-suite files), and
//! write glance's panels.toml. Honors --dry-run.
use crate::manifest::Launcher;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
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
        if !repo.is_dir() {
            eprintln!("  ! {}: repo not found at {} — skipping", l.name, repo.display());
            continue;
        }
        let artifact = repo.join("target/release").join(l.artifact());
        let dest = bin.join(&l.bin);

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
        // Clobber guard (the op-wrapper lesson): never overwrite a non-ELF file
        // (e.g. a credential shell wrapper) that we did not install ourselves.
        let dest_s = dest.to_string_lossy().into_owned();
        if dest.exists() && !is_elf(&dest) && !installed.bin.iter().any(|r| r.path == dest_s) {
            eprintln!("  ! refusing to overwrite non-suite file {} (not an ELF binary)", dest.display());
            continue;
        }
        std::fs::copy(&artifact, &dest).with_context(|| format!("install {}", dest.display()))?;
        set_exec(&dest);
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
    i.bin.retain(|r| r.path != path);
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
