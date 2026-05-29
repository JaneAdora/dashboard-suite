---
type: spec
title: launchers-quicklaunch-design
status: draft
date: 2026-05-28
---

# glance `launchers` panel: quick-launch (spawn in tmux) design

Make the glance `launchers` panel able to **spawn** a launcher in a new tmux window, not just copy its name to the clipboard. Mirrors the proven `crew` panel spawn pattern. All code claims below were verified against source on 2026-05-28.

## Goal

Today the `launchers` panel is a reference palette: each row's letter hotkey copies the launcher *name* to the clipboard (`clip::copy`). Add the ability to actually open a launcher in a new tmux window, so the dashboard becomes a real launcher and not just a copy-paste menu.

## Decisions (settled in brainstorming)

- **Keep copy, add spawn.** Letter hotkeys still copy the name (unchanged). A new selection cursor + `Enter` spawns the focused launcher.
- **Bare command spawn**, matching `crew`: `tmux new-window <launcher>`. No `eval` wrapper (so `clip`'s output is never executed; `gst`'s printed `cd` is browse-only for now, a clean follow-up).
- **Built-flag gated scope.** Only entries that are genuinely-built suite launchers are spawnable; the rest toast "not built yet."

## Non-goals (v1)

- No `eval "$(launcher)"` wrapper / no auto-applying an exit-with-command launcher's output in the new window. (Follow-up.)
- No standalone-binary form of this panel (there is none).
- No confirm prompt (spawning an interactive picker is cheap and reversible; matches `crew`'s no-confirm `d`).
- No change to the live-card polling (`CARDS`) or the `--summary --json` machinery.
- No new config; the palette + built flags stay hardcoded consts.

## Current state (verified against source)

`~/projects/glance/src/panels/launchers.rs`:
- `PALETTE: &[(&str, &str, char)]` (lines 20-37): 16 `(name, description, shortcut)` rows.
- `CARDS` (line 42): `[("gst","gst"), ("clip","clip"), ("proc","proc")]` — launchers with a live card; `kick_all` (line 71) and `render` (line 175) iterate it.
- `render` builds the panel as a `Vec<Line>`: it lists **all** `PALETTE` rows (each row currently styled name=`pane_header_focused()`, description=`dim()`, key=`historical()`), then a live-cards section from `CARDS` below.
- Panel struct (lines 44-54) holds `cards: HashMap<String, String>` and `copied: Option<(String, Instant)>` (a 3s "copied:" message shown in the title, line 151-155). **No selection cursor today; no general toast/footer line** (unlike `crew`, which has a dedicated footer chunk).
- `handle_key` (lines 185-194): on `Char(c)`, scans `PALETTE` for `shortcut == c`; if found, `crate::clip::copy(name)`, set `self.copied`, return `true`. Does not override `wants_keys()` (so the trait default `false` applies).

`~/projects/glance/src/panels/crew.rs` (lines 42-59): the only tmux-spawn in the crate, inline, in the `CrewAction::Drop { command, cwd, claude }` arm (`cwd: Option<String>`):
```rust
if std::env::var("TMUX").is_ok() {
    let mut c = Command::new("tmux");
    c.arg("new-window");
    if let Some(dir) = &cwd { c.arg("-c").arg(dir); }
    for part in claude.split(' ') { c.arg(part); }
    let ok = c.status().map(|s| s.success()).unwrap_or(false);
    self.toast = Some(if ok { "opened in new tmux window".into() } else { "tmux failed".into() });
} else {
    clip::copy(&command);
    self.toast = Some("no tmux: copied instead".into());
}
```

`~/projects/glance/src/app.rs` global key router (`handle_key`, lines 173-234): runs FIRST, reserves `q ? r [ ] n p`, digits `0-9`, `Tab`, `Left`, `Right`, `Ctrl-C`; `Esc` only while help is open. Unrecognized keys fall through the `_ =>` arm (lines 227-231) to the current panel's `handle_key`. **`Up`, `Down`, `j`, `k`, `Enter` are NOT reserved** and reach the panel in normal mode (no `wants_keys` needed). The panel's `handle_key` `bool` return is ignored at the call site (`let _ = ...`), which is fine: the global handler dispatches its own keys before delegating.

Installed-binary probe (2026-05-28): genuine suite launchers present = `gst` `clip` `1p` `proc` (`~/.local/bin`), `roam` `wt` `recall` (`~/.cargo/bin`), `mm` (bash shim, `~/.local/bin`). The names `docker` (`/usr/bin/docker`), `ssh` (`/usr/bin/ssh`), `gh` (`~/.local/bin/gh`, the GitHub CLI Go binary), and `agent` (`~/.local/bin/agent`, cursor-agent) also resolve on PATH but are **foreign tools, not the intended Rep Cap suite launchers**, so they are NOT built. Genuinely absent (no binary at all): `svc` `note` `port` `hub`. PATH-probing alone is therefore unsafe (it would spawn the wrong thing for docker/ssh/gh/agent), which is why scope is gated by an explicit `built` flag, not by `command -v`.

## Design

### 1. Selection cursor (new)

Add `selected: usize` to `LaunchersPanel` (default `0`). Cursor movement (normal mode, no `wants_keys`):
- `Up` or `k` -> `selected = selected.saturating_sub(1)`
- `Down` or `j` -> `selected = (selected + 1).min(PALETTE.len() - 1)`

`j`/`k` are safe because: (a) the global router reserves only `q ? r [ ] n p` + digits + Tab/arrows, not `j`/`k`; (b) `j`/`k` are the established cursor convention in every other interactive panel (`cal`, `tasks`, `crew`, `health`); and (c) they are not `PALETTE` shortcuts, so no collision with copy hotkeys.

### 2. Rendering (cursor highlight + dim-unbuilt)

`render` already lists every `PALETTE` row, so the change is per-row styling, not new layout. Add a leading gutter and restyle each row by (focused, built) state. Replace the current uniform per-row styles with this table:

| state | gutter | name span | key `[k]` span |
|---|---|---|---|
| focused + built | `▸ ` | `theme::active_row()` (pink bold) | `theme::historical()` |
| unfocused + built | `  ` | `theme::pane_header()` (lavender bold) | `theme::historical()` |
| unbuilt (focused or not) | `▸ `/`  ` as above | `theme::dim()` | `theme::dim()` |

The description span stays `theme::dim()` in all states. Key point: non-focused built rows must drop from the current always-on `pane_header_focused()` (magenta) down to `pane_header()` so the focused row's `active_row()` actually pops. Unbuilt rows render fully dimmed so it is obvious at a glance what can be spawned. The focus gutter (`▸ `) still shows on an unbuilt focused row so the cursor is locatable.

### 3. Status/toast model (generalize `copied`)

The panel has only `copied: Option<(String, Instant)>` today, rendered in the title. Generalize it to a single transient-status slot used for ALL messages: rename to `status: Option<(String, Instant)>` (or keep the field name and broaden its use) rendered exactly where "copied: X" shows now. Every action sets this one slot, so messages never collide:
- copy: `"copied: <name>"`
- spawn ok: `"opened <name> in new tmux window"`
- spawn fail: `"tmux failed"`
- unbuilt: `"<name>: not built yet"`
- no tmux: `"no tmux: copied <name>"`

No second toast field and no new footer line are introduced.

### 4. Spawn action (Enter)

`Enter` acts on `PALETTE[selected]`:
1. If `!built` -> status `"<name>: not built yet"`, do nothing else.
2. Else if in tmux (`spawn::in_tmux()`) -> `spawn::tmux_new_window(None, &[name])`; status `"opened <name> in new tmux window"` on success, `"tmux failed"` on failure.
3. Else (not in tmux) -> `clip::copy(name)` + status `"no tmux: copied <name>"` (preserves today's behavior as the fallback).

**cwd = `None`** (omit `-c`): the new window inherits the spawning tmux pane's cwd, which is exactly the `$PWD` the user is sitting in. Glance's own process cwd (`current_dir()`) is unrelated to the user's shell and must NOT be used. Launchers have no per-row cwd, so inheritance is the right default (and `crew`'s use of a per-job cwd does not apply here).

The launcher is spawned by its **command name** (PATH lookup, exactly as `crew` spawns `claude`). We spawn the string `"1p"`, never `"op"` — the fork-bomb path stays untouchable (no `"op"` is constructed anywhere in this feature; enforce with a code comment, not a test). Robustness note: bare-name spawn works when the tmux server's inherited PATH includes `~/.local/bin` and `~/.cargo/bin` — true for an interactively-started tmux server; a systemd-started server may have a stale PATH. The `~/.cargo/bin` launchers (`roam`/`wt`/`recall`) are more exposed to this than `crew`'s `claude`. Absolute-path resolution is the future hardening; bare-name matches `crew` for v1.

### 5. Letter hotkeys (unchanged + nudge)

The existing `Char(c)` copy behavior is kept verbatim (now writing the generalized `status` slot). Additionally, when a letter matches a `PALETTE` shortcut, set `selected` to that row's index for visual feedback, so a following `Enter` spawns the same launcher **when it is built** (pressing `Enter` on a copied-but-unbuilt row toasts "not built yet", per §4). Copy works for built and unbuilt entries alike (copying a name is always harmless). `handle_key` must test the cursor/Enter keys BEFORE the letter-scan fallback.

### 6. Built flag on the palette

Extend each `PALETTE` row to `(name, description, shortcut, built: bool)`:
```rust
const PALETTE: &[(&str, &str, char, bool)] = &[
    ("gst",    "git status/log", 'g', true),
    ("clip",   "clipboard",      'c', true),
    ("1p",     "1password",      'o', true),
    ("proc",   "processes",      'P', true),
    ("roam",   "directories",    'R', true),
    ("wt",     "git worktrees",  'w', true),
    ("recall", "cc sessions",    'l', true),
    ("docker", "containers",     'd', false),
    ("svc",    "services",       's', false),
    ("ssh",    "hosts",          'h', false),
    ("note",   "journal",        'N', false),
    ("gh",     "PR triage",      'G', false),
    ("port",   "listeners",      't', false),
    ("agent",  "AI sessions",    'a', false),
    ("hub",    "hubspot portals",'b', false),
    ("mm",     "miss minutes",   'm', true),
];
```
Built set = `{gst, clip, 1p, proc, roam, wt, recall, mm}`. The `CARDS` table is independent and unchanged. Flipping a flag to `true` when `svc`/`note`/`gh`/etc. ship is a one-line edit.

### 7. Shared spawn helper (extract + refactor crew)

Create `~/projects/glance/src/spawn.rs`:
```rust
use std::process::Command;

/// True when running inside a tmux session.
pub fn in_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Build the argv for `tmux new-window [-c cwd] <args...>`. Pure; unit-tested.
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
/// Caller owns the not-in-tmux fallback (this never falls back).
pub fn tmux_new_window(cwd: Option<&str>, args: &[&str]) -> bool {
    Command::new("tmux")
        .args(tmux_argv(cwd, args))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
```
Register `pub mod spawn;` in **`lib.rs` only** (the crate-root module list, near `pub mod panels;`). `main.rs` declares no modules (it consumes the lib via `use glance::...`), so it needs no change; the helper is automatically visible to the `glance` and `health` binaries.

**Refactor `crew.rs`** to use the helper, preserving its exact behavior:
- Delete the now-unused `use std::process::Command;` (line 11) to avoid an unused-import warning.
- Replace the inline spawn (lines 43-57) with:
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
Crew **keeps its own three toast strings and its `clip::copy(&command)` no-tmux fallback** (the helper deliberately does not fall back). `cwd.as_deref()` bridges `Option<String>` -> `Option<&str>` without moving `cwd`. `claude.split(' ').collect()` is behaviorally identical to crew's current per-token loop (crew's command has no space-containing args). The standalone `src/bin/crew.rs` does not spawn tmux and is unaffected.

## Error handling / edge cases

- Not in tmux: fall back to copy + status (no failed spawn).
- `tmux new-window` nonzero exit: status `"tmux failed"`, no panic.
- Unbuilt entry: status `"<name>: not built yet"`, never spawn.
- Empty PALETTE is impossible (const); cursor clamps to `[0, len-1]`.
- `1p` spawned as `"1p"`; `"op"` is never constructed.
- **Flash-close (document so it is not mistaken for a bug):** print-and-exit launchers (`gst`, `roam`, `recall`) print a command and exit immediately, so their tmux window opens and closes at once (a brief flash) until the eval-wrapper follow-up lands. Pure-interactive launchers (`proc`, `1p`, `clip`) and `wt` (which launches into claude) stay open until you quit them. This is expected v1 behavior.

## Testing

Unit (in `spawn.rs` and `launchers.rs` `#[cfg(test)]`):
- `tmux_argv(None, &["gst"])` == `["new-window", "gst"]`.
- `tmux_argv(Some("/home/jane"), &["gst"])` == `["new-window", "-c", "/home/jane", "gst"]`.
- `tmux_argv` with a multi-arg vector preserves order.
- Cursor clamp: moving up at index 0 stays 0; moving down at `len-1` stays `len-1` (test the `saturating_sub`/`min` logic, factored if needed).
- Built gating: a helper that maps `PALETTE[i]` to an action returns Spawn for a built row and NotBuilt for an unbuilt row; assert the spawn argv for the `1p` row is `["new-window", "1p"]` (this is the meaningful `1p`-not-`op` guard; a "no row equals op" assert is theater since `"op"` is never a PALETTE name).
- Assert the built set is exactly `{gst, clip, 1p, proc, roam, wt, recall, mm}`.

Manual (the post-merge live smoke that has caught real bugs):
- `tmux capture-pane` smoke (NOT script-log scraping; ratatui cell-diffing hides updates). Launch glance inside tmux, go to the launchers panel, move the cursor (confirm the highlight tracks and non-focused rows are dimmer), press `Enter` on `proc` (a window opens and STAYS — proc is interactive), press `Enter` on `gst` (a window opens and flash-closes — expected, gst prints+exits), press `Enter` on an unbuilt entry like `docker` (status "docker: not built yet", no window), press a letter (copy still works, cursor nudges to that row). See [[reference-glance-panel-dev]].

## Out of scope (future)

- `eval`-wrapping exit-with-command launchers (`gst`/`roam`/`recall`/`wt`) so their `cd`/resume lands in the new window (removes the flash-close).
- Per-launcher spawn behavior.
- Flipping `built` flags as `svc`/`note`/`port`/`gh`/etc. ship (one-line edits then).
- Absolute-path launcher resolution (tmux-server stale-PATH hardening).
- A standalone binary form of the launchers panel.
