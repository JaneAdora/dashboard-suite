mod apply;
mod manifest;
mod picker;
mod theme;

use anyhow::Result;
use manifest::{Launcher, Manifest};

const HELP: &str = "\
rsuite — Dashboard Widget Suite installer

USAGE:
  rsuite                    Interactive picker (launchers + glance panels), then install
  rsuite --defaults         Install the default set, no prompt
  rsuite --all              Install everything
  rsuite --launchers a,b    Install just these launchers
  rsuite --panels cpu,mem   Write glance panels.toml with just these
  rsuite --dry-run [...]    Show the plan without building/installing/writing

VERBS:
  rsuite list               Show all components, defaults, and missing deps
  rsuite doctor             Health check: PATH, installed bins, deps, glance config
  rsuite update             Rebuild + reinstall everything currently installed
  rsuite uninstall [--config]  Remove installed bins (and optionally panels.toml)

  rsuite --help | --version
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        Some("list") => {
            print_list(&Manifest::load()?);
            return Ok(());
        }
        Some("doctor") => return apply::doctor(&Manifest::load()?),
        Some("update") => return apply::update(&Manifest::load()?),
        Some("uninstall") => return apply::uninstall(args.iter().any(|a| a == "--config")),
        _ => {}
    }

    let (mut dry, mut all, mut defaults) = (false, false, false);
    let mut launchers: Option<Vec<String>> = None;
    let mut panels: Option<Vec<String>> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--help" | "-h" => {
                print!("{HELP}");
                return Ok(());
            }
            "--version" | "-V" => {
                println!("rsuite {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            "--dry-run" => dry = true,
            "--all" => all = true,
            "--defaults" => defaults = true,
            "--launchers" => launchers = Some(split(it.next())),
            "--panels" => panels = Some(split(it.next())),
            other => {
                eprintln!("rsuite: unknown arg: {other}\n\n{HELP}");
                std::process::exit(2);
            }
        }
    }

    let m = Manifest::load()?;
    let non_interactive = all || defaults || launchers.is_some() || panels.is_some();
    let (sel_launchers, sel_panels): (Vec<Launcher>, Vec<String>) = if non_interactive {
        (select_launchers(&m, all, defaults, &launchers), select_panels(&m, all, defaults, &panels))
    } else {
        match picker::run(&m)? {
            Some(s) => (
                s.launchers.into_iter().map(|i| m.launchers[i].clone()).collect(),
                s.panels.into_iter().map(|i| m.panels[i].name.clone()).collect(),
            ),
            None => {
                println!("cancelled.");
                return Ok(());
            }
        }
    };

    if sel_launchers.is_empty() && sel_panels.is_empty() {
        println!("nothing selected.");
        return Ok(());
    }
    apply::run(&apply::Plan { launchers: sel_launchers, panels: sel_panels, dry_run: dry })
}

fn split(s: Option<&String>) -> Vec<String> {
    s.map(|x| x.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
        .unwrap_or_default()
}

fn select_launchers(m: &Manifest, all: bool, defaults: bool, names: &Option<Vec<String>>) -> Vec<Launcher> {
    if let Some(ns) = names {
        return m.launchers.iter().filter(|l| ns.iter().any(|n| n == &l.name || n == &l.bin)).cloned().collect();
    }
    if all {
        return m.launchers.clone();
    }
    if defaults {
        return m.launchers.iter().filter(|l| l.default).cloned().collect();
    }
    Vec::new()
}

fn select_panels(m: &Manifest, all: bool, defaults: bool, names: &Option<Vec<String>>) -> Vec<String> {
    if let Some(ns) = names {
        return m.panels.iter().filter(|p| ns.contains(&p.name)).map(|p| p.name.clone()).collect();
    }
    if all {
        return m.panels.iter().map(|p| p.name.clone()).collect();
    }
    if defaults {
        return m.panels.iter().filter(|p| p.default).map(|p| p.name.clone()).collect();
    }
    Vec::new()
}

fn print_list(m: &Manifest) {
    println!("Launchers (* = default):");
    for l in &m.launchers {
        let miss: Vec<&str> = l.requires.iter().map(|s| s.as_str()).filter(|r| apply::which(r).is_none()).collect();
        let star = if l.default { "*" } else { " " };
        let needs = if miss.is_empty() { String::new() } else { format!("  (needs: {})", miss.join(", ")) };
        println!("  {star} {:<8} {}{}", l.name, l.summary, needs);
    }
    println!("\nGlance panels (* = default):");
    for p in &m.panels {
        let star = if p.default { "*" } else { " " };
        let env = if p.env.is_empty() { String::new() } else { format!("  env: {}", p.env.join(", ")) };
        println!("  {star} {:<12} {}{}", p.name, p.summary, env);
    }
}
