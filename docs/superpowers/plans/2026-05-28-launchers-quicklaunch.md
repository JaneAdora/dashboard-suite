---
type: plan
title: launchers-quicklaunch-plan
status: ready
date: 2026-05-28
spec: ../specs/2026-05-28-launchers-quicklaunch-design.md
---

# launchers quick-launch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the glance `launchers` panel spawn a launcher in a new tmux window (cursor + Enter), keeping the existing letter-copy behavior, gated by a per-entry `built` flag.

**Architecture:** Extract a shared `spawn` module (`in_tmux`, `tmux_argv`, `tmux_new_window`) from crew's inline tmux block; refactor `crew` to use it; then add a selection cursor, a `built` flag on `PALETTE`, dim-unbuilt + focus-highlight rendering, and an `Enter`-spawns action to the `launchers` panel. All facts verified against source 2026-05-28.

**Tech Stack:** Rust 2021, ratatui 0.29, crossterm 0.28, `std::process::Command`, tmux. No new dependencies.

---

## File structure

- **Create:** `~/projects/glance/src/spawn.rs` — `in_tmux()`, pure `tmux_argv()`, `tmux_new_window()`. ~35 lines + tests.
- **Modify:** `~/projects/glance/src/lib.rs` — add `pub mod spawn;`.
- **Modify:** `~/projects/glance/src/panels/crew.rs` — replace the inline tmux block with the helper; drop the now-unused `Command` import.
- **Modify:** `~/projects/glance/src/panels/launchers.rs` — `PALETTE` 4-tuple + `built` flags; struct gains `selected`, renames `copied`->`status`; render gains gutter/focus/dim; `handle_key` gains cursor + Enter; new `move_up`/`move_down`/`spawn_selected`; tests.

No other files change. `src/main.rs` is NOT touched (it declares no modules; it `use`s the lib). `src/bin/crew.rs` is NOT touched (it does not spawn tmux).

---

### Task 1: `spawn` module + tmux_argv tests + lib registration

**Files:**
- Create: `~/projects/glance/src/spawn.rs`
- Modify: `~/projects/glance/src/lib.rs:13-14`

- [ ] **Step 1.1: Create `src/spawn.rs` with the helper + failing test**

Write `~/projects/glance/src/spawn.rs`:

```rust
//! Shared spawn helpers. Currently: open a command in a new tmux window.
//! Extracted from the crew panel so launchers can reuse it.
use std::process::Command;

/// True when running inside a tmux session.
pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Build the argv for `tmux new-window [-c <cwd>] <args...>`. Pure; unit-tested.
pub fn tmux_argv(cwd: Option<&str>, args: &[&str]) -> Vec<String> {
    let mut v = vec!["new-window".to_string()];
    if let Some(d) = cwd {
        v.push("-c".to_string());
        v.push(d.to_string());
    }
    v.extend(args.iter().map(|s| s.to_string()));
    v
}

/// Spawn a new tmux window running `args`. Returns true on success.
/// The caller owns the not-in-tmux fallback; this never falls back.
pub fn tmux_new_window(cwd: Option<&str>, args: &[&str]) -> bool {
    Command::new("tmux")
        .args(tmux_argv(cwd, args))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_without_cwd() {
        assert_eq!(tmux_argv(None, &["gst"]), vec!["new-window", "gst"]);
    }

    #[test]
    fn argv_with_cwd() {
        assert_eq!(
            tmux_argv(Some("/home/jane"), &["gst"]),
            vec!["new-window", "-c", "/home/jane", "gst"]
        );
    }

    #[test]
    fn argv_preserves_multi_arg_order() {
        assert_eq!(
            tmux_argv(None, &["claude", "--resume", "abc", "--dangerously-skip-permissions"]),
            vec!["new-window", "claude", "--resume", "abc", "--dangerously-skip-permissions"]
        );
    }
}
```

- [ ] **Step 1.2: Register the module in `lib.rs`**

In `~/projects/glance/src/lib.rs`, add `pub mod spawn;` between `pub mod panels;` (line 13) and `pub mod tasks;` (line 14):

```rust
pub mod panels;
pub mod spawn;
pub mod tasks;
```

- [ ] **Step 1.3: Run the tests**

Run: `cd ~/projects/glance && cargo test --lib spawn`
Expected: 3 tests pass (`argv_without_cwd`, `argv_with_cwd`, `argv_preserves_multi_arg_order`).

(Note: `Vec<String> == vec!["..."]` of `&str` compiles via `String: PartialEq<&str>`.)

- [ ] **Step 1.4: Commit**

```bash
cd ~/projects/glance && git add src/spawn.rs src/lib.rs && git commit -m "spawn: shared tmux_new_window helper + tests"
```

---

### Task 2: Refactor `crew` to use the spawn helper

Mechanical extraction; crew's behavior, toast strings, and no-tmux fallback are preserved exactly.

**Files:**
- Modify: `~/projects/glance/src/panels/crew.rs:11` (remove import), `:3-5` (add import), `:43-57` (replace block)

- [ ] **Step 2.1: Swap the import**

In `~/projects/glance/src/panels/crew.rs`, delete the line `use std::process::Command;` (line 11; it becomes unused after this task). Add `use crate::spawn;` right after `use crate::crew::{CrewAction, CrewCore};` (line 4).

- [ ] **Step 2.2: Replace the inline tmux block**

Replace the body of the `CrewAction::Drop` arm. The current inner code (lines 43-57) is:

```rust
                if std::env::var("TMUX").is_ok() {
                    let mut c = Command::new("tmux");
                    c.arg("new-window");
                    if let Some(dir) = &cwd {
                        c.arg("-c").arg(dir);
                    }
                    for part in claude.split(' ') {
                        c.arg(part);
                    }
                    let ok = c.status().map(|s| s.success()).unwrap_or(false);
                    self.toast = Some(if ok { "opened in new tmux window".into() } else { "tmux failed".into() });
                } else {
                    clip::copy(&command);
                    self.toast = Some("no tmux: copied instead".into());
                }
```

Replace it with:

```rust
                if spawn::in_tmux() {
                    let argv: Vec<&str> = claude.split(' ').collect();
                    let ok = spawn::tmux_new_window(cwd.as_deref(), &argv);
                    self.toast = Some(if ok { "opened in new tmux window".into() } else { "tmux failed".into() });
                } else {
                    clip::copy(&command);
                    self.toast = Some("no tmux: copied instead".into());
                }
```

(The `CrewAction::Drop { command, cwd, claude } => { ... true }` wrapper and the three field bindings stay. `cwd.as_deref()` bridges `Option<String>` -> `Option<&str>` without moving `cwd`. `claude.split(' ').collect()` is identical to the old per-token loop for crew's space-free args.)

- [ ] **Step 2.3: Build (no warnings) + run crew tests**

Run: `cd ~/projects/glance && cargo build --release 2>&1 | tail -3`
Expected: clean build, NO `unused import: std::process::Command` warning (confirms the import was removed).

Run: `cd ~/projects/glance && cargo test --lib crew`
Expected: existing crew tests still pass.

- [ ] **Step 2.4: Commit**

```bash
cd ~/projects/glance && git add src/panels/crew.rs && git commit -m "crew: use shared spawn::tmux_new_window helper"
```

---

### Task 3: launchers data model + rendering (built flag, cursor field, styles)

Reshape `PALETTE` to carry a `built` flag, add the `selected` cursor and a generalized `status` slot, and render the gutter/focus-highlight/dim-unbuilt styling. Behavior unchanged except the title now shows the full status string; no cursor movement or spawn yet (Task 4).

**Files:**
- Modify: `~/projects/glance/src/panels/launchers.rs` (PALETTE lines 19-37; struct 44-54; new() 56-67; render 134-183; handle_key 185-194)

- [ ] **Step 3.1: Reshape `PALETTE` to a 4-tuple with `built`**

Replace the `PALETTE` const (lines 19-37) with:

```rust
/// (name, description, shortcut, built). `built` gates the spawn action; the
/// full family is listed, but only genuine suite launchers are spawnable.
const PALETTE: &[(&str, &str, char, bool)] = &[
    ("gst", "git status/log", 'g', true),
    ("clip", "clipboard", 'c', true),
    ("1p", "1password", 'o', true),
    ("proc", "processes", 'P', true),
    ("roam", "directories", 'R', true),
    ("wt", "git worktrees", 'w', true),
    ("recall", "cc sessions", 'l', true),
    ("docker", "containers", 'd', false),
    ("svc", "services", 's', false),
    ("ssh", "hosts", 'h', false),
    ("note", "journal", 'N', false),
    ("gh", "PR triage", 'G', false),
    ("port", "listeners", 't', false),
    ("agent", "AI sessions", 'a', false),
    ("hub", "hubspot portals", 'b', false),
    ("mm", "miss minutes", 'm', true),
];
```

- [ ] **Step 3.2: Add `selected`, rename `copied` -> `status` in the struct**

Replace the struct field (lines 52-53):

```rust
    /// Last command name copied to the clipboard, for a transient toast.
    copied: Option<(String, Instant)>,
```

with:

```rust
    /// Transient status message (copied / opened / not-built / no-tmux), shown
    /// in the title for 3s. Generalized from the old copy-only toast.
    status: Option<(String, Instant)>,
    /// Cursor index into PALETTE for the spawn action.
    selected: usize,
```

In `new()` (lines 59-66), replace `copied: None,` with:

```rust
            status: None,
            selected: 0,
```

- [ ] **Step 3.3: Update the title to show the status string**

In `render`, replace the title block (lines 150-156):

```rust
        let mut title = vec![Span::styled(" launchers", theme::pane_header())];
        if let Some((name, ts)) = &self.copied {
            if ts.elapsed() < Duration::from_secs(3) {
                title.push(Span::styled(format!("   copied: {name}"), theme::now()));
            }
        }
        f.render_widget(Paragraph::new(Line::from(title)), chunks[0]);
```

with:

```rust
        let mut title = vec![Span::styled(" launchers", theme::pane_header())];
        if let Some((msg, ts)) = &self.status {
            if ts.elapsed() < Duration::from_secs(3) {
                title.push(Span::styled(format!("   {msg}"), theme::now()));
            }
        }
        f.render_widget(Paragraph::new(Line::from(title)), chunks[0]);
```

- [ ] **Step 3.4: Render the palette rows with gutter / focus / dim**

Replace the palette rows block (lines 158-168):

```rust
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
```

with:

```rust
        let rows: Vec<Line> = PALETTE
            .iter()
            .enumerate()
            .map(|(i, (name, desc, key, built))| {
                let focused = i == self.selected;
                let gutter = if focused { "▸ " } else { "  " };
                let name_style = if !*built {
                    theme::dim()
                } else if focused {
                    theme::active_row()
                } else {
                    theme::pane_header()
                };
                let key_style = if *built { theme::historical() } else { theme::dim() };
                Line::from(vec![
                    Span::styled(format!("{gutter}{name:<7}"), name_style),
                    Span::styled(format!("{desc:<16}"), theme::dim()),
                    Span::styled(format!("[{key}]"), key_style),
                ])
            })
            .collect();
        f.render_widget(Paragraph::new(rows), chunks[1]);
```

- [ ] **Step 3.5: Fix `handle_key`'s destructure to compile (copy still writes `status`)**

Replace `handle_key` (lines 185-194):

```rust
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if let crossterm::event::KeyCode::Char(c) = key.code {
            if let Some((name, _, _)) = PALETTE.iter().find(|(_, _, k)| *k == c) {
                crate::clip::copy(name);
                self.copied = Some((name.to_string(), Instant::now()));
                return true;
            }
        }
        false
    }
```

with:

```rust
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        if let crossterm::event::KeyCode::Char(c) = key.code {
            if let Some((name, _, _, _)) = PALETTE.iter().find(|(_, _, k, _)| *k == c) {
                crate::clip::copy(name);
                self.status = Some((format!("copied: {name}"), Instant::now()));
                return true;
            }
        }
        false
    }
```

- [ ] **Step 3.6: Add a built-set test**

Append a test module at the end of `~/projects/glance/src/panels/launchers.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn built_set_is_exact() {
        let built: HashSet<&str> = PALETTE
            .iter()
            .filter(|(_, _, _, b)| *b)
            .map(|(n, _, _, _)| *n)
            .collect();
        let expected: HashSet<&str> =
            ["gst", "clip", "1p", "proc", "roam", "wt", "recall", "mm"].into_iter().collect();
        assert_eq!(built, expected);
    }
}
```

- [ ] **Step 3.7: Build + test**

Run: `cd ~/projects/glance && cargo build --release 2>&1 | tail -3`
Expected: clean build (a `field selected is never read` style warning is acceptable here; Task 4 uses it — though it IS read by render, so likely no warning).

Run: `cd ~/projects/glance && cargo test --lib panels::launchers`
Expected: `built_set_is_exact` passes.

- [ ] **Step 3.8: Commit**

```bash
cd ~/projects/glance && git add src/panels/launchers.rs && git commit -m "launchers: built flag + cursor field + focus/dim rendering"
```

---

### Task 4: launchers cursor movement + Enter-to-spawn

Add the cursor keys, the spawn action, and the letter-copy cursor nudge, plus unit tests for cursor clamping.

**Files:**
- Modify: `~/projects/glance/src/panels/launchers.rs` (add methods to `impl LaunchersPanel`; rewrite `handle_key`; extend tests)

- [ ] **Step 4.1: Add `move_up`/`move_down`/`spawn_selected` to `impl LaunchersPanel`**

Inside `impl LaunchersPanel { ... }` (after the `kick_all` method, before the closing brace of the impl, around line 99), add:

```rust
    fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    fn move_down(&mut self) {
        self.selected = (self.selected + 1).min(PALETTE.len() - 1);
    }

    /// Spawn the focused launcher in a new tmux window (bare command), or toast
    /// if it is not built / not in tmux. We spawn the command name (e.g. "1p"),
    /// never "op" -- the fork-bomb path is never constructed here.
    fn spawn_selected(&mut self) {
        let (name, _, _, built) = PALETTE[self.selected];
        if !built {
            self.status = Some((format!("{name}: not built yet"), Instant::now()));
            return;
        }
        if crate::spawn::in_tmux() {
            let ok = crate::spawn::tmux_new_window(None, &[name]);
            let msg = if ok {
                format!("opened {name} in new tmux window")
            } else {
                "tmux failed".to_string()
            };
            self.status = Some((msg, Instant::now()));
        } else {
            crate::clip::copy(name);
            self.status = Some((format!("no tmux: copied {name}"), Instant::now()));
        }
    }
```

- [ ] **Step 4.2: Rewrite `handle_key` for cursor + Enter + copy-nudge**

Replace the whole `handle_key` (the version from Step 3.5) with:

```rust
    fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> bool {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_up();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_down();
                true
            }
            KeyCode::Enter => {
                self.spawn_selected();
                true
            }
            KeyCode::Char(c) => {
                if let Some(i) = PALETTE.iter().position(|(_, _, k, _)| *k == c) {
                    let name = PALETTE[i].0;
                    crate::clip::copy(name);
                    self.selected = i;
                    self.status = Some((format!("copied: {name}"), Instant::now()));
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
```

(`j`/`k` match the cursor arms before the generic `Char(c)` arm; neither is a `PALETTE` shortcut, so letter-copy is unaffected.)

- [ ] **Step 4.3: Add cursor tests**

Add these tests inside the existing `#[cfg(test)] mod tests` block in `launchers.rs` (alongside `built_set_is_exact`):

```rust
    #[test]
    fn cursor_clamps_at_top() {
        let mut p = LaunchersPanel::new();
        p.selected = 0;
        p.move_up();
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn cursor_clamps_at_bottom() {
        let mut p = LaunchersPanel::new();
        p.selected = PALETTE.len() - 1;
        p.move_down();
        assert_eq!(p.selected, PALETTE.len() - 1);
    }

    #[test]
    fn cursor_moves_within_bounds() {
        let mut p = LaunchersPanel::new();
        p.selected = 2;
        p.move_up();
        assert_eq!(p.selected, 1);
        p.move_down();
        assert_eq!(p.selected, 2);
    }
```

- [ ] **Step 4.4: Build + test**

Run: `cd ~/projects/glance && cargo build --release 2>&1 | tail -3`
Expected: clean build, no unused-method warnings (`move_up`/`move_down`/`spawn_selected` are all used by `handle_key`).

Run: `cd ~/projects/glance && cargo test --lib panels::launchers`
Expected: 4 tests pass (`built_set_is_exact`, `cursor_clamps_at_top`, `cursor_clamps_at_bottom`, `cursor_moves_within_bounds`).

- [ ] **Step 4.5: Commit**

```bash
cd ~/projects/glance && git add src/panels/launchers.rs && git commit -m "launchers: cursor + Enter-to-spawn in new tmux window"
```

---

## Verification

After all tasks land, full build + targeted tests, then a live tmux smoke (the post-merge check that has caught real bugs; do NOT rely on script-log scraping -- ratatui cell-diffing hides updates):

```bash
cd ~/projects/glance && cargo build --release && install -m 0755 target/release/glance ~/.local/bin/glance
cargo test --lib spawn && cargo test --lib panels::launchers && cargo test --lib crew
```

Live smoke (standup-style, isolating the panel via a temp config so it is index 0):

```bash
TMPCFG="$CLAUDE_JOB_DIR/tmp/xdg-ql"; mkdir -p "$TMPCFG/glance"
printf 'panels = ["launchers"]\n' > "$TMPCFG/glance/panels.toml"
tmux new-session -d -s qlsmoke -x 120 -y 40
tmux send-keys -t qlsmoke "XDG_CONFIG_HOME='$TMPCFG' /home/jane/.local/bin/glance" Enter
sleep 2
tmux send-keys -t qlsmoke Down Down        # move cursor
tmux capture-pane -t qlsmoke -p | sed -n '1,20p'   # confirm highlight tracks + unbuilt rows dimmer
tmux send-keys -t qlsmoke Enter            # spawn the focused (built) launcher
sleep 1
tmux list-windows -t qlsmoke               # confirm a new window opened
tmux kill-session -t qlsmoke
```

Manual confirmations (per spec):
- Cursor highlight (`▸` + pink) tracks Up/Down/j/k; non-focused built rows are lavender (dimmer than before), unbuilt rows fully dim.
- `Enter` on `proc` opens a window that STAYS; `Enter` on `gst` opens a window that flash-closes (gst prints+exits) -- expected, not a bug.
- `Enter` on an unbuilt entry (e.g. `docker`) shows "docker: not built yet", no window.
- A letter (e.g. `g`) still copies the name and nudges the cursor to that row.

## Out of scope (not in this plan)

- `eval`-wrapping exit-with-command launchers so their `cd` lands (removes the flash-close).
- Per-launcher spawn behavior; absolute-path launcher resolution.
- Flipping `built` flags as svc/note/port/gh ship (one-line edits then).

## Self-review notes

- Spec coverage: cursor (Task 3 field + render, Task 4 movement); built flag + dim render (Task 3); Enter-spawn + bare command + cwd=None + 1p-not-op (Task 4 `spawn_selected`); keep-copy + nudge (Task 4 `handle_key`); shared helper + crew refactor (Tasks 1-2); status/toast model (Task 3); tests (Tasks 1, 3, 4). All spec sections mapped.
- Type/name consistency: `PALETTE` is the 4-tuple `(&str,&str,char,bool)` in every task that reads it; `status`/`selected` field names match across struct, new(), render, handle_key, methods; `spawn::in_tmux`/`tmux_argv`/`tmux_new_window` signatures identical in definition (Task 1) and callers (Tasks 2, 4). `tmux_new_window(None, &[name])` passes `&[&str]`; `cwd.as_deref()` in crew passes `Option<&str>`.
- No placeholders; every step has full code and exact commands.
