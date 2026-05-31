---
type: spec
title: suite-term-design
status: revised
date: 2026-05-31
note: v2 incorporates the 2026-05-31 review (resolution at end of file)
---

# suite-term — Shared TUI Primitives Crate

**Goal:** Extract the drift-prone clipboard, shell-quoting, and panic-hook helpers that are copy-pasted across the dashboard widget suite into one small library crate, consumed by every suite repo via a pinned git dependency, so a fix lands once instead of N times.

**Status:** Design approved 2026-05-30; revised 2026-05-31 after review. Next step: implementation plan.

---

## Background / motivation

The 2026-05-29 suite robustness audit found three bug *classes* that recur across the separate repos because each repo carries its own copy of the same helper, and the copies drift:

- **Clipboard / OSC 52** — 6 independent implementations with 5 different return types. Two (`wt`, `recall`) have **no size cap at all** (latent unbounded-OSC52). One (`roam`) shipped a char-boundary **panic**. The wl-copy "null the child's stderr" fix landed in `glance/src/clip.rs` last session but was **never propagated** to the launcher binaries (`launchers/clip`, `launchers/onepw`).
- **Shell quoting** — 5+ copies of "single-quote a token for an emitted `cd`/`eval` command." `gst` and `wt` were missing it entirely (shell-injection under `eval "$(launcher)"`).
- **Panic hook** — `glance`/`atlas`/`recall`/`mandalas` each have their own; **`roam` and `wt` have none**, so a panic leaves the terminal in raw/alt-screen mode (precisely why roam's OSC52 panic corrupted the display).

A single source of truth makes these fixes land once and stops the drift.

## Locked decisions

1. **Sharing mechanism: git dependency.** `suite-term` is published as its own GitHub repo; each consumer pins a rev. Standalone `cargo build` of the public repos (`roam`/`wt`/`atlas`/`mandalas`) keeps working. Rejected: path dependency (breaks standalone public clones), vendor+drift-check (more machinery), crates.io (public-namespace + release overhead).
2. **Scope: clipboard + quote + panic hook**, the latter feature-gated. Out of scope: tmux-spawn helper, per-app exit-command builders, theme (already shared at runtime via `theme.toml`).
3. **Repos stay separate.** No mono-repo (consistent with the 2026-05-21 roadmap decision).
4. **Quoting policy: quote every dynamic token; normalize safe values to bare.** `shell_quote` is conditional — safe tokens stay bare, metachar tokens are single-quoted. It is applied to **all** interpolated dynamic values in emitted commands — paths **and** session IDs — not only `cwd`. Command names taken from the environment (`$EDITOR`, `$SHELL`) are **not** quoted: a command may legitimately carry arguments (`EDITOR='code -w'`), so blanket-quoting would break them; they are treated as trusted user environment. (Validating command names is noted as future work, not v1.)

---

## Architecture

A single library crate at `~/projects/suite-term`, published to `github.com/JaneAdora/suite-term`.

```
suite-term/
├── Cargo.toml
├── README.md
└── src/
    ├── lib.rs        # pub mod quote; #[cfg(feature="clipboard")] pub mod clipboard; #[cfg(feature="panic-hook")] pub mod panic;
    ├── clipboard.rs  # OSC 52 + wl-copy/clear   (feature "clipboard")
    ├── quote.rs      # shell_quote / quote_path  (always compiled, std-only)
    └── panic.rs      # install_panic_hook        (feature "panic-hook")
```

### Cargo.toml

```toml
[package]
name = "suite-term"
version = "0.1.0"
edition = "2021"

[features]
default = ["clipboard"]
clipboard = ["base64"]
panic-hook = ["crossterm"]

[dependencies]
base64 = { version = "0.22", optional = true }
crossterm = { version = "0.28", optional = true }
```

- **`quote` is always compiled** (pure std, zero deps) — a quote-only consumer pulls nothing.
- **`clipboard`** is a feature pulling `base64` (matches the version every consumer already uses).
- **`panic-hook`** is a feature pulling `crossterm` (all TUI consumers already depend on crossterm 0.28).
- A panic-hook-only consumer (`mandalas`) uses `default-features = false, features = ["panic-hook"]` and compiles **neither** `base64` **nor** the clipboard module. This makes the "lean" claim honest.
- Consumed as `suite-term = { git = "https://github.com/JaneAdora/suite-term", rev = "<sha>", features = [...] }`. `Cargo.lock` pins the exact commit for reproducible builds between rev bumps.

---

## API surface

### `quote` (always compiled)

```rust
use std::path::Path;

/// Shell-quote a token for an emitted `cd`/`eval "$(launcher)"` command.
/// Shell-safe tokens stay bare; anything with metacharacters (spaces, `;`, `$`,
/// backticks, ...) is single-quoted, with embedded single quotes escaped as `'\''`,
/// so an untrusted path/branch/dir/id can't inject commands.
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
pub fn quote_path(p: &Path) -> String { shell_quote(&p.to_string_lossy()) }
```

Applied to every dynamic token in an emitted command — `cd {path}`, `--resume {id}`, etc. (decision #4).

### `clipboard` (feature `clipboard`)

Collapses all six existing implementations onto one correct core, and returns an **exact** result rather than a lossy bool.

```rust
use base64::Engine;

/// Raw-byte cap before base64. Termux's pty and Blink's clipboard handler choke
/// on OSC 52 sequences much larger than this.
pub const OSC52_CAP: usize = 4096;

/// Result of building/emitting an OSC 52 payload. `sent_bytes` is EXACT: the
/// builder backs off to a UTF-8 char boundary, so it may be a few bytes below
/// the cap. Subsumes roam's CopyResult { Full | Truncated { sent, total } }.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc52 {
    pub sequence: String,
    pub sent_bytes: usize,
    pub total_bytes: usize,
}
impl Osc52 {
    pub fn truncated(&self) -> bool { self.sent_bytes < self.total_bytes }
}

/// Pure: build the OSC 52 sequence for `data`, capped at a char boundary <= cap.
pub fn osc52_sequence(data: &str) -> Osc52 {
    let mut end = data.len().min(OSC52_CAP);
    while end > 0 && !data.is_char_boundary(end) { end -= 1; }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data[..end]);
    Osc52 { sequence: format!("\x1b]52;c;{b64}\x07"), sent_bytes: end, total_bytes: data.len() }
}

/// Write the sequence to `out` (testable; surfaces io::Result for deterministic tests).
pub fn emit_osc52_to<W: std::io::Write>(out: &mut W, data: &str) -> std::io::Result<Osc52> {
    let built = osc52_sequence(data);
    out.write_all(built.sequence.as_bytes())?;
    out.flush()?;
    Ok(built)
}

/// Write to locked stdout (best-effort; clipboard writes are not worth crashing over).
pub fn emit_osc52(data: &str) -> Osc52 {
    use std::io::Write;
    let built = osc52_sequence(data);
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(built.sequence.as_bytes());
    let _ = out.flush();
    built
}

/// Best-effort copy to the local Wayland clipboard. stdout/stderr nulled so a
/// "Failed to connect to a Wayland server" error (over SSH / no WAYLAND_DISPLAY)
/// can never land on the alt-screen. No-op-safe if wl-copy is absent. Not capped.
pub fn wl_copy(data: &str) { /* spawn wl-copy; stdin piped; stdout+stderr null */ }

/// Clear the local Wayland clipboard (`wl-copy --clear`), stdout/stderr nulled.
/// onepw routes its delayed secret-clear spawn through this so the stdio hygiene
/// can't drift back (finding #7).
pub fn wl_clear() { /* spawn wl-copy --clear; stdout+stderr null */ }

/// OSC 52 + local Wayland clipboard (panel-side path for glance/launchers).
pub fn copy(data: &str) -> Osc52 { let built = emit_osc52(data); wl_copy(data); built }
```

**Usage:** OSC52-only consumers (`roam`/`wt`/`recall`/`atlas`) call `emit_osc52`; consumers that also want the local clipboard (`glance`, launcher panels) call `copy`. Toasts read `.truncated()` / `.sent_bytes` / `.total_bytes` directly (no reconstruction).

### `panic` (feature `panic-hook`)

```rust
/// Install BEFORE terminal setup (enable_raw_mode / EnterAlternateScreen) so a
/// panic during setup is also covered. On panic: disable mouse capture, leave
/// the alternate screen, disable raw mode, then run the default hook (prints the
/// panic). DisableMouseCapture is a harmless no-op where capture was never
/// enabled, so it covers apps (mandalas) that use it without an extension point.
pub fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let mut out = std::io::stdout();
        let _ = crossterm::execute!(
            out,
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        );
        let _ = crossterm::terminal::disable_raw_mode();
        default(info);
    }));
}
```

---

## Consumer migration map

| repo | clipboard | quote | panic hook |
|---|---|---|---|
| `roam` | `emit_osc52` (drop local OSC52 + `OSC52_RAW_CAP`) | `quote_path` + cd/claude/editor/shell builders; quote the resume id too | **NEW** `install_panic_hook()` (had none) |
| `wt` | `emit_osc52` (**fixes missing cap**) | `launch_command_for`: quote `cwd` **and** `resume_id` | **NEW** `install_panic_hook()` (had none) |
| `recall` | `emit_osc52` (**fixes missing cap**) | `resume_command`: quote `cwd` **and** `session_id`; tests updated to bare-normalized form | replace own `set_hook` |
| `atlas` | `emit_osc52` (drop `ui/osc52.rs`) | `shell_escape` -> `shell_quote` | replace own `set_hook` |
| `glance` | `copy` (OSC52+wl-copy; gains the cap) | crew/tasks: `shell_quote` for `cwd` **and** the resume id; tests updated to bare-normalized form | replace own `set_hook` |
| `launchers`/`launcher-core` | use `osc52_sequence`/`wl_copy` (drop local `clipboard.rs`); `clip`/`onepw` use `wl_copy`; `onepw` delayed clear routes through `wl_clear` | `exit::shell_quote` -> re-export crate's | keeps its RAII `TerminalGuard` (no hook) |
| `mandalas` | — (`default-features = false`) | — | optional: replace own hook (`features = ["panic-hook"]`) |

**Bugs fixed for free:** `wt` + `recall` unbounded OSC52 -> capped; `glance` OSC52 gains the cap; `roam` + `wt` gain a panic hook.

**Intentional normalization (finding #1):** `recall` and `glance` currently *always* single-quote `cwd`; the canonical `shell_quote` leaves safe paths bare. `cd /safe` and `cd '/safe'` are behaviorally identical, so the migration commits **update those tests to expect the bare form** — a deliberate normalization, not a regression.

---

## Out of scope (v1) — including deliberate review declines

- **tmux-spawn helper** — semantics diverge per app (roam emits a string for `eval`; glance/crew spawn directly). Not unified here.
- **Per-app exit-command builders** — stay per-app; only the quoting is shared.
- **Theme** — already shared at runtime via `theme.toml`.
- **`shell_quote_always` (declined, finding #1):** no behavioral reason to preserve always-quoting; bare safe paths are equivalent.
- **Command-name validation for `$EDITOR`/`$SHELL` (declined, finding #2):** can't blanket-quote a command that may carry args; treated as trusted user environment (decision #4). Validation noted as future work.
- **Panic-hook extension point / guard helper (declined, finding #5):** YAGNI — `DisableMouseCapture` in the common hook covers the only consumer (mandalas) that uses extra modes. Revisit only if a consumer needs bracketed paste etc.
- **An RAII `TerminalGuard`** — `launcher-core`'s Drop-based guard is more robust (restores on normal return AND unwind), but migrating roam/glance to it is larger than adding a hook. Future option; the hook is the v1 primitive.

---

## Versioning & update workflow

- Fix in `suite-term` -> commit + push -> bump the pinned `rev` in each consumer -> rebuild. ~6 consumers; bounded and infrequent once stable.
- `Cargo.lock` pins the resolved commit, so builds are reproducible and standalone clones are deterministic.
- **Future (not v1):** an `rsuite update --shared` verb that bumps the rev across all manifest repos in one step.

---

## Testing strategy

**`suite-term` owns (pure tests by default):**
- `clipboard`: short input not truncated (`sent_bytes == total_bytes`); oversized input truncated at the nearest char boundary <= cap; a multibyte char straddling `OSC52_CAP` does not panic, is excluded, and yields **exact** `sent_bytes` (a few below cap); envelope round-trips (`\x1b]52;c;<base64>\x07`); `emit_osc52_to` against an in-memory `Vec<u8>` writes the sequence and returns the same `Osc52`.
- `quote`: plain token bare; spaces quoted; injection string (`foo; rm -rf ~`) neutralized; embedded apostrophe escaped (`/a/b's` -> `'/a/b'\''s'`); empty string -> `''`.
- `panic`: smoke test that `install_panic_hook()` runs without panicking.

**Each consumer:** existing suite green after the swap (clipboard tests moved onto the pure builder / `emit_osc52_to` to stop emitting raw OSC 52 into test output, finding #6). At least one migrated consumer (roam) gets an **integration test asserting exact `sent_bytes`** for a multibyte string crossing the cap (additional rec).

### Per-repo migration acceptance checklist
A repo's migration is "done" only when all hold:
- [ ] No local OSC 52 encoder remains (uses `suite_term::clipboard`).
- [ ] No local shell-quote/`shell_escape` helper remains (uses `suite_term::quote`).
- [ ] Every emitted shell command has **all** dynamic tokens (paths AND ids) quoted/validated.
- [ ] Every local `wl-copy` spawn — including any delayed `--clear` — nulls stdout **and** stderr (or routes through `wl_copy`/`wl_clear`).
- [ ] Panic cleanup is installed **before** terminal setup (or the repo uses an RAII guard).
- [ ] `cargo build --release` **and** `cargo test` are both warning-clean (the test profile too — the 2026-05-29 release-only check missed test-profile warnings).
- [ ] For public repos: a fresh standalone `git clone` builds (the git-dep resolves).

---

## Rollout order (for the implementation plan)

1. **Create + publish `suite-term`** with full unit tests; push to `github.com/JaneAdora/suite-term`.
2. **Migrate `launcher-core` first** — local-only, lowest risk; it already holds the canonical `osc52_sequence` + `shell_quote`. Validates the crate end-to-end.
3. **`glance`** (local-only; `copy` + replaces its panic hook).
4. **`recall`** (local-only; fixes the missing cap; tests re-baselined to bare quoting).
5. **`roam`, `wt`, `atlas`** (public) — migrate, then verify each still builds from a fresh standalone clone.
6. **`mandalas`** (optional; panic-hook-only consumer).

Each step is its own commit (and rev bump). The plan pins the rev once `suite-term` is published and bumps it only if the crate changes mid-rollout.

---

## Review assessment - 2026-05-31

Scope reviewed: this spec plus the local `dashboard-suite`, `launchers`, `roam`, `wt`, `recall`, `atlas`, `glance`, and `mandalas` repositories. I treated this as a read/review/assess pass only; no implementation changes were made.

### Findings

1. **High - The proposed shared quote helper is not behavior-preserving for all existing consumers.**
   The canonical `shell_quote` returns bare strings for safe-looking paths, but `recall` and parts of `glance` currently always single-quote non-empty `cwd` values. Examples: `recall/src/actions.rs` tests expect `cd '/home/jane/projects/recall' && ...`, while `glance/src/crew/job.rs` and `glance/src/bin/tasks.rs` build `cd '<cwd>' && ...`. Migrating them directly to the proposed helper changes emitted command strings and will break the spec's "existing test suite must remain green" requirement unless those tests are intentionally updated.

   **Recommendation:** either document that the rollout intentionally normalizes safe paths to bare tokens and update tests in the migration commits, or expose a second `shell_quote_always` helper and use it for consumers whose command shape is intentionally stable.

2. **High - The shell-injection scope still leaves dynamic non-path tokens unquoted.**
   The design frames the quote helper around paths/dirs, but the emitted `eval "$(launcher)"` commands also interpolate session IDs and sometimes env-derived command names. Current examples include `wt/src/actions.rs` interpolating `resume_id`, `recall/src/actions.rs` interpolating `row.session_id`, `glance/src/crew/job.rs` interpolating the resume ID, and `glance/src/bin/tasks.rs` interpolating `sid`. If any of those values are malformed or attacker-controlled through a state file/database, path quoting alone does not close the shell-injection class.

   **Recommendation:** require every dynamic shell token to be quoted or validated, not only `cwd`. A pragmatic v1 rule is: quote paths and IDs with `shell_quote`, and explicitly validate command names such as `$EDITOR`/`$SHELL` when they are used in command position.

3. **Medium - The boolean clipboard return loses information some consumers already have.**
   `roam` currently returns `CopyResult::Truncated { sent, total }`. The proposed API returns only `bool` and suggests consumers combine that with `OSC52_CAP` and `data.len()`. That is not exact when the cap lands inside a multibyte UTF-8 character, because the builder backs off to the previous char boundary and sends fewer than `OSC52_CAP` bytes.

   **Recommendation:** return a small structured result, for example `{ sequence, truncated, sent_bytes, total_bytes }` from the pure builder and `{ truncated, sent_bytes, total_bytes }` from `emit_osc52`. Keep a `bool` convenience only if needed.

4. **Medium - The feature graph is less lean than the text says.**
   `base64` is specified as an unconditional dependency. `glance` and `mandalas` do not currently declare `base64` directly, and a panic-hook-only or quote-only consumer would still pull `base64` unless clipboard code is feature-gated. `default = []` does not make quote-only use lean if the default crate still compiles a public `clipboard` module that depends on unconditional `base64`.

   **Recommendation:** either make `clipboard` a feature with `base64` optional, or keep the simpler unconditional dependency and correct the spec language so implementation expectations are honest.

5. **Medium - Panic-hook install guidance should avoid the raw-mode failure window and preserve app-specific cleanup.**
   The spec says to call `install_panic_hook()` after entering raw mode. Current `atlas`, `glance`, and `mandalas` install their hooks before terminal setup, which also covers panics/errors between `enable_raw_mode()` and `EnterAlternateScreen`. `mandalas` additionally disables mouse capture in its cleanup path.

   **Recommendation:** specify that the hook is installed before terminal setup, or provide a small guard-style terminal setup helper in v1. If the hook stays generic, include an extension point or a documented local cleanup hook for consumers that enable mouse capture, bracketed paste, or other terminal modes.

6. **Low - Side-effecting clipboard tests already pollute test output.**
   `cargo test` for `atlas` and `glance` emitted raw OSC 52 escape sequences because some tests call side-effecting copy paths. The new crate's pure `osc52_sequence` helps, but the consumer migration plan should explicitly move tests onto pure helpers where possible.

   **Recommendation:** make pure builder tests the default. Consider an `emit_osc52_to<W: Write>` helper for deterministic tests of write behavior without touching real stdout.

7. **Low - `onepw`'s delayed `wl-copy --clear` remains a local drift point.**
   The spec centralizes `wl_copy` but leaves the secret-specific delayed clear in `onepw`. The current implementation correctly nulls stdout/stderr, but this is the same stdio-hygiene class the shared crate is meant to stop from drifting.

   **Recommendation:** either add a tiny shared `wl_clear` / `wl_clear_after` helper, or add a rollout checklist item that verifies every local `wl-copy` spawn, including delayed clear commands, has stdout and stderr nulled.

### Additional recommendations

- Add a migration acceptance checklist per repo: no local OSC 52 encoder remains, no local shell-quote helper remains unless intentionally named differently, every emitted shell command has all dynamic tokens quoted/validated, and panic cleanup is installed before terminal setup.
- Add one integration-style test in at least one migrated consumer that verifies a multibyte string crossing the OSC 52 cap reports the exact `sent_bytes`, not just `truncated = true`.
- Decide before implementation whether command-output normalization from always-quoted safe paths to bare safe paths is acceptable. This is not a safety issue, but it is a compatibility and test-update issue.

### Checks run

- `cargo test` in `dashboard-suite`: 4 passed.
- `cargo test` in `launchers`: 32 passed across workspace members.
- `cargo test` in `roam`: 34 passed.
- `cargo test` in `wt`: 56 passed; one existing unused-import warning in `src/app.rs`.
- `cargo test` in `recall`: 75 passed.
- `cargo test` in `atlas`: 74 passed; raw OSC 52 sequences appeared in test output.
- `cargo test` in `glance`: 117 passed; two existing unused-import warnings and raw OSC 52 sequences appeared in test output.
- `cargo test` in `mandalas`: 35 passed.

Local git status was clean for the reviewed repos before writing this assessment. `/home/jane/projects/suite-term` did not exist yet, which is consistent with the rollout starting at "Create + publish `suite-term`."

---

## Review resolution (v2, 2026-05-31)

Disposition of the 2026-05-31 review. All 7 findings are valid; folded in proportionately — cheap correct fixes adopted, three specific pieces of machinery declined with rationale.

- **#1 quote normalization — ADOPTED; `shell_quote_always` DECLINED.** `shell_quote` stays conditional; `recall`/`glance` migrate and their tests are re-baselined to the bare form (intentional normalization; `cd /safe` ≡ `cd '/safe'`). No second always-quote helper — no behavioral need.
- **#2 quote all dynamic tokens — ADOPTED (ids); command-name validation DECLINED.** Decision #4 now applies `shell_quote` to paths AND session IDs. Command names (`$EDITOR`/`$SHELL`) can't be blanket-quoted (they may carry args) and are documented as trusted user environment; validation is future work.
- **#3 structured clipboard result — ADOPTED.** Pure builder returns `Osc52 { sequence, sent_bytes, total_bytes }` with exact `sent_bytes`; subsumes roam's `CopyResult`. Lossy bool reconstruction removed.
- **#4 feature-graph honesty — ADOPTED.** `clipboard` (→ optional base64) and `panic-hook` (→ optional crossterm) are features; `quote` is always compiled (std-only). mandalas pulls neither base64 nor clipboard.
- **#5 panic-hook timing/cleanup — ADOPTED (lean); extension point DECLINED.** Install before terminal setup; common hook does `DisableMouseCapture` + `LeaveAlternateScreen` + `disable_raw_mode` (covers mandalas for free). No closure/guard extension point (YAGNI).
- **#6 testable emit + pure-builder tests — ADOPTED.** Added `emit_osc52_to<W: Write>`; consumer clipboard tests move off side-effecting paths.
- **#7 onepw delayed-clear drift — ADOPTED.** Added `wl_clear()`; onepw keeps the 30s scheduling but routes the spawn through it; the acceptance checklist enforces nulled stdio on every wl-copy spawn.
- **Additional recs — ADOPTED.** Per-repo acceptance checklist and a sent_bytes integration test are in the Testing section.

**Independent of the crate:** the review also caught **test-profile** unused-import warnings (`wt`: `Duration`; `glance`: `ResponseStatus` ×2) that the 2026-05-29 release-only check missed. Fixed separately in those repos.
