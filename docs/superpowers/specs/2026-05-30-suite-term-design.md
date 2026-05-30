---
type: spec
title: suite-term-design
status: draft
date: 2026-05-30
---

# suite-term — Shared TUI Primitives Crate

**Goal:** Extract the drift-prone clipboard, shell-quoting, and panic-hook helpers that are copy-pasted across the dashboard widget suite into one small library crate, consumed by every suite repo via a pinned git dependency, so a fix lands once instead of N times.

**Status:** Design approved 2026-05-30. Next step: implementation plan.

---

## Background / motivation

The 2026-05-29 suite robustness audit found three bug *classes* that recur across the separate repos because each repo carries its own copy of the same helper, and the copies drift:

- **Clipboard / OSC 52** — 6 independent implementations with 5 different return types (`Result<CopyResult>`, `Result<()>`, `bool`, `(String, bool)`, `()`). Two of them (`wt`, `recall`) have **no size cap at all** (latent unbounded-OSC52). One (`roam`) shipped a char-boundary **panic**. The wl-copy "null the child's stderr" fix landed in `glance/src/clip.rs` last session but was **never propagated** to the launcher binaries, which had their own copies (`launchers/clip`, `launchers/onepw`).
- **Shell quoting** — 5+ copies of "single-quote a path for an emitted `cd`/`eval` command." `gst` and `wt` were missing it entirely (shell-injection under `eval "$(launcher)"`).
- **Panic hook** — `glance`/`atlas`/`recall`/`mandalas` each have their own `set_hook` that restores the terminal; **`roam` and `wt` have none**, so a panic leaves the terminal in raw/alt-screen mode (precisely why roam's OSC52 panic corrupted the display).

A single source of truth makes these fixes land once and stops the drift.

## Locked decisions

1. **Sharing mechanism: git dependency.** `suite-term` is published as its own GitHub repo; each consumer pins a rev. Standalone `cargo build` of the public repos (`roam`/`wt`/`atlas`/`mandalas`) keeps working (cargo fetches the dep). Rejected: path dependency (breaks standalone public clones), vendor+drift-check (more machinery, fixes still touch N files), crates.io (public-namespace + release overhead).
2. **Scope: clipboard + quote + panic hook.** The panic hook is feature-gated so quote-only use stays lean. Rejected from scope: the tmux-spawn helper (semantics vary too much per app), the per-app exit-command builders, and theme (already shared at runtime via `theme.toml`).
3. **Repos stay separate.** No mono-repo consolidation (consistent with the 2026-05-21 roadmap decision).

---

## Architecture

A single library crate at `~/projects/suite-term`, published to `github.com/JaneAdora/suite-term`.

```
suite-term/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs        # pub mod clipboard; pub mod quote; #[cfg(feature="panic-hook")] pub mod panic;
    ├── clipboard.rs  # OSC 52 + wl-copy
    ├── quote.rs      # shell_quote / quote_path
    └── panic.rs      # install_panic_hook (feature-gated)
```

### Cargo.toml

```toml
[package]
name = "suite-term"
version = "0.1.0"
edition = "2021"

[features]
default = []
panic-hook = ["crossterm"]

[dependencies]
base64 = "0.22"
crossterm = { version = "0.28", optional = true }
```

- `base64 = "0.22"` matches the version already used in every consumer (no version skew).
- `crossterm` is optional, pulled in only by the `panic-hook` feature. All consumers already depend on crossterm 0.28, so enabling the feature adds no new transitive dep for them.
- Consumed as: `suite-term = { git = "https://github.com/JaneAdora/suite-term", rev = "<sha>" }`, with `features = ["panic-hook"]` where the hook is used. `Cargo.lock` pins the exact commit for reproducible builds between rev bumps.

---

## API surface

### `clipboard`

Collapses all six existing implementations onto one correct core. The canonical OSC 52 builder is lifted from `launcher-core` (already does the char-boundary backoff correctly).

```rust
use base64::Engine;

/// Raw-byte cap before base64. Termux's pty and Blink's clipboard handler choke
/// on OSC 52 sequences much larger than this.
pub const OSC52_CAP: usize = 4096;

/// Build the OSC 52 escape sequence for `data`, capped at a UTF-8 char boundary
/// <= OSC52_CAP. Pure. Returns (sequence, truncated).
pub fn osc52_sequence(data: &str) -> (String, bool) {
    let mut end = data.len().min(OSC52_CAP);
    while end > 0 && !data.is_char_boundary(end) {
        end -= 1;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data[..end]);
    let truncated = end < data.len();
    (format!("\x1b]52;c;{b64}\x07"), truncated)
}

/// Write the OSC 52 sequence to locked stdout (best-effort; clipboard writes are
/// not worth crashing over). Returns whether the payload was truncated.
pub fn emit_osc52(data: &str) -> bool {
    use std::io::Write;
    let (seq, truncated) = osc52_sequence(data);
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
    truncated
}

/// Best-effort copy to the local Wayland clipboard. stdout/stderr are nulled so
/// wl-copy's "Failed to connect to a Wayland server" error (over SSH / no
/// WAYLAND_DISPLAY) can never land on the alt-screen and corrupt the TUI.
/// No-op-safe if wl-copy is absent. Not capped (local clipboard has no OSC limit).
pub fn wl_copy(data: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("wl-copy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(data.as_bytes());
        }
        let _ = child.wait();
    }
}

/// OSC 52 + local Wayland clipboard (the panel-side path for glance/launchers).
/// Returns whether the OSC 52 payload was truncated.
pub fn copy(data: &str) -> bool {
    let truncated = emit_osc52(data);
    wl_copy(data);
    truncated
}
```

**Usage split:** OSC52-only consumers (`roam`/`wt`/`recall`/`atlas`) call `emit_osc52`; consumers that also want the local clipboard (`glance`, the launcher panels) call `copy`. A consumer that needs a richer "truncated N of M" toast uses the returned `bool` plus the public `OSC52_CAP` const and `data.len()`.

### `quote`

One canonical implementation (the conditional single-quote already written for `launcher-core` and `wt` in the audit).

```rust
use std::path::Path;

/// Shell-quote an argument for an emitted `cd`/`eval "$(launcher)"` command.
/// Shell-safe strings stay bare; anything with metacharacters (spaces, `;`, `$`,
/// backticks, ...) is single-quoted, with embedded single quotes escaped as `'\''`,
/// so an untrusted path/branch/dir name can't inject commands.
pub fn shell_quote(s: &str) -> String {
    let safe = !s.is_empty()
        && s.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(c, '/' | '_' | '-' | '.' | '+' | '~' | ',' | ':' | '@' | '%')
        });
    if safe {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Convenience: shell-quote a path via its lossy string form.
pub fn quote_path(p: &Path) -> String {
    shell_quote(&p.to_string_lossy())
}
```

### `panic` (feature `panic-hook`)

```rust
/// Chain the current panic hook with a terminal restore: on panic, leave the
/// alternate screen and disable raw mode before the default hook prints the
/// panic. Without this, a panic leaves the terminal unusable with the error
/// swallowed by the alt-screen. Call once, after entering raw mode.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = std::io::stdout();
        let _ = crossterm::execute!(out, crossterm::terminal::LeaveAlternateScreen);
        let _ = crossterm::terminal::disable_raw_mode();
        default(info);
    }));
}
```

---

## Consumer migration map

| repo | clipboard | quote | panic hook |
|---|---|---|---|
| `roam` | `emit_osc52` (drop local OSC52 + `OSC52_RAW_CAP`) | `quote_path` + cd/claude/editor/shell builders call `shell_quote` | **NEW** `install_panic_hook()` (had none) |
| `wt` | `emit_osc52` (**fixes missing cap**) | `launch_command_for` calls `shell_quote` (drop local copy) | **NEW** `install_panic_hook()` (had none) |
| `recall` | `emit_osc52` (**fixes missing cap**) | `resume_command` calls `shell_quote` | replace own `set_hook` |
| `atlas` | `emit_osc52` (drop `ui/osc52.rs`) | `shell_escape` -> `shell_quote` | replace own `set_hook` |
| `glance` | `copy` in `clip.rs` (OSC52+wl-copy; gains capping) | crew/tasks inline escapes -> `shell_quote` | replace own `set_hook` |
| `launchers`/`launcher-core` | re-export or use `osc52_sequence`/`wl_copy` (drop local `clipboard.rs`); `clip`/`onepw` use `wl_copy` | `exit::shell_quote` -> re-export crate's | keeps its RAII `TerminalGuard` (Drop restores on panic; no hook needed) |
| `mandalas` | — (no clipboard use) | — (no quote use) | optional: replace own `set_hook` (panic-hook-only consumer) |

**Bugs fixed for free by migration:** `wt` + `recall` unbounded OSC52 -> capped; `glance` OSC52 gains the char-boundary cap; `roam` + `wt` gain a panic hook.

**Behavior preservation:** every consumer's existing tests must stay green after the swap. Per-app exit-command builders (`cd_command`, `claude_command`, `launch_command_for`, `resume_command`) stay in their own repos (they differ per app) but call the shared `shell_quote`. The `onepw` 30-second secret auto-clear stays in `onepw` (it is secret-specific, not general clipboard behavior) but its `wl-copy --clear` spawn must also null stdout/stderr.

---

## Out of scope (v1)

- **tmux-spawn helper** — `glance/spawn.rs` and `launcher-core` both have versions, but semantics diverge (roam emits a command string for `eval`; glance/crew spawn directly). Not unified here.
- **Per-app exit-command builders** — stay per-app; only the quoting is shared.
- **Theme** — already shared at runtime via `~/.config/dashboard-suite/theme.toml`; not a code-crate concern.
- **An RAII `TerminalGuard`** — `launcher-core`'s Drop-based guard is strictly more robust than the panic hook (it restores on both normal return and unwind), but migrating roam/glance from explicit-restore to a guard is a larger change than adding a hook. Noted as a future option; the hook is the agreed v1 primitive.

---

## Versioning & update workflow

- Fix in `suite-term` -> commit + push -> bump the pinned `rev` in each consumer's `Cargo.toml` -> rebuild. ~6 consumers; bounded and infrequent once stable.
- `Cargo.lock` pins the exact resolved commit, so builds are reproducible between rev bumps and CI/standalone clones are deterministic.
- **Future (not v1):** an `rsuite update --shared` verb that bumps the pinned rev across all manifest repos in one step. Noted only.

---

## Testing strategy

**`suite-term` owns:**
- `clipboard`: short input not truncated; oversized input truncated at exactly `OSC52_CAP` or the nearest lower char boundary; a multibyte char straddling `OSC52_CAP` does not panic and is excluded; sequence envelope round-trips (`\x1b]52;c;<base64>\x07`).
- `quote`: plain path stays bare; spaces quoted; injection string (`foo; rm -rf ~`) neutralized; embedded apostrophe escaped (`/a/b's` -> `'/a/b'\''s'`); empty string -> `''`.
- `panic`: a smoke test that `install_panic_hook()` runs without panicking (full behavior is integration-only).

**Each consumer:** its existing test suite must remain green after swapping to the crate. The migration of a repo is "done" only when `cargo build --release` is warning-clean and `cargo test` passes, and (for the public repos) a fresh standalone clone still builds.

---

## Rollout order (for the implementation plan)

1. **Create + publish `suite-term`** with full unit tests; push to `github.com/JaneAdora/suite-term`.
2. **Migrate `launcher-core` first** — local-only, lowest risk, and it already holds the canonical `osc52_sequence` + `shell_quote`. Validates the crate end-to-end and confirms `clip`/`onepw` still work.
3. **`glance`** (local-only; uses `copy` + replaces its panic hook).
4. **`recall`** (local-only; fixes the missing cap).
5. **`roam`, `wt`, `atlas`** (public) — migrate, then verify each still builds from a fresh standalone clone (the git-dep must resolve).
6. **`mandalas`** (optional; panic-hook-only consumer).

Each step is its own commit (and rev bump). The plan pins the rev once `suite-term` is published and bumps it only if the crate changes mid-rollout.
