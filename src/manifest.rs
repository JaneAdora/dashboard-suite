//! The component manifest (`suite.toml`): launchers (cargo-built bins) + glance
//! panels. Prefers a runtime file (editable) and falls back to an embedded copy.
use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Manifest {
    #[serde(default, rename = "launcher")]
    pub launchers: Vec<Launcher>,
    #[serde(default, rename = "panel")]
    pub panels: Vec<Panel>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Launcher {
    pub name: String,
    #[serde(default)]
    pub summary: String,
    pub repo: String,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
    pub bin: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub default: bool,
}

impl Launcher {
    /// target/release/<artifact>; defaults to the installed bin name.
    pub fn artifact(&self) -> &str {
        self.artifact.as_deref().unwrap_or(&self.bin)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Panel {
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub env: Vec<String>,
}

const EMBEDDED: &str = include_str!("../suite.toml");

impl Manifest {
    pub fn load() -> Result<Manifest> {
        for p in runtime_paths() {
            if p.is_file() {
                let s = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
                return toml::from_str(&s).with_context(|| format!("parse {}", p.display()));
            }
        }
        toml::from_str(EMBEDDED).context("parse embedded suite.toml")
    }
}

fn runtime_paths() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(p) = std::env::var("RSUITE_MANIFEST") {
        v.push(PathBuf::from(p));
    }
    if let Some(home) = dirs::home_dir() {
        v.push(home.join("projects/dashboard-suite/suite.toml"));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_and_has_components() {
        let m = Manifest::load().expect("manifest loads");
        assert!(!m.launchers.is_empty());
        assert!(m.panels.iter().any(|p| p.name == "cpu"));
        // the 1p launcher builds the `onepw` artifact but installs as `1p`
        assert!(m.launchers.iter().any(|l| l.bin == "1p" && l.artifact() == "onepw"));
    }
}
