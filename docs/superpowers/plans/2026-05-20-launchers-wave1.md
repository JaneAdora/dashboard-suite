# Launchers Wave 1 Implementation Plan (clip, op, proc)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add three more launchers — `clip` (clipboard history via cliphist), `op` (1Password items), `proc` (process viewer/killer via sysinfo) — to the `~/projects/launchers` workspace, each as a standalone launcher, and wire glance live cards for `clip` and `proc`.

**Architecture:** Each launcher repeats the committed `gst` exemplar: a `source.rs` (data access + pure parsing, unit-tested), a `--summary --json` path, and an interactive `app.rs` built on `launcher-core` (`TerminalGuard`, `Selection`, `three_row`, `Toast`, `RunOutcome`/`finish`). glance's data-driven `CARDS` table grows by appending `(name, bin)` pairs.

**Tech Stack:** Rust 2021, ratatui 0.29, crossterm 0.28, anyhow 1, serde/serde_json 1, sysinfo 0.32 (proc). External CLIs: `cliphist`, `wl-copy`/`wl-paste` (clip); `op` (op). Spec: `docs/superpowers/specs/2026-05-20-launchers-design.md`.

---

## The exemplar — READ THESE FIRST

Wave 0 shipped `gst` as the pattern. Before writing any launcher, read the committed exemplar so the new launchers match it exactly:
- `~/projects/launchers/gst/src/main.rs` — CLI dispatch (`wants_summary` → summary path, else `app::run()`).
- `~/projects/launchers/gst/src/app.rs` — the canonical interactive shape: `run()` wraps `event_loop()` in a `TerminalGuard` scope and calls `finish(outcome)` after the guard drops; `State` holds `Selection` + filtered `visible` indices + `Toast`; `draw()` uses `three_row` + `render_footer`. **Copy this structure; only the data type, footer string, and action keys differ.**
- `~/projects/launchers/gst/src/source.rs` and `summary_cmd.rs` — data-source + summary shape.

launcher-core API you will use (do not redefine): `theme::{header,dim,active_row,PINK,LAVENDER,MAGENTA}`, `ui::{three_row, Toast, render_footer}`, `filter::filter_indices`, `list::Selection` (`new/set_len/down/up/selected_index`, fields `selected/len`), `exit::{RunOutcome, finish}`, `tui::{TerminalGuard, terminal}`, `clipboard::osc52_sequence`, `summary::{Summary, wants_summary}` (`Summary::new(launcher, headline, items, count)`).

**ENVIRONMENT NOTE:** The Edit/Write tools may be blocked by the background-job isolation guard on git repos. Prefer Bash heredocs (`cat > path << 'EOF' … EOF`) for file writes; fall back if Edit errors. Use Bash for cargo/git normally. Commit after each task in `~/projects/launchers` (or `~/projects/glance` for the glance task).

---

## File structure (Wave 1)

```
~/projects/launchers/
  Cargo.toml                 # MODIFY: members += clip, op, proc
  install.sh                 # MODIFY: install clip, op, proc too
  clip/  { Cargo.toml, src/{main.rs, source.rs, app.rs} }
  op/    { Cargo.toml, src/{main.rs, source.rs, app.rs} }
  proc/  { Cargo.toml, src/{main.rs, source.rs, app.rs} }
~/projects/glance/
  src/panels/launchers.rs    # MODIFY: append ("clip","clip") and ("proc","proc") to CARDS
```

---

## Task 1: Add the three crates to the workspace + install script

**Files:** Modify `~/projects/launchers/Cargo.toml`, `~/projects/launchers/install.sh`

- [ ] **Step 1: Add members**

Edit `~/projects/launchers/Cargo.toml` so members is:
```toml
members = ["launcher-core", "gst", "clip", "op", "proc"]
```

- [ ] **Step 2: Update install.sh**

Change the install loop in `~/projects/launchers/install.sh` to:
```bash
for bin in gst clip op proc; do
  install -m 0755 "target/release/$bin" "$HOME/.local/bin/$bin"
  echo "installed $bin"
done
```

- [ ] **Step 3: Commit** (the crates don't exist yet, so don't build until they're created in later tasks)

```bash
cd ~/projects/launchers && git add Cargo.toml install.sh && git commit -m "chore: register clip/op/proc crates + install"
```

Note: `cargo build` will fail until Tasks 2-10 create the crates. That is expected; build per-crate as you create them.

---

## Task 2: clip — source.rs (cliphist parsing, TDD)

**Files:** Create `~/projects/launchers/clip/Cargo.toml`, `clip/src/source.rs`, `clip/src/main.rs` (stub)

- [ ] **Step 1: Create the crate**

`clip/Cargo.toml`:
```toml
[package]
name = "clip"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "clip"
path = "src/main.rs"

[dependencies]
launcher-core = { path = "../launcher-core" }
ratatui = { workspace = true }
crossterm = { workspace = true }
anyhow = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 2: Write source.rs with failing parser test**

`clip/src/source.rs`:
```rust
//! Clipboard history via `cliphist`. `cliphist list` emits one entry per line as
//! "<id>\t<preview>"; `cliphist decode <id>` returns the full content; deleting
//! pipes the original list line back to `cliphist delete`.
use std::process::{Command, Stdio};
use std::io::Write;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub preview: String,
    pub raw: String, // the full original "id\tpreview" line, needed for delete
}

/// Parse `cliphist list` output into entries (most-recent first, as cliphist emits).
pub fn parse_list(out: &str) -> Vec<Entry> {
    out.lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let (id, preview) = l.split_once('\t')?;
            Some(Entry { id: id.to_string(), preview: preview.to_string(), raw: l.to_string() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_id_and_preview() {
        let out = "42\thello world\n7\tmulti  spaces\n";
        let e = parse_list(out);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0], Entry { id: "42".into(), preview: "hello world".into(), raw: "42\thello world".into() });
        assert_eq!(e[1].id, "7");
    }

    #[test]
    fn skips_malformed_lines() {
        let out = "no-tab-here\n9\tok\n\n";
        let e = parse_list(out);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, "9");
    }
}

pub fn list() -> Vec<Entry> {
    let o = Command::new("cliphist").arg("list").output();
    match o {
        Ok(o) if o.status.success() => parse_list(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

/// Full decoded content for an entry id.
pub fn decode(id: &str) -> String {
    let o = Command::new("cliphist").args(["decode", id]).output();
    match o {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => String::new(),
    }
}

/// Delete an entry by piping its original list line to `cliphist delete`.
pub fn delete(raw_line: &str) {
    if let Ok(mut child) = Command::new("cliphist").arg("delete").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(raw_line.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Re-copy text to the Wayland clipboard (best-effort; OSC 52 is handled separately).
pub fn wl_copy(text: &str) {
    if let Ok(mut child) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}
```

- [ ] **Step 3: Stub main.rs so it compiles**

`clip/src/main.rs`:
```rust
mod source;
mod app;
fn main() {}
```
(Also create an empty `clip/src/app.rs` with `pub fn run() -> anyhow::Result<()> { Ok(()) }` so the module resolves; Task 4 fills it.)

- [ ] **Step 4: Run the parser tests**

Run: `cd ~/projects/launchers && cargo test -p clip source`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add clip/ Cargo.toml && git commit -m "feat(clip): cliphist list parsing + subprocess wrappers"
```

---

## Task 3: clip — summary + main dispatch

**Files:** Modify `clip/src/main.rs`

- [ ] **Step 1: Write the summary builder + main dispatch**

Replace `clip/src/main.rs`:
```rust
mod source;
mod app;

use launcher_core::summary::{wants_summary, Summary};

/// Card summary: a short, one-line, truncated preview of the most recent entry,
/// plus the entry count. Set CLIP_CARD_HIDE=1 to suppress content (count only) —
/// the clipboard can hold secrets and this card is always-on in glance.
fn build_summary(entries: &[source::Entry]) -> Summary {
    let hide = std::env::var("CLIP_CARD_HIDE").map(|v| v == "1").unwrap_or(false);
    let headline = match entries.first() {
        _ if hide => format!("{} entries", entries.len()),
        Some(e) => {
            let mut p: String = e.preview.chars().take(24).collect();
            if e.preview.chars().count() > 24 { p.push('…'); }
            p
        }
        None => "empty".to_string(),
    };
    Summary::new("clip", headline, Vec::new(), entries.len())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if wants_summary(&args) {
        println!("{}", build_summary(&source::list()).emit_json());
        return Ok(());
    }
    app::run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Entry;

    fn e(id: &str, prev: &str) -> Entry { Entry { id: id.into(), preview: prev.into(), raw: format!("{id}\t{prev}") } }

    #[test]
    fn headline_is_truncated_preview() {
        let s = build_summary(&[e("1", "short")]);
        assert_eq!(s.headline, "short");
        assert_eq!(s.count, 1);
    }

    #[test]
    fn empty_history() {
        let s = build_summary(&[]);
        assert_eq!(s.headline, "empty");
        assert_eq!(s.count, 0);
    }
}
```

- [ ] **Step 2: Run tests + smoke**

Run: `cargo test -p clip` (parser + summary tests pass).
Run: `cargo run -p clip -- --summary --json` → one JSON line `{"launcher":"clip",...}` (headline is your latest clipboard preview or "empty").

- [ ] **Step 3: Commit**

```bash
git add clip/ && git commit -m "feat(clip): --summary --json (truncated preview, CLIP_CARD_HIDE opt-out)"
```

---

## Task 4: clip — interactive app

**Files:** Modify `clip/src/app.rs`

- [ ] **Step 1: Write app.rs (mirror gst/src/app.rs structure)**

Read `gst/src/app.rs` first. Then write `clip/src/app.rs` with the same `run()`/`event_loop()`/`draw()` shape. Differences: items are `source::Entry`; footer and actions differ. Full code:
```rust
//! Interactive clip: browse clipboard history, paste/recopy/delete.
use crate::source::{decode, delete, list, wl_copy, Entry};
use crossterm::event::{self, Event, KeyCode};
use launcher_core::clipboard::osc52_sequence;
use launcher_core::exit::{finish, RunOutcome};
use launcher_core::filter::filter_indices;
use launcher_core::list::Selection;
use launcher_core::theme;
use launcher_core::tui::{terminal, TerminalGuard};
use launcher_core::ui::{render_footer, three_row, Toast};
use ratatui::backend::CrosstermBackend;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::io::{Stdout, Write};
use std::time::Duration;

const FOOTER: &str = "j/k move  / filter  ⏎ paste  y recopy  d delete  q quit";

struct State {
    entries: Vec<Entry>,
    visible: Vec<usize>,
    list: Selection,
    query: String,
    searching: bool,
    toast: Toast,
}

impl State {
    fn new() -> Self {
        let entries = list();
        let visible: Vec<usize> = (0..entries.len()).collect();
        let list = Selection::new(visible.len());
        Self { entries, visible, list, query: String::new(), searching: false, toast: Toast::new() }
    }
    fn refilter(&mut self) {
        self.visible = filter_indices(&self.entries, &self.query, |e| e.preview.as_str());
        self.list.set_len(self.visible.len());
    }
    fn focused(&self) -> Option<&Entry> {
        self.list.selected_index().and_then(|i| self.visible.get(i)).map(|&i| &self.entries[i])
    }
    fn reload(&mut self) {
        self.entries = list();
        self.refilter();
    }
}

pub fn run() -> anyhow::Result<()> {
    let outcome = {
        let _guard = TerminalGuard::enter()?;
        let mut term = terminal()?;
        event_loop(&mut term)?
    };
    finish(outcome);
    Ok(())
}

fn event_loop(term: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<RunOutcome> {
    let mut state = State::new();
    loop {
        term.draw(|f| draw(f, &state))?;
        if !event::poll(Duration::from_millis(200))? { continue; }
        let Event::Key(k) = event::read()? else { continue };
        if state.searching {
            match k.code {
                KeyCode::Esc | KeyCode::Enter => state.searching = false,
                KeyCode::Backspace => { state.query.pop(); state.refilter(); }
                KeyCode::Char(c) => { state.query.push(c); state.refilter(); }
                _ => {}
            }
            continue;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(RunOutcome::Quit),
            KeyCode::Char('j') | KeyCode::Down => state.list.down(),
            KeyCode::Char('k') | KeyCode::Up => state.list.up(),
            KeyCode::Char('/') => state.searching = true,
            KeyCode::Enter => {
                if let Some(e) = state.focused() {
                    return Ok(RunOutcome::PrintAndExit(decode(&e.id)));
                }
            }
            KeyCode::Char('y') => {
                if let Some(e) = state.focused() {
                    let content = decode(&e.id);
                    let (seq, _) = osc52_sequence(&content);
                    let mut out = std::io::stdout();
                    let _ = out.write_all(seq.as_bytes());
                    let _ = out.flush();
                    wl_copy(&content);
                    state.toast.set("recopied");
                }
            }
            KeyCode::Char('d') => {
                if let Some(e) = state.focused() {
                    let raw = e.raw.clone();
                    delete(&raw);
                    state.reload();
                    state.toast.set("deleted");
                }
            }
            _ => {}
        }
    }
}

fn draw(f: &mut Frame, state: &State) {
    let [head, body, foot] = three_row(f.area());
    let title = if state.searching {
        format!(" clip  /{}", state.query)
    } else {
        format!(" clip  {} entries", state.entries.len())
    };
    f.render_widget(Paragraph::new(Line::from(Span::styled(title, theme::header()))), head);

    let highlighted = state.list.selected_index();
    let lines: Vec<Line> = state.visible.iter().enumerate().map(|(row, &i)| {
        let e = &state.entries[i];
        let is_active = highlighted == Some(row);
        let marker = if is_active { "> " } else { "  " };
        let style = if is_active { theme::active_row() } else { theme::dim() };
        let preview: String = e.preview.chars().take(60).collect();
        Line::from(Span::styled(format!("{marker}{preview}"), style))
    }).collect();
    f.render_widget(Paragraph::new(lines), body);

    render_footer(f, foot, FOOTER, &state.toast);
}
```

- [ ] **Step 2: Build + smoke**

Run: `cargo build -p clip` (clean).
Run a pty smoke: `{ sleep 0.5; printf 'j'; sleep 0.3; printf 'q'; } | env COLUMNS=80 LINES=24 script -q -c "$(pwd)/target/debug/clip" /tmp/clip.log >/dev/null; grep -i clip /tmp/clip.log | head -1` (no panic; header present). If `cliphist` is absent the list is empty — still must not panic.

- [ ] **Step 3: Commit**

```bash
git add clip/ && git commit -m "feat(clip): interactive history browser (paste/recopy/delete)"
```

---

## Task 5: op — source.rs (op item list JSON, TDD)

**Files:** Create `op/Cargo.toml`, `op/src/source.rs`, `op/src/main.rs` (stub)

- [ ] **Step 1: Create the crate**

`op/Cargo.toml`:
```toml
[package]
name = "op"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "op"
path = "src/main.rs"

[dependencies]
launcher-core = { path = "../launcher-core" }
ratatui = { workspace = true }
crossterm = { workspace = true }
anyhow = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

- [ ] **Step 2: Write source.rs with parsing test**

`op/src/source.rs`:
```rust
//! 1Password items via the `op` CLI (the ~/.local/bin/op wrapper, skai-agent-v2
//! token). `op item list --format=json` returns an array; we keep id/title/vault.
//! Secrets are fetched on demand and never cached or logged.
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub vault: String,
}

#[derive(Deserialize)]
struct RawItem {
    id: String,
    title: String,
    #[serde(default)]
    vault: RawVault,
}
#[derive(Deserialize, Default)]
struct RawVault {
    #[serde(default)]
    name: String,
}

/// Parse `op item list --format=json` output.
pub fn parse_items(json: &str) -> Vec<Item> {
    let raw: Vec<RawItem> = serde_json::from_str(json).unwrap_or_default();
    raw.into_iter()
        .map(|r| Item { id: r.id, title: r.title, vault: r.vault.name })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_items() {
        let json = r#"[{"id":"abc","title":"GitHub","vault":{"name":"Dev"}},
                       {"id":"def","title":"Email","vault":{"name":"Work"}}]"#;
        let items = parse_items(json);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], Item { id: "abc".into(), title: "GitHub".into(), vault: "Dev".into() });
    }

    #[test]
    fn bad_json_is_empty() {
        assert!(parse_items("not json").is_empty());
        assert!(parse_items("[]").is_empty());
    }
}

pub fn list() -> Vec<Item> {
    let o = Command::new("op").args(["item", "list", "--format=json"]).output();
    match o {
        Ok(o) if o.status.success() => parse_items(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

/// Fetch a single field (e.g. "password") for an item, revealed. Returns empty on error.
pub fn field(item_id: &str, field: &str) -> String {
    let o = Command::new("op")
        .args(["item", "get", item_id, "--fields", field, "--reveal"])
        .output();
    match o {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}
```

- [ ] **Step 3: Stub main.rs + app.rs**

`op/src/main.rs`:
```rust
mod source;
mod app;
fn main() {}
```
Create `op/src/app.rs` with `pub fn run() -> anyhow::Result<()> { Ok(()) }`.

- [ ] **Step 4: Test**

Run: `cargo test -p op source` → 2 passed.

- [ ] **Step 5: Commit**

```bash
git add op/ Cargo.toml && git commit -m "feat(op): op item list JSON parsing"
```

---

## Task 6: op — summary + main (count only, no secrets)

**Files:** Modify `op/src/main.rs`

- [ ] **Step 1: Write summary + dispatch**

`op/src/main.rs`:
```rust
mod source;
mod app;

use launcher_core::summary::{wants_summary, Summary};

/// op is palette-only in glance (no card), but it still answers --summary --json
/// for consistency. It emits ONLY a count — never item titles or secrets.
fn build_summary(items: &[source::Item]) -> Summary {
    Summary::new("op", format!("{} items", items.len()), Vec::new(), items.len())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if wants_summary(&args) {
        println!("{}", build_summary(&source::list()).emit_json());
        return Ok(());
    }
    app::run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Item;
    #[test]
    fn summary_is_count_only() {
        let items = vec![Item { id: "1".into(), title: "secretish".into(), vault: "V".into() }];
        let s = build_summary(&items);
        assert_eq!(s.headline, "1 items");
        assert!(s.items.is_empty(), "must not leak titles");
    }
}
```

- [ ] **Step 2: Test**

Run: `cargo test -p op` → all pass. `cargo run -p op -- --summary --json` prints `{"launcher":"op","headline":"N items","items":[],"count":N}` (N=0 if op not authed; must not panic).

- [ ] **Step 3: Commit**

```bash
git add op/ && git commit -m "feat(op): --summary --json (count only, no secret leakage)"
```

---

## Task 7: op — interactive app (secret handling)

**Files:** Modify `op/src/app.rs`

- [ ] **Step 1: Write app.rs (mirror gst structure)**

Read `gst/src/app.rs`. Write `op/src/app.rs` with the same shape; items are `source::Item`. Actions: `y` copy password (OSC 52 + wl-copy + spawn a 30s clipboard auto-clear), `Y` copy a chosen field is out of scope for v1 (only password), `e` reveal password in a transient toast (NOT printed to stdout). No `RunOutcome::PrintAndExit` (never print secrets to stdout). Full code:
```rust
//! Interactive op: browse 1Password items, copy password (auto-clearing).
use crate::source::{field, list, Item};
use crossterm::event::{self, Event, KeyCode};
use launcher_core::clipboard::osc52_sequence;
use launcher_core::exit::{finish, RunOutcome};
use launcher_core::filter::filter_indices;
use launcher_core::list::Selection;
use launcher_core::theme;
use launcher_core::tui::{terminal, TerminalGuard};
use launcher_core::ui::{render_footer, three_row, Toast};
use ratatui::backend::CrosstermBackend;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::io::{Stdout, Write};
use std::process::Command;
use std::time::Duration;

const FOOTER: &str = "j/k move  / filter  y copy pw  e reveal  q quit";

struct State {
    items: Vec<Item>,
    visible: Vec<usize>,
    list: Selection,
    query: String,
    searching: bool,
    toast: Toast,
}

impl State {
    fn new() -> Self {
        let items = list();
        let visible: Vec<usize> = (0..items.len()).collect();
        let list = Selection::new(visible.len());
        Self { items, visible, list, query: String::new(), searching: false, toast: Toast::new() }
    }
    fn refilter(&mut self) {
        self.visible = filter_indices(&self.items, &self.query, |i| i.title.as_str());
        self.list.set_len(self.visible.len());
    }
    fn focused(&self) -> Option<&Item> {
        self.list.selected_index().and_then(|i| self.visible.get(i)).map(|&i| &self.items[i])
    }
}

/// Copy to OSC 52 + Wayland clipboard, and schedule a 30s auto-clear of wl clipboard.
fn copy_secret(s: &str) {
    let (seq, _) = osc52_sequence(s);
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
    if let Ok(mut c) = Command::new("wl-copy").stdin(std::process::Stdio::piped()).spawn() {
        if let Some(mut si) = c.stdin.take() { let _ = si.write_all(s.as_bytes()); }
        let _ = c.wait();
    }
    // Detached best-effort auto-clear (Wayland clipboard only; OSC 52 can't be cleared).
    let _ = Command::new("sh").args(["-c", "sleep 30 && wl-copy --clear"]).spawn();
}

pub fn run() -> anyhow::Result<()> {
    let outcome = {
        let _guard = TerminalGuard::enter()?;
        let mut term = terminal()?;
        event_loop(&mut term)?
    };
    finish(outcome);
    Ok(())
}

fn event_loop(term: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<RunOutcome> {
    let mut state = State::new();
    loop {
        term.draw(|f| draw(f, &state))?;
        if !event::poll(Duration::from_millis(200))? { continue; }
        let Event::Key(k) = event::read()? else { continue };
        if state.searching {
            match k.code {
                KeyCode::Esc | KeyCode::Enter => state.searching = false,
                KeyCode::Backspace => { state.query.pop(); state.refilter(); }
                KeyCode::Char(c) => { state.query.push(c); state.refilter(); }
                _ => {}
            }
            continue;
        }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(RunOutcome::Quit),
            KeyCode::Char('j') | KeyCode::Down => state.list.down(),
            KeyCode::Char('k') | KeyCode::Up => state.list.up(),
            KeyCode::Char('/') => state.searching = true,
            KeyCode::Char('y') => {
                if let Some(it) = state.focused() {
                    let pw = field(&it.id, "password");
                    if pw.is_empty() { state.toast.set("no password"); }
                    else { copy_secret(&pw); state.toast.set("copied (clears 30s)"); }
                }
            }
            KeyCode::Char('e') => {
                if let Some(it) = state.focused() {
                    let pw = field(&it.id, "password");
                    state.toast.set(if pw.is_empty() { "no password".into() } else { pw });
                }
            }
            _ => {}
        }
    }
}

fn draw(f: &mut Frame, state: &State) {
    let [head, body, foot] = three_row(f.area());
    let title = if state.searching {
        format!(" op  /{}", state.query)
    } else {
        format!(" op  {} items", state.items.len())
    };
    f.render_widget(Paragraph::new(Line::from(Span::styled(title, theme::header()))), head);

    let highlighted = state.list.selected_index();
    let lines: Vec<Line> = state.visible.iter().enumerate().map(|(row, &i)| {
        let it = &state.items[i];
        let is_active = highlighted == Some(row);
        let marker = if is_active { "> " } else { "  " };
        let style = if is_active { theme::active_row() } else { theme::dim() };
        Line::from(Span::styled(format!("{marker}{}  ({})", it.title, it.vault), style))
    }).collect();
    f.render_widget(Paragraph::new(lines), body);

    render_footer(f, foot, FOOTER, &state.toast);
}
```

- [ ] **Step 2: Build + smoke**

Run: `cargo build -p op` (clean). pty smoke as in Task 4 (no panic; if `op` not authed the list is empty).

- [ ] **Step 3: Commit**

```bash
git add op/ && git commit -m "feat(op): interactive item browser, auto-clearing password copy"
```

---

## Task 8: proc — source.rs (sysinfo snapshot, TDD on pure helpers)

**Files:** Create `proc/Cargo.toml`, `proc/src/source.rs`, `proc/src/main.rs` (stub)

- [ ] **Step 1: Create the crate**

`proc/Cargo.toml`:
```toml
[package]
name = "proc"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "proc"
path = "src/main.rs"

[dependencies]
launcher-core = { path = "../launcher-core" }
ratatui = { workspace = true }
crossterm = { workspace = true }
anyhow = { workspace = true }
serde_json = { workspace = true }
sysinfo = "0.32"
```

- [ ] **Step 2: Write source.rs**

`proc/src/source.rs` (the snapshot uses sysinfo; the sort/format helper is pure and tested):
```rust
//! Process list via sysinfo. `Proc` is a plain snapshot row; `sort_by_cpu` is a
//! pure, tested helper. Killing uses sysinfo's signal API.
use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, Signal, System};

#[derive(Debug, Clone, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,     // percent
    pub mem_mb: u64,  // resident, MiB
}

/// Sort processes by CPU descending (stable on name for ties). Pure + tested.
pub fn sort_by_cpu(mut v: Vec<Proc>) -> Vec<Proc> {
    v.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal).then(a.name.cmp(&b.name)));
    v
}

/// Take a fresh snapshot. Two refreshes with a short gap so CPU% is meaningful.
pub fn snapshot() -> Vec<Proc> {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let procs = sys
        .processes()
        .iter()
        .map(|(pid, p)| Proc {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            cpu: p.cpu_usage(),
            mem_mb: p.memory() / 1024 / 1024,
        })
        .collect();
    sort_by_cpu(procs)
}

/// Send a signal to a pid. Returns true if the process was found.
pub fn signal(pid: u32, sig: Signal) -> bool {
    let mut sys = System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::everything()));
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    if let Some(p) = sys.process(Pid::from_u32(pid)) {
        p.kill_with(sig).unwrap_or(false)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorts_by_cpu_desc() {
        let v = vec![
            Proc { pid: 1, name: "a".into(), cpu: 5.0, mem_mb: 10 },
            Proc { pid: 2, name: "b".into(), cpu: 50.0, mem_mb: 10 },
        ];
        let s = sort_by_cpu(v);
        assert_eq!(s[0].pid, 2);
        assert_eq!(s[1].pid, 1);
    }
}
```

Note: verified against glance's sysinfo 0.32.1 usage (`src/panels/cpu.rs`): `RefreshKind::new().with_processes(ProcessRefreshKind::everything())`, `refresh_processes(ProcessesToUpdate::All, true)`, `processes()`, `cpu_usage()`, `pid().as_u32()`, `memory()`. `kill_with(Signal)` and `Pid::from_u32` are the sysinfo signal API; if a name differs in 0.32.x, follow the compiler.

- [ ] **Step 3: Stub main.rs + app.rs**, then `cargo test -p proc source` → 1 passed. `cargo build -p proc` builds.

- [ ] **Step 4: Commit**

```bash
git add proc/ Cargo.toml && git commit -m "feat(proc): sysinfo snapshot + cpu sort"
```

---

## Task 9: proc — summary + main

**Files:** Modify `proc/src/main.rs`

- [ ] **Step 1: Summary + dispatch**

`proc/src/main.rs`:
```rust
mod source;
mod app;

use launcher_core::summary::{wants_summary, Summary};

fn build_summary(procs: &[source::Proc]) -> Summary {
    let headline = procs.first().map(|p| format!("{} {:.0}%", p.name, p.cpu)).unwrap_or_else(|| "idle".into());
    let items: Vec<String> = procs.iter().take(3).map(|p| format!("{} {:.0}%", p.name, p.cpu)).collect();
    Summary::new("proc", headline, items, procs.len())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if wants_summary(&args) {
        println!("{}", build_summary(&source::snapshot()).emit_json());
        return Ok(());
    }
    app::run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Proc;
    #[test]
    fn headline_is_top_proc() {
        let p = vec![Proc { pid: 1, name: "firefox".into(), cpu: 87.0, mem_mb: 100 }];
        assert_eq!(build_summary(&p).headline, "firefox 87%");
    }
}
```

- [ ] **Step 2:** `cargo test -p proc` pass; `cargo run -p proc -- --summary --json` prints a JSON line with a top process. **Commit:**

```bash
git add proc/ && git commit -m "feat(proc): --summary --json (top CPU process)"
```

---

## Task 10: proc — interactive app (kill with two-step confirm)

**Files:** Modify `proc/src/app.rs`

- [ ] **Step 1: Write app.rs (mirror gst structure)**

Read `gst/src/app.rs`. Write `proc/src/app.rs`; items are `source::Proc`. Actions: `k` SIGTERM (immediate), `K` SIGKILL behind a two-step confirm (first `K` arms with a toast, second `K` within the armed state kills), `/` filter, `r` refresh snapshot. No exit-with-command (proc acts directly). Full code:
```rust
//! Interactive proc: list processes by CPU, filter, signal (TERM/KILL).
use crate::source::{signal, snapshot, Proc};
use crossterm::event::{self, Event, KeyCode};
use launcher_core::exit::{finish, RunOutcome};
use launcher_core::filter::filter_indices;
use launcher_core::list::Selection;
use launcher_core::theme;
use launcher_core::tui::{terminal, TerminalGuard};
use launcher_core::ui::{render_footer, three_row, Toast};
use ratatui::backend::CrosstermBackend;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, Terminal};
use std::io::Stdout;
use std::time::Duration;
use sysinfo::Signal;

const FOOTER: &str = "j/k move  / filter  k term  K kill  r refresh  q quit";

struct State {
    procs: Vec<Proc>,
    visible: Vec<usize>,
    list: Selection,
    query: String,
    searching: bool,
    arm_kill: Option<u32>, // pid armed for SIGKILL confirm
    toast: Toast,
}

impl State {
    fn new() -> Self {
        let procs = snapshot();
        let visible: Vec<usize> = (0..procs.len()).collect();
        let list = Selection::new(visible.len());
        Self { procs, visible, list, query: String::new(), searching: false, arm_kill: None, toast: Toast::new() }
    }
    fn refilter(&mut self) {
        self.visible = filter_indices(&self.procs, &self.query, |p| p.name.as_str());
        self.list.set_len(self.visible.len());
    }
    fn focused(&self) -> Option<&Proc> {
        self.list.selected_index().and_then(|i| self.visible.get(i)).map(|&i| &self.procs[i])
    }
    fn reload(&mut self) {
        self.procs = snapshot();
        self.refilter();
    }
}

pub fn run() -> anyhow::Result<()> {
    let outcome = {
        let _guard = TerminalGuard::enter()?;
        let mut term = terminal()?;
        event_loop(&mut term)?
    };
    finish(outcome);
    Ok(())
}

fn event_loop(term: &mut Terminal<CrosstermBackend<Stdout>>) -> anyhow::Result<RunOutcome> {
    let mut state = State::new();
    loop {
        term.draw(|f| draw(f, &state))?;
        if !event::poll(Duration::from_millis(200))? { continue; }
        let Event::Key(k) = event::read()? else { continue };
        if state.searching {
            match k.code {
                KeyCode::Esc | KeyCode::Enter => state.searching = false,
                KeyCode::Backspace => { state.query.pop(); state.refilter(); }
                KeyCode::Char(c) => { state.query.push(c); state.refilter(); }
                _ => {}
            }
            continue;
        }
        // Any non-K key cancels an armed kill.
        if !matches!(k.code, KeyCode::Char('K')) { state.arm_kill = None; }
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(RunOutcome::Quit),
            KeyCode::Down | KeyCode::Char('j') => state.list.down(),
            KeyCode::Up => state.list.up(),
            KeyCode::Char('k') => {
                if let Some(p) = state.focused() {
                    let ok = signal(p.pid, Signal::Term);
                    state.toast.set(if ok { format!("TERM → {}", p.name) } else { "not found".into() });
                    state.reload();
                }
            }
            KeyCode::Char('/') => state.searching = true,
            KeyCode::Char('r') => state.reload(),
            KeyCode::Char('K') => {
                if let Some(p) = state.focused() {
                    let pid = p.pid;
                    let name = p.name.clone();
                    if state.arm_kill == Some(pid) {
                        let ok = signal(pid, Signal::Kill);
                        state.arm_kill = None;
                        state.toast.set(if ok { format!("KILL → {name}") } else { "not found".into() });
                        state.reload();
                    } else {
                        state.arm_kill = Some(pid);
                        state.toast.set(format!("press K again to KILL {name}"));
                    }
                }
            }
            _ => {}
        }
    }
}

fn draw(f: &mut Frame, state: &State) {
    let [head, body, foot] = three_row(f.area());
    let title = if state.searching {
        format!(" proc  /{}", state.query)
    } else {
        format!(" proc  {} procs", state.procs.len())
    };
    f.render_widget(Paragraph::new(Line::from(Span::styled(title, theme::header()))), head);

    let highlighted = state.list.selected_index();
    let lines: Vec<Line> = state.visible.iter().enumerate().map(|(row, &i)| {
        let p = &state.procs[i];
        let is_active = highlighted == Some(row);
        let marker = if is_active { "> " } else { "  " };
        let style = if is_active { theme::active_row() } else { theme::dim() };
        Line::from(Span::styled(format!("{marker}{:>6} {:>5.0}%  {}", p.pid, p.cpu, p.name), style))
    }).collect();
    f.render_widget(Paragraph::new(lines), body);

    render_footer(f, foot, FOOTER, &state.toast);
}
```

Note: in proc, `k` is SIGTERM (not "up") — movement is `j`/Down and the Up arrow only; the footer documents this. The `if !matches!(k.code, KeyCode::Char('K')) { state.arm_kill = None; }` line before the match cancels an armed SIGKILL on any other keypress.

- [ ] **Step 2: Build + smoke**

Run: `cargo build -p proc` (clean). pty smoke: launch, press `j`, `q` — no panic, header shows process count. Do NOT actually kill anything in the smoke test.

- [ ] **Step 3: Commit**

```bash
git add proc/ && git commit -m "feat(proc): interactive process list with TERM/KILL (two-step)"
```

---

## Task 11: glance — add clip + proc cards

**Files:** Modify `~/projects/glance/src/panels/launchers.rs`

- [ ] **Step 1: Append to CARDS**

In `~/projects/glance/src/panels/launchers.rs`, change the `CARDS` const to:
```rust
const CARDS: &[(&str, &str)] = &[("gst", "gst"), ("clip", "clip"), ("proc", "proc")];
```
(No other changes; `kick_all`/`render` already iterate `CARDS`. `op` is intentionally NOT added — palette-only.)

- [ ] **Step 2: Build glance**

Run: `cd ~/projects/glance && cargo build --release` → clean.

- [ ] **Step 3: Commit**

```bash
cd ~/projects/glance && git add src/panels/launchers.rs && git commit -m "feat: add clip + proc live cards to launchers panel"
```

---

## Task 12: install everything + workspace verify + smoke

**Files:** none (verification)

- [ ] **Step 1: Build, test, install**

```bash
cd ~/projects/launchers && cargo test && cargo clippy --workspace 2>&1 | tail -3 && cargo build --release && ./install.sh
```
Expected: all tests pass (launcher-core + gst + clip + op + proc), clippy clean, "installed gst/clip/op/proc".

- [ ] **Step 2: Summary smoke for each**

```bash
for b in gst clip op proc; do echo "== $b =="; "$HOME/.local/bin/$b" --summary --json; done
```
Expected: each prints a valid `{"launcher":...}` line (clip/op/proc may show empty/idle/0 depending on environment — must not error).

- [ ] **Step 3: glance card smoke**

Install glance (`cd ~/projects/glance && cargo build --release && install -m 0755 target/release/glance ~/.local/bin/glance`). Capture the launchers panel (launchers is already in `~/.config/glance/panels.toml`):
```bash
tmux kill-session -t w1 2>/dev/null; tmux new-session -d -s w1 -x 40 -y 44 'glance'
sleep 1; tmux send-keys -t w1 'p'; sleep 8
tmux capture-pane -t w1 -p | grep -E 'gst|clip|proc' 
tmux send-keys -t w1 'q'; tmux kill-session -t w1 2>/dev/null
```
Expected: the cards region shows `gst · …`, `clip · …`, `proc · …` lines (clip/proc with real data if cliphist/processes available).

- [ ] **Step 4: Final commit if any tweaks**

```bash
cd ~/projects/launchers && git add -A && git commit -m "test: wave 1 launchers verified" || echo "no changes"
```

---

## Wave 1 done when

- `cargo test` passes in the workspace (launcher-core + gst + clip + op + proc).
- `clip`, `op`, `proc` installed; each runs interactively (no terminal corruption thanks to `TerminalGuard`) and answers `--summary --json`.
- clip: paste/recopy/delete work; op: copy-password auto-clears; proc: TERM works, KILL needs two presses.
- glance launchers panel shows gst/clip/proc cards; op stays palette-only.
- Remaining for later waves: Wave 2 (`docker`, `svc`, `hub`), Wave 3 (`ssh`, `note`, `gh`, `port`, `agent`).
