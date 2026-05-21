# Launchers Wave 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `launchers` cargo workspace with a reusable `launcher-core` lib and the first launcher, `gst`, plus a glance `launchers` palette panel with a live `gst` card, proving the dual-form pattern end to end.

**Architecture:** A new `~/projects/launchers` cargo workspace holds `launcher-core` (shared TUI scaffold + `--summary --json` envelope) and one binary crate per launcher. Wave 0 builds only `gst`. glance stays decoupled: its new `launchers` panel holds a static palette table and fetches the gst card by shelling out to `gst --summary --json` on a background thread (the existing weather/commits pattern).

**Tech Stack:** Rust 2021, ratatui 0.29, crossterm 0.28, anyhow 1, base64 0.22, serde 1 + serde_json 1. gst shells out to the system `git`. No `git2`.

**Spec:** `~/projects/dashboard-suite/docs/superpowers/specs/2026-05-20-launchers-design.md`

---

## File structure (Wave 0)

```
~/projects/launchers/
  Cargo.toml                      # workspace members
  launcher-core/
    Cargo.toml
    src/lib.rs                    # re-exports
    src/clipboard.rs              # OSC 52 (4 KiB cap)
    src/summary.rs               # Summary envelope + --summary --json arg parse
    src/filter.rs                 # fuzzy subsequence match + filter_indices
    src/theme.rs                  # Rep Cap palette
    src/exit.rs                   # RunOutcome + print_and_exit
    src/list.rs                   # ListState<T> selection helper
    src/ui.rs                     # single_column layout, footer + toast
  gst/
    Cargo.toml
    src/main.rs                   # CLI dispatch: interactive vs --summary --json
    src/source.rs                 # repo discovery + git status/log parsing
    src/app.rs                    # interactive TUI (event loop, render, actions)
  install.sh                      # build + copy bins to ~/.local/bin

~/projects/glance/               # existing repo, modified
  src/panels/launchers.rs         # NEW: palette panel + gst card
  src/panels/mod.rs               # MODIFY: register "launchers"
```

`launcher-core` keeps each concern in its own small file so units stay testable in isolation. gst's parsing logic (`source.rs`) is pure and unit-tested; its TUI wiring (`app.rs`) is verified by build + smoke run.

---

## Task 1: Workspace + launcher-core skeleton

**Files:**
- Create: `~/projects/launchers/Cargo.toml`
- Create: `~/projects/launchers/launcher-core/Cargo.toml`
- Create: `~/projects/launchers/launcher-core/src/lib.rs`

- [ ] **Step 1: Create the workspace and crate**

```bash
mkdir -p ~/projects/launchers/launcher-core/src
cd ~/projects/launchers && git init
```

- [ ] **Step 2: Write the workspace `Cargo.toml`**

`~/projects/launchers/Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["launcher-core", "gst"]

[workspace.dependencies]
ratatui = "0.29"
crossterm = "0.28"
anyhow = "1"
base64 = "0.22"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
lto = "thin"
codegen-units = 1
```

- [ ] **Step 3: Write `launcher-core/Cargo.toml`**

```toml
[package]
name = "launcher-core"
version = "0.1.0"
edition = "2021"

[dependencies]
ratatui = { workspace = true }
crossterm = { workspace = true }
anyhow = { workspace = true }
base64 = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 4: Write a stub `lib.rs` so the crate compiles**

`launcher-core/src/lib.rs`:
```rust
pub mod clipboard;
pub mod summary;
pub mod filter;
pub mod theme;
pub mod exit;
pub mod list;
pub mod ui;
```

Create empty module files so it builds:
```bash
cd ~/projects/launchers/launcher-core/src
for m in clipboard summary filter theme exit list ui; do touch $m.rs; done
```

- [ ] **Step 5: Verify the workspace is recognized**

Run: `cd ~/projects/launchers && cargo build -p launcher-core`
Expected: builds (warnings about empty modules are fine). gst is not built yet (no crate); that is added in Task 7.

Note: `members` lists `gst`, which does not exist yet. Temporarily set `members = ["launcher-core"]` for this task, then restore `["launcher-core", "gst"]` in Task 7 Step 1.

- [ ] **Step 6: Commit**

```bash
cd ~/projects/launchers
printf "/target\n" > .gitignore
git add . && git commit -m "chore: launchers workspace + launcher-core skeleton"
```

---

## Task 2: launcher-core clipboard (OSC 52 with 4 KiB cap)

**Files:**
- Modify: `~/projects/launchers/launcher-core/src/clipboard.rs`

- [ ] **Step 1: Write the failing test**

`launcher-core/src/clipboard.rs`:
```rust
//! OSC 52 clipboard escape with a hard 4 KiB pre-base64 cap (Termux pty limit).
use base64::Engine;

pub const OSC52_CAP: usize = 4096;

/// Build the OSC 52 escape sequence for `data`. Returns (sequence, truncated).
/// Caps the raw bytes at OSC52_CAP before base64 to stay under Termux/Blink limits.
pub fn osc52_sequence(data: &str) -> (String, bool) {
    let bytes = data.as_bytes();
    let truncated = bytes.len() > OSC52_CAP;
    let slice = if truncated { &bytes[..OSC52_CAP] } else { bytes };
    let b64 = base64::engine::general_purpose::STANDARD.encode(slice);
    (format!("\x1b]52;c;{b64}\x07"), truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_not_truncated() {
        let (seq, truncated) = osc52_sequence("hello");
        assert!(!truncated);
        assert!(seq.starts_with("\x1b]52;c;"));
        assert!(seq.ends_with('\x07'));
    }

    #[test]
    fn long_input_truncated_at_cap() {
        let big = "a".repeat(OSC52_CAP + 100);
        let (_seq, truncated) = osc52_sequence(&big);
        assert!(truncated);
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd ~/projects/launchers && cargo test -p launcher-core clipboard`
Expected: 2 passed. (Implementation and test are in the same step because the logic is trivial and self-contained; the test still gates the behavior.)

- [ ] **Step 3: Commit**

```bash
git add launcher-core/src/clipboard.rs && git commit -m "feat(core): OSC 52 clipboard with 4 KiB cap"
```

---

## Task 3: launcher-core summary envelope + arg parse

**Files:**
- Modify: `~/projects/launchers/launcher-core/src/summary.rs`

- [ ] **Step 1: Write the implementation + tests**

`launcher-core/src/summary.rs`:
```rust
//! Shared JSON envelope every launcher emits for glance cards via `--summary --json`.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub launcher: String,
    pub headline: String,
    pub items: Vec<String>,
    pub count: usize,
}

impl Summary {
    pub fn new(launcher: &str, headline: impl Into<String>, items: Vec<String>, count: usize) -> Self {
        Self { launcher: launcher.to_string(), headline: headline.into(), items, count }
    }
    pub fn emit_json(&self) -> String {
        serde_json::to_string(self).expect("Summary serializes")
    }
}

/// True when invoked as `<bin> --summary --json` (order-independent).
pub fn wants_summary(args: &[String]) -> bool {
    args.iter().any(|a| a == "--summary") && args.iter().any(|a| a == "--json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Summary::new("gst", "3 dirty", vec!["a1 wip".into()], 16);
        let json = s.emit_json();
        let back: Summary = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn detects_summary_flags() {
        assert!(wants_summary(&["--summary".into(), "--json".into()]));
        assert!(wants_summary(&["--json".into(), "--summary".into()]));
        assert!(!wants_summary(&["--summary".into()]));
        assert!(!wants_summary(&[]));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p launcher-core summary`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add launcher-core/src/summary.rs && git commit -m "feat(core): Summary envelope + --summary --json arg parse"
```

---

## Task 4: launcher-core fuzzy filter

**Files:**
- Modify: `~/projects/launchers/launcher-core/src/filter.rs`

- [ ] **Step 1: Write the implementation + tests**

`launcher-core/src/filter.rs`:
```rust
//! Case-insensitive subsequence matching for list filtering.

/// True if every char of `query` appears in `haystack` in order (subsequence).
pub fn fuzzy_match(haystack: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let h = haystack.to_lowercase();
    let mut hay = h.chars();
    query
        .to_lowercase()
        .chars()
        .all(|qc| hay.any(|hc| hc == qc))
}

/// Indices of `items` (mapped to a string via `key`) that match `query`.
pub fn filter_indices<T>(items: &[T], query: &str, key: impl Fn(&T) -> &str) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, it)| fuzzy_match(key(it), query))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_matches() {
        assert!(fuzzy_match("launchers-design.md", "lnchdsg"));
        assert!(fuzzy_match("Cargo.toml", "cargo"));
        assert!(!fuzzy_match("abc", "xyz"));
    }

    #[test]
    fn empty_query_matches_all() {
        assert!(fuzzy_match("anything", ""));
    }

    #[test]
    fn filters_indices() {
        let v = vec!["alpha", "beta", "gamma"];
        assert_eq!(filter_indices(&v, "a", |s| s), vec![0, 1, 2]);
        assert_eq!(filter_indices(&v, "be", |s| s), vec![1]);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p launcher-core filter`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add launcher-core/src/filter.rs && git commit -m "feat(core): fuzzy subsequence filter"
```

---

## Task 5: launcher-core theme + UI helpers

**Files:**
- Modify: `~/projects/launchers/launcher-core/src/theme.rs`
- Modify: `~/projects/launchers/launcher-core/src/ui.rs`

- [ ] **Step 1: Write the theme (copied palette from the suite)**

`launcher-core/src/theme.rs`:
```rust
//! Rep Cap palette, shared across the suite.
use ratatui::style::{Color, Modifier, Style};

pub const PINK: Color = Color::Rgb(0xe8, 0x8b, 0x9f);
pub const LAVENDER: Color = Color::Rgb(0xc5, 0xa3, 0xff);
pub const MAGENTA: Color = Color::Rgb(0xff, 0x6e, 0xc7);

pub fn header() -> Style {
    Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)
}
pub fn dim() -> Style {
    Style::default().fg(LAVENDER).add_modifier(Modifier::DIM)
}
pub fn active_row() -> Style {
    Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)
}
```

- [ ] **Step 2: Write single-column layout + footer/toast**

`launcher-core/src/ui.rs`:
```rust
//! Mobile-first single-column layout helpers + footer with transient toast.
use crate::theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::{Duration, Instant};

/// Split `area` into (header line, body, footer line). Single column always.
pub fn three_row(area: Rect) -> [Rect; 3] {
    let c = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    [c[0], c[1], c[2]]
}

/// A footer keymap line plus an optional toast that expires after 3s.
pub struct Toast {
    msg: Option<(String, Instant)>,
}
impl Toast {
    pub fn new() -> Self { Self { msg: None } }
    pub fn set(&mut self, m: impl Into<String>) { self.msg = Some((m.into(), Instant::now())); }
    pub fn active(&self) -> Option<&str> {
        match &self.msg {
            Some((m, t)) if t.elapsed() < Duration::from_secs(3) => Some(m),
            _ => None,
        }
    }
}
impl Default for Toast { fn default() -> Self { Self::new() } }

pub fn render_footer(f: &mut Frame, area: Rect, keys: &str, toast: &Toast) {
    let line = match toast.active() {
        Some(m) => Line::from(vec![Span::styled(m.to_string(), theme::header())]),
        None => Line::from(vec![Span::styled(keys.to_string(), theme::dim())]),
    };
    f.render_widget(Paragraph::new(line), area);
}
```

- [ ] **Step 3: Verify it builds**

Run: `cargo build -p launcher-core`
Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add launcher-core/src/theme.rs launcher-core/src/ui.rs && git commit -m "feat(core): Rep Cap theme + single-column UI helpers"
```

---

## Task 6: launcher-core exit (RunOutcome) + list state

**Files:**
- Modify: `~/projects/launchers/launcher-core/src/exit.rs`
- Modify: `~/projects/launchers/launcher-core/src/list.rs`

- [ ] **Step 1: Write exit.rs**

`launcher-core/src/exit.rs`:
```rust
//! Exit-with-command plumbing. PrintAndExit writes the command to stdout AND
//! copies it via OSC 52 (belt-and-suspenders), so `eval "$(launcher)"` works
//! and the command is also on the clipboard.
use crate::clipboard::osc52_sequence;
use std::io::Write;

pub enum RunOutcome {
    Quit,
    PrintAndExit(String),
}

/// Call AFTER terminal raw mode is disabled and the alt screen is left.
pub fn finish(outcome: RunOutcome) {
    if let RunOutcome::PrintAndExit(cmd) = outcome {
        let (seq, _truncated) = osc52_sequence(&cmd);
        let mut out = std::io::stdout();
        let _ = out.write_all(seq.as_bytes());
        let _ = writeln!(out, "{cmd}");
        let _ = out.flush();
    }
}
```

- [ ] **Step 2: Write list.rs**

`launcher-core/src/list.rs`:
```rust
//! Generic selection state over a filtered list of items.

pub struct ListState {
    pub selected: usize,
    pub len: usize,
}
impl ListState {
    pub fn new(len: usize) -> Self { Self { selected: 0, len } }
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.selected >= len { self.selected = len.saturating_sub(1); }
    }
    pub fn down(&mut self) {
        if self.len > 0 { self.selected = (self.selected + 1).min(self.len - 1); }
    }
    pub fn up(&mut self) { self.selected = self.selected.saturating_sub(1); }
    pub fn top(&mut self) { self.selected = 0; }
    pub fn bottom(&mut self) { self.selected = self.len.saturating_sub(1); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clamps_navigation() {
        let mut s = ListState::new(3);
        s.up(); assert_eq!(s.selected, 0);
        s.down(); s.down(); s.down(); s.down(); assert_eq!(s.selected, 2);
        s.set_len(1); assert_eq!(s.selected, 0);
    }
}
```

- [ ] **Step 3: Run tests + build**

Run: `cargo test -p launcher-core list && cargo build -p launcher-core`
Expected: 1 passed; builds clean.

- [ ] **Step 4: Commit**

```bash
git add launcher-core/src/exit.rs launcher-core/src/list.rs && git commit -m "feat(core): RunOutcome exit + list selection state"
```

---

## Task 7: gst crate + source parsing (TDD)

**Files:**
- Modify: `~/projects/launchers/Cargo.toml` (restore gst member)
- Create: `~/projects/launchers/gst/Cargo.toml`
- Create: `~/projects/launchers/gst/src/source.rs`
- Create: `~/projects/launchers/gst/src/main.rs` (stub for now)

- [ ] **Step 1: Restore the gst workspace member and create the crate**

Ensure `~/projects/launchers/Cargo.toml` `members = ["launcher-core", "gst"]`.

```bash
mkdir -p ~/projects/launchers/gst/src
```

`gst/Cargo.toml`:
```toml
[package]
name = "gst"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "gst"
path = "src/main.rs"

[dependencies]
launcher-core = { path = "../launcher-core" }
ratatui = { workspace = true }
crossterm = { workspace = true }
anyhow = { workspace = true }
serde_json = { workspace = true }
dirs = "5"
```

- [ ] **Step 2: Write failing parser tests**

`gst/src/source.rs`:
```rust
//! Repo discovery and git status/log parsing. Pure functions are unit-tested;
//! the actual `git` subprocess calls are thin wrappers around them.
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

#[derive(Debug, Clone)]
pub struct Repo {
    pub path: PathBuf,
    pub name: String,
    pub dirty: usize,
}

/// Count changed entries from `git status --porcelain` output.
pub fn parse_porcelain(out: &str) -> usize {
    out.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Parse `git log --format=%h %s` output into commits.
pub fn parse_log(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(sha, subject)| Commit { sha: sha.to_string(), subject: subject.to_string() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_dirty_entries() {
        let out = " M src/main.rs\n?? new.txt\n";
        assert_eq!(parse_porcelain(out), 2);
        assert_eq!(parse_porcelain(""), 0);
    }

    #[test]
    fn parses_log_lines() {
        let out = "a1b2c3 weather: wrap forecast\n9f8e7d commits: heatmap fill\n";
        let c = parse_log(out);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0], Commit { sha: "a1b2c3".into(), subject: "weather: wrap forecast".into() });
    }
}
```

- [ ] **Step 3: Add a `main.rs` stub so the crate builds**

`gst/src/main.rs`:
```rust
mod source;
fn main() {}
```

- [ ] **Step 4: Run tests**

Run: `cd ~/projects/launchers && cargo test -p gst source`
Expected: 2 passed.

- [ ] **Step 5: Add the subprocess wrappers (discovery + git calls)**

Append to `gst/src/source.rs`:
```rust
use std::process::Command;

/// Roots to scan for repos: $WT_ROOTS (colon-list) else ~/projects + ~/Projects.
pub fn roots() -> Vec<PathBuf> {
    if let Ok(s) = std::env::var("WT_ROOTS") {
        return s.split(':').filter(|x| !x.is_empty()).map(PathBuf::from).collect();
    }
    let home = dirs::home_dir().unwrap_or_default();
    vec![home.join("projects"), home.join("Projects")]
}

pub fn discover_repos() -> Vec<Repo> {
    let mut out = Vec::new();
    for root in roots() {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() && p.join(".git").exists() {
                let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let dirty = git_dirty(&p);
                out.push(Repo { path: p, name, dirty });
            }
        }
    }
    out.sort_by(|a, b| b.dirty.cmp(&a.dirty).then(a.name.cmp(&b.name)));
    out
}

fn git_dirty(repo: &std::path::Path) -> usize {
    let o = Command::new("git").arg("-C").arg(repo).args(["status", "--porcelain"]).output();
    match o {
        Ok(o) if o.status.success() => parse_porcelain(&String::from_utf8_lossy(&o.stdout)),
        _ => 0,
    }
}

pub fn recent_commits(repo: &std::path::Path, n: usize) -> Vec<Commit> {
    let o = Command::new("git")
        .arg("-C").arg(repo)
        .args(["log", &format!("-{n}"), "--format=%h %s", "--no-merges"])
        .output();
    match o {
        Ok(o) if o.status.success() => parse_log(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}
```

- [ ] **Step 6: Build + commit**

Run: `cargo build -p gst`
Expected: builds clean.
```bash
git add Cargo.toml gst/ && git commit -m "feat(gst): repo discovery + git status/log parsing"
```

---

## Task 8: gst `--summary --json` (TDD)

**Files:**
- Create: `~/projects/launchers/gst/src/summary_cmd.rs`
- Modify: `~/projects/launchers/gst/src/main.rs`

- [ ] **Step 1: Write the summary builder + test**

`gst/src/summary_cmd.rs`:
```rust
//! Builds the gst Summary envelope from discovered repos (no TUI).
use crate::source::Repo;
use launcher_core::summary::Summary;

pub fn build_summary(repos: &[Repo]) -> Summary {
    let dirty: Vec<&Repo> = repos.iter().filter(|r| r.dirty > 0).collect();
    let headline = format!("{} dirty", dirty.len());
    let items: Vec<String> = dirty
        .iter()
        .take(3)
        .map(|r| format!("{} ({})", r.name, r.dirty))
        .collect();
    Summary::new("gst", headline, items, repos.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn summarizes_dirty_repos() {
        let repos = vec![
            Repo { path: PathBuf::from("/x/a"), name: "a".into(), dirty: 3 },
            Repo { path: PathBuf::from("/x/b"), name: "b".into(), dirty: 0 },
        ];
        let s = build_summary(&repos);
        assert_eq!(s.headline, "1 dirty");
        assert_eq!(s.count, 2);
        assert_eq!(s.items, vec!["a (3)".to_string()]);
    }
}
```

- [ ] **Step 2: Wire the dispatch in main.rs**

`gst/src/main.rs`:
```rust
mod source;
mod summary_cmd;
mod app;

use launcher_core::summary::wants_summary;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if wants_summary(&args) {
        let repos = source::discover_repos();
        println!("{}", summary_cmd::build_summary(&repos).emit_json());
        return Ok(());
    }
    app::run()
}
```

(Note: `app` is created in Task 9; add a stub `gst/src/app.rs` with `pub fn run() -> anyhow::Result<()> { Ok(()) }` so this compiles now.)

- [ ] **Step 3: Run tests + smoke the summary path**

Run: `cargo test -p gst summary_cmd`
Expected: 1 passed.
Run: `cargo run -p gst -- --summary --json`
Expected: a single JSON line like `{"launcher":"gst","headline":"N dirty","items":[...],"count":M}`.

- [ ] **Step 4: Commit**

```bash
git add gst/ && git commit -m "feat(gst): --summary --json output"
```

---

## Task 9: gst interactive TUI

**Files:**
- Modify: `~/projects/launchers/gst/src/app.rs`

- [ ] **Step 1: Write the interactive app**

`gst/src/app.rs`:
```rust
//! Interactive gst: list repos (dirty first), filter, act with exit-with-command.
use crate::source::{discover_repos, recent_commits, Repo};
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use launcher_core::exit::{finish, RunOutcome};
use launcher_core::filter::filter_indices;
use launcher_core::list::ListState;
use launcher_core::theme;
use launcher_core::ui::{render_footer, three_row, Toast};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::Duration;

const FOOTER: &str = "j/k move  / filter  o cd  y copy sha  q quit";

struct State {
    repos: Vec<Repo>,
    visible: Vec<usize>,
    list: ListState,
    query: String,
    searching: bool,
    toast: Toast,
}

impl State {
    fn new() -> Self {
        let repos = discover_repos();
        let visible: Vec<usize> = (0..repos.len()).collect();
        let list = ListState::new(visible.len());
        Self { repos, visible, list, query: String::new(), searching: false, toast: Toast::new() }
    }
    fn refilter(&mut self) {
        self.visible = filter_indices(&self.repos, &self.query, |r| r.name.as_str());
        self.list.set_len(self.visible.len());
    }
    fn focused(&self) -> Option<&Repo> {
        self.visible.get(self.list.selected).map(|&i| &self.repos[i])
    }
}

pub fn run() -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut term = ratatui::Terminal::new(backend)?;
    let mut state = State::new();
    let mut outcome = RunOutcome::Quit;

    loop {
        term.draw(|f| draw(f, &state))?;
        if !event::poll(Duration::from_millis(200))? { continue; }
        let Event::Key(k) = event::read()? else { continue };
        if state.searching {
            match k.code {
                KeyCode::Esc => { state.searching = false; }
                KeyCode::Enter => { state.searching = false; }
                KeyCode::Backspace => { state.query.pop(); state.refilter(); }
                KeyCode::Char(c) => { state.query.push(c); state.refilter(); }
                _ => {}
            }
            continue;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('j') | KeyCode::Down => state.list.down(),
            KeyCode::Char('k') | KeyCode::Up => state.list.up(),
            KeyCode::Char('/') => { state.searching = true; }
            KeyCode::Char('o') => {
                if let Some(r) = state.focused() {
                    outcome = RunOutcome::PrintAndExit(format!("cd {}", r.path.display()));
                    break;
                }
            }
            KeyCode::Char('y') => {
                if let Some(r) = state.focused() {
                    if let Some(c) = recent_commits(&r.path, 1).first() {
                        outcome = RunOutcome::PrintAndExit(c.sha.clone());
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    crossterm::execute!(std::io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    finish(outcome);
    Ok(())
}

fn draw(f: &mut Frame, state: &State) {
    let [head, body, foot] = three_row(f.area());
    let title = if state.searching {
        format!(" gst  /{}", state.query)
    } else {
        format!(" gst  {} repos", state.repos.len())
    };
    f.render_widget(Paragraph::new(Line::from(Span::styled(title, theme::header()))), head);

    let lines: Vec<Line> = state
        .visible
        .iter()
        .enumerate()
        .map(|(row, &i)| {
            let r = &state.repos[i];
            let marker = if row == state.list.selected { "> " } else { "  " };
            let style = if row == state.list.selected { theme::active_row() } else { theme::dim() };
            Line::from(vec![Span::styled(
                format!("{marker}{}  {} dirty", r.name, r.dirty),
                style,
            )])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), body);

    render_footer(f, foot, FOOTER, &state.toast);
}
```

- [ ] **Step 2: Build**

Run: `cargo build -p gst`
Expected: builds clean.

- [ ] **Step 3: Smoke test under a pty**

Run:
```bash
cd ~/projects/launchers && cargo build --release -p gst
{ sleep 0.5; printf 'j'; sleep 0.3; printf 'q'; } | env COLUMNS=80 LINES=24 script -q -c './target/release/gst' /tmp/gst.log >/dev/null
grep -i 'gst' /tmp/gst.log | head -1
```
Expected: output contains the `gst N repos` header. No panic.

- [ ] **Step 4: Verify exit-with-command**

Run: `{ sleep 0.5; printf 'o'; } | ./target/release/gst` (in a real terminal)
Expected: prints a `cd <path>` line to stdout after exit; `eval "$(...)"` would change directory.

- [ ] **Step 5: Commit**

```bash
git add gst/src/app.rs && git commit -m "feat(gst): interactive repo picker with exit-with-command"
```

---

## Task 10: install script

**Files:**
- Create: `~/projects/launchers/install.sh`

- [ ] **Step 1: Write install.sh**

`~/projects/launchers/install.sh`:
```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
mkdir -p "$HOME/.local/bin"
for bin in gst; do
  install -m 0755 "target/release/$bin" "$HOME/.local/bin/$bin"
  echo "installed $bin"
done
```

- [ ] **Step 2: Install and verify**

Run:
```bash
chmod +x ~/projects/launchers/install.sh && ~/projects/launchers/install.sh
gst --summary --json
```
Expected: prints "installed gst" then a JSON summary line.

- [ ] **Step 3: Commit**

```bash
git add install.sh && git commit -m "chore: install script for launcher binaries"
```

---

## Task 11: glance `launchers` palette panel

**Files:**
- Create: `~/projects/glance/src/panels/launchers.rs`
- Modify: `~/projects/glance/src/panels/mod.rs`

- [ ] **Step 1: Write the panel with the static palette table**

`~/projects/glance/src/panels/launchers.rs`:
```rust
//! Quick-reference palette of the launcher family + live cards (Wave 0: gst).
//! Vertical, single-column, mobile-first. Card data is fetched by shelling out
//! to `<bin> --summary --json` on a background thread (weather/commits pattern).
use crate::panels::Panel;
use crate::theme;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// (name, description, shortcut). The full family; cards exist only for some.
const PALETTE: &[(&str, &str, char)] = &[
    ("gst", "git status/log", 'g'),
    ("clip", "clipboard", 'c'),
    ("op", "1password", 'o'),
    ("proc", "processes", 'p'),
    ("docker", "containers", 'd'),
    ("svc", "services", 's'),
    ("ssh", "hosts", 'h'),
    ("note", "journal", 'n'),
    ("gh", "PR triage", 'G'),
    ("port", "listeners", 't'),
    ("agent", "AI sessions", 'a'),
    ("hub", "hubspot portals", 'b'),
];

pub struct LaunchersPanel {
    gst_card: Option<String>,
    last_kick: Option<Instant>,
    rx: mpsc::Receiver<Option<String>>,
    tx: mpsc::Sender<Option<String>>,
    inflight: Arc<Mutex<bool>>,
}

impl LaunchersPanel {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { gst_card: None, last_kick: None, rx, tx, inflight: Arc::new(Mutex::new(false)) }
    }

    fn kick(&mut self) {
        let mut g = match self.inflight.lock() { Ok(g) => g, Err(_) => return };
        if *g { return; }
        *g = true;
        drop(g);
        let tx = self.tx.clone();
        let inflight = Arc::clone(&self.inflight);
        thread::spawn(move || {
            let out = Command::new("gst").args(["--summary", "--json"]).output();
            let headline = out.ok().filter(|o| o.status.success()).and_then(|o| {
                let v: serde_json::Value = serde_json::from_slice(&o.stdout).ok()?;
                Some(v.get("headline")?.as_str()?.to_string())
            });
            let _ = tx.send(headline);
            if let Ok(mut g) = inflight.lock() { *g = false; }
        });
        self.last_kick = Some(Instant::now());
    }
}

impl Panel for LaunchersPanel {
    fn name(&self) -> &str { "launchers" }
    fn refresh_ms(&self) -> u64 { 5_000 }

    fn tick(&mut self) {
        while let Ok(card) = self.rx.try_recv() { self.gst_card = card; }
        let stale = match self.last_kick {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(60),
        };
        if stale { self.kick(); }
    }

    fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),                              // title
                Constraint::Length(PALETTE.len() as u16),          // palette
                Constraint::Length(1),                             // divider
                Constraint::Min(0),                                // cards
            ])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" launchers", theme::pane_header()))),
            chunks[0],
        );

        let rows: Vec<Line> = PALETTE
            .iter()
            .map(|(name, desc, key)| {
                Line::from(vec![
                    Span::styled(format!("  {name:<7}"), theme::pane_header_focused()),
                    Span::styled(format!("{desc:<16}"), theme::dim()),
                    Span::styled(format!("[{key}]"), theme::historical()),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(rows), chunks[1]);

        f.render_widget(
            Paragraph::new(Line::from(Span::styled(" ──────────────", theme::dim()))),
            chunks[2],
        );

        let card = match &self.gst_card {
            Some(h) => format!(" gst · {h}"),
            None => " gst · …".to_string(),
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(card, theme::now()))),
            chunks[3],
        );
    }
}
```

- [ ] **Step 2: Register the panel**

In `~/projects/glance/src/panels/mod.rs`:
- Add `pub mod launchers;` with the other `pub mod` lines.
- Add to `build_panel`'s match: `"launchers" => Box::new(launchers::LaunchersPanel::new()),`
- Add `"launchers"` to `DEFAULT_ORDER` and `ALL_PANELS`.

- [ ] **Step 3: Build glance**

Run: `cd ~/projects/glance && cargo build --release`
Expected: builds clean.

- [ ] **Step 4: Commit (in the glance repo)**

```bash
cd ~/projects/glance && git add src/panels/launchers.rs src/panels/mod.rs && git commit -m "feat: launchers palette panel with gst live card"
```

---

## Task 12: smoke-test the glance panel + gst card end to end

**Files:** none (verification only)

- [ ] **Step 1: Install both and capture the panel**

Run:
```bash
~/projects/launchers/install.sh
cd ~/projects/glance && cargo build --release && install -m 0755 target/release/glance ~/.local/bin/glance
tmux kill-session -t lx 2>/dev/null; tmux new-session -d -s lx -x 40 -y 40 'glance --only launchers 2>/dev/null || glance'
sleep 3
tmux capture-pane -t lx -p | sed -n '1,30p'
tmux send-keys -t lx 'q'; tmux kill-session -t lx 2>/dev/null
```
Expected: the `launchers` panel lists all 12 entries in a single column and the `gst · N dirty` card shows a real count (matching `gst --summary --json`).

- [ ] **Step 2: Verify mobile width**

Run: same capture at `-x 32`.
Expected: palette still single-column and readable; the card line present (may be the only card row).

- [ ] **Step 3: Final commit if any tweaks were needed**

```bash
cd ~/projects/glance && git add -A && git commit -m "test: verify launchers panel renders gst card" || echo "no changes"
```

---

## Wave 0 done when

- `cargo test` passes in the launchers workspace (clipboard, summary, filter, list, gst source, gst summary_cmd).
- `gst` installed; interactive picker works; `o` prints `cd <path>`; `y` prints a SHA; `--summary --json` emits a valid envelope.
- glance shows the `launchers` palette (12 entries) + a live `gst` card, single-column at 32 cols.
- This stabilizes the `launcher-core` API. Waves 1 to 3 (clip/op/proc, docker/svc/hub, ssh/note/gh/port/agent) get their own plans, each adding a `source.rs` + `--summary --json` + interactive `app.rs` per launcher and a card in the glance panel.
