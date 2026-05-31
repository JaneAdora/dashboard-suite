# suite-term Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a small `suite-term` library crate (clipboard / shell-quote / panic-hook) and migrate every dashboard-suite repo onto it via a pinned git dependency, so the drift-prone helpers have one source of truth.

**Architecture:** New standalone crate at `~/projects/suite-term`, published to `github.com/JaneAdora/suite-term`. Three feature-gated areas: `quote` (always compiled, std-only), `clipboard` (feature → `base64`), `panic-hook` (feature → `crossterm`). Consumers depend on it by pinned git rev so public repos still build from a standalone clone. The crate code in Phase 1 is already compile-verified across the feature matrix.

**Tech Stack:** Rust 2021, `base64 = "0.22"`, `crossterm = "0.28"` (both versions match every existing consumer).

**Spec:** `docs/superpowers/specs/2026-05-30-suite-term-design.md` (v2).

**Conventions for this repo set:** Commit messages use NO `Co-Authored-By` trailer (match repo convention). No em dashes in any prose. Each consumer installs to its existing location (`~/.cargo/bin` for wt/recall/roam/atlas/mandalas; `~/.local/bin` for the launcher binaries + glance + rsuite); reinstall after a successful migration. Never construct `op` (fork-bomb guard) — only `1p`.

**Definition of done for a migrated repo (acceptance checklist — verify ALL):**
- [ ] No local OSC 52 encoder remains (uses `suite_term::clipboard`).
- [ ] No local shell-quote / `shell_escape` helper remains (uses `suite_term::quote`).
- [ ] Every emitted shell command quotes ALL dynamic tokens (paths AND session ids), not just `cwd`.
- [ ] Every local `wl-copy` spawn (including any delayed `--clear`) nulls stdout AND stderr, or routes through `wl_copy`/`wl_clear`.
- [ ] Panic cleanup installs BEFORE terminal setup (or the repo uses an RAII guard, like launcher-core).
- [ ] `cargo build --release` AND `cargo test` are BOTH warning-clean (test profile too — run `cargo test --no-run 2>&1 | grep -c '^warning'` and expect `0`).
- [ ] Public repos (roam/wt/atlas/mandalas): a fresh `git clone` into a temp dir builds (the git-dep resolves).

---

## File structure

**New crate `~/projects/suite-term/`:**
- `Cargo.toml` — package + features + optional deps.
- `src/lib.rs` — module declarations (feature-gated).
- `src/quote.rs` — `shell_quote`, `quote_path` (+ tests). Always compiled.
- `src/clipboard.rs` — `Osc52`, `osc52_sequence`, `emit_osc52`, `emit_osc52_to`, `wl_copy`, `wl_clear`, `copy` (+ tests). Feature `clipboard`.
- `src/panic.rs` — `install_panic_hook` (+ smoke test). Feature `panic-hook`.
- `README.md` — one-paragraph purpose + usage snippet.

**Modified per consumer (Phase 2):** each repo's `Cargo.toml` (add git dep), its clipboard/quote source, its panic-hook install site, and the affected tests. Specifics per task.

---

## Phase 1 — Build and publish `suite-term`

### Task 1: Crate scaffold + `quote` module

**Files:**
- Create: `~/projects/suite-term/Cargo.toml`
- Create: `~/projects/suite-term/src/lib.rs`
- Create: `~/projects/suite-term/src/quote.rs`

- [ ] **Step 1: Create the crate skeleton**

`~/projects/suite-term/Cargo.toml`:
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

`~/projects/suite-term/src/lib.rs`:
```rust
//! Shared TUI primitives for the dashboard widget suite: clipboard (OSC 52 +
//! wl-copy), shell quoting, and a terminal-restoring panic hook. Each is
//! feature-gated so a consumer pulls only what it uses.
pub mod quote;

#[cfg(feature = "clipboard")]
pub mod clipboard;

#[cfg(feature = "panic-hook")]
pub mod panic;
```

- [ ] **Step 2: Write `src/quote.rs` with failing tests first**

```rust
//! Shell quoting for emitted `cd`/`eval "$(launcher)"` commands. Std-only.
use std::path::Path;

/// Shell-quote a token for an emitted command. Shell-safe tokens stay bare;
/// anything with metacharacters (spaces, `;`, `$`, backticks, ...) is
/// single-quoted, with embedded single quotes escaped as `'\''`, so an
/// untrusted path/branch/dir/id can't inject commands under `eval`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_token_stays_bare() {
        assert_eq!(shell_quote("/home/jane/projects/wt"), "/home/jane/projects/wt");
        assert_eq!(shell_quote("abc-123"), "abc-123");
    }

    #[test]
    fn space_is_quoted() {
        assert_eq!(shell_quote("/home/jane/My Repo"), "'/home/jane/My Repo'");
    }

    #[test]
    fn injection_is_neutralized() {
        assert_eq!(shell_quote("foo; rm -rf ~"), "'foo; rm -rf ~'");
        assert_eq!(shell_quote("$(id)"), "'$(id)'");
    }

    #[test]
    fn apostrophe_is_escaped() {
        assert_eq!(shell_quote("/a/b's"), "'/a/b'\\''s'");
    }

    #[test]
    fn empty_is_quoted() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn quote_path_delegates() {
        assert_eq!(quote_path(Path::new("/a/b")), "/a/b");
        assert_eq!(quote_path(Path::new("/a b")), "'/a b'");
    }
}
```

- [ ] **Step 3: Run the quote tests**

Run: `cd ~/projects/suite-term && cargo test --no-default-features quote`
Expected: `test result: ok. 6 passed`. (`--no-default-features` proves quote compiles with zero deps.)

- [ ] **Step 4: Commit**

```bash
cd ~/projects/suite-term && git init -q && git add -A
git commit -m "feat: suite-term scaffold + quote module"
```

### Task 2: `clipboard` module

**Files:**
- Create: `~/projects/suite-term/src/clipboard.rs`

- [ ] **Step 1: Write `src/clipboard.rs`**

```rust
//! OSC 52 clipboard (works over SSH / mobile) + best-effort local Wayland copy.
use base64::Engine;

/// Raw-byte cap before base64. Termux's pty and Blink's clipboard handler choke
/// on OSC 52 sequences much larger than this.
pub const OSC52_CAP: usize = 4096;

/// Result of building/emitting an OSC 52 payload. `sent_bytes` is EXACT: the
/// builder backs off to a UTF-8 char boundary, so it may be a few bytes below
/// the cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Osc52 {
    pub sequence: String,
    pub sent_bytes: usize,
    pub total_bytes: usize,
}

impl Osc52 {
    pub fn truncated(&self) -> bool {
        self.sent_bytes < self.total_bytes
    }
}

/// Pure: build the OSC 52 sequence for `data`, capped at a char boundary <= cap.
pub fn osc52_sequence(data: &str) -> Osc52 {
    let mut end = data.len().min(OSC52_CAP);
    while end > 0 && !data.is_char_boundary(end) {
        end -= 1;
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&data[..end]);
    Osc52 {
        sequence: format!("\x1b]52;c;{b64}\x07"),
        sent_bytes: end,
        total_bytes: data.len(),
    }
}

/// Write the sequence to `out` (testable; surfaces io::Result).
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

/// Clear the local Wayland clipboard (`wl-copy --clear`), stdout/stderr nulled.
pub fn wl_clear() {
    use std::process::{Command, Stdio};
    let _ = Command::new("wl-copy")
        .arg("--clear")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// OSC 52 + local Wayland clipboard (panel-side path for glance/launchers).
pub fn copy(data: &str) -> Osc52 {
    let built = emit_osc52(data);
    wl_copy(data);
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_input_not_truncated() {
        let r = osc52_sequence("hello");
        assert!(!r.truncated());
        assert_eq!(r.sent_bytes, 5);
        assert_eq!(r.total_bytes, 5);
        assert!(r.sequence.starts_with("\x1b]52;c;"));
        assert!(r.sequence.ends_with('\x07'));
    }

    #[test]
    fn envelope_round_trips() {
        let r = osc52_sequence("hello");
        let body = &r.sequence[7..r.sequence.len() - 1];
        let decoded = base64::engine::general_purpose::STANDARD.decode(body).unwrap();
        assert_eq!(decoded, b"hello");
    }

    #[test]
    fn multibyte_straddling_cap_does_not_panic_and_sent_is_exact() {
        // 4095 ASCII bytes + a 2-byte 'é' => byte 4096 lands mid-char.
        let mut s = "a".repeat(OSC52_CAP - 1);
        s.push('é');
        let r = osc52_sequence(&s);
        assert!(r.truncated());
        assert_eq!(r.sent_bytes, OSC52_CAP - 1);
        assert_eq!(r.total_bytes, OSC52_CAP + 1);
        let body = &r.sequence[7..r.sequence.len() - 1];
        let decoded = base64::engine::general_purpose::STANDARD.decode(body).unwrap();
        assert!(std::str::from_utf8(&decoded).is_ok());
    }

    #[test]
    fn emit_to_writes_sequence_and_reports() {
        let mut buf: Vec<u8> = Vec::new();
        let r = emit_osc52_to(&mut buf, "hi").unwrap();
        assert_eq!(buf, r.sequence.as_bytes());
        assert!(!r.truncated());
    }
}
```

- [ ] **Step 2: Run the clipboard tests**

Run: `cd ~/projects/suite-term && cargo test`
Expected: `test result: ok. 10 passed` (6 quote + 4 clipboard).

- [ ] **Step 3: Commit**

```bash
cd ~/projects/suite-term && git add -A
git commit -m "feat: clipboard module (OSC 52 + wl-copy/clear, exact Osc52 result)"
```

### Task 3: `panic` module + feature-matrix verification

**Files:**
- Create: `~/projects/suite-term/src/panic.rs`

- [ ] **Step 1: Write `src/panic.rs`**

```rust
//! A panic hook that restores the terminal before printing the panic.

/// Install BEFORE terminal setup (enable_raw_mode / EnterAlternateScreen) so a
/// panic during setup is also covered. On panic: disable mouse capture, leave
/// the alternate screen, disable raw mode, then run the default hook. The mouse
/// disable is a harmless no-op where capture was never enabled.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_runs_without_panicking() {
        install_panic_hook();
    }
}
```

- [ ] **Step 2: Verify the full feature matrix**

Run each and confirm the expected counts (and that quote-only / panic-only do NOT compile base64):
```bash
cd ~/projects/suite-term
cargo test                                              # expect: 10 passed
cargo test --all-features                               # expect: 11 passed
cargo test --no-default-features                        # expect: 6 passed (quote only)
cargo test --no-default-features --features panic-hook  # expect: 7 passed
# Prove the feature graph: quote-only pulls no deps.
cargo tree --no-default-features 2>/dev/null | grep -E "base64|crossterm" && echo "UNEXPECTED DEP" || echo "OK: zero deps"
```
Expected: counts as noted; final line `OK: zero deps`.

- [ ] **Step 3: Commit**

```bash
cd ~/projects/suite-term && git add -A
git commit -m "feat: feature-gated panic hook (restores terminal, DisableMouseCapture)"
```

### Task 4: README + publish

**Files:**
- Create: `~/projects/suite-term/README.md`

- [ ] **Step 1: Write `README.md`**

```markdown
# suite-term

Shared TUI primitives for the dashboard widget suite. One source of truth for
the helpers that used to be copy-pasted (and drift) across every repo.

- `quote` (always): `shell_quote` / `quote_path` for emitted `cd`/`eval` commands.
- `clipboard` (feature, default): OSC 52 (`osc52_sequence`/`emit_osc52`/`copy`,
  capped at a UTF-8 char boundary) + best-effort `wl_copy`/`wl_clear` with
  stdout/stderr nulled.
- `panic-hook` (feature): `install_panic_hook()` restores the terminal on panic.

```toml
suite-term = { git = "https://github.com/JaneAdora/suite-term", rev = "<sha>", features = ["clipboard", "panic-hook"] }
```
```

- [ ] **Step 2: Create the GitHub repo and push**

```bash
cd ~/projects/suite-term && git add -A && git commit -m "docs: README"
gh repo create JaneAdora/suite-term --public --source=. --remote=origin --push
git rev-parse HEAD   # <-- record this SHA; it is <REV> for every consumer below
```
Expected: repo created, pushed; `git rev-parse HEAD` prints the SHA to pin.

> **NOTE:** `<REV>` in every Phase 2 task is the SHA printed here. Substitute the real SHA when adding the dependency. If `suite-term` changes mid-rollout, push the change and bump `<REV>` in the remaining + already-migrated consumers.

---

## Phase 2 — Migrate consumers (pin `rev = "<REV>"`)

Each migration task follows the same shape: add the git dep, replace local impls with crate calls, apply the quoting policy to ALL dynamic tokens, re-baseline any test whose expected command string changes from always-quoted to bare, run the acceptance checklist, commit, reinstall.

### Task 5: `launcher-core` workspace (validates the crate first)

**Files:**
- Modify: `~/projects/launchers/launcher-core/Cargo.toml`, `.../src/clipboard.rs`, `.../src/exit.rs`
- Modify: `~/projects/launchers/clip/src/source.rs`, `~/projects/launchers/onepw/src/app.rs`

- [ ] **Step 1: Add the dep** to `launcher-core/Cargo.toml`:
```toml
suite-term = { git = "https://github.com/JaneAdora/suite-term", rev = "<REV>", features = ["clipboard"] }
```

- [ ] **Step 2: Replace `launcher-core/src/clipboard.rs`** body with re-exports so existing `launcher_core::clipboard::*` paths keep working:
```rust
//! Re-export of the shared clipboard. Kept as a module so existing
//! `launcher_core::clipboard::...` call sites are unchanged.
pub use suite_term::clipboard::{copy, emit_osc52, emit_osc52_to, osc52_sequence, wl_copy, wl_clear, Osc52, OSC52_CAP};
```
Then fix callers of the OLD `osc52_sequence(&str) -> (String, bool)`: it now returns `Osc52`. In `exit.rs::finish`, change `let (seq, _truncated) = osc52_sequence(&cmd);` to `let seq = osc52_sequence(&cmd).sequence;`. Search the workspace: `grep -rn "osc52_sequence" ~/projects/launchers` and update each tuple destructure to use `.sequence` / `.truncated()`.

- [ ] **Step 3: Replace `launcher-core/src/exit.rs::shell_quote`** with a re-export, keeping the path stable:
```rust
pub use suite_term::quote::shell_quote;
```
Delete the local `shell_quote` fn and its `#[cfg(test)] mod tests` (the crate owns those tests now).

- [ ] **Step 4: Route `clip` + `onepw` wl-copy through the crate.** In `clip/src/source.rs` replace the local `wl_copy` body and the cliphist-delete `wl-copy` usage with `suite_term::clipboard::wl_copy(...)`; in `onepw/src/app.rs` replace the `Command::new("wl-copy")` spawn with `suite_term::clipboard::wl_copy(s)` and the detached `sleep 30 && wl-copy --clear` with a thread that sleeps then calls `suite_term::clipboard::wl_clear()`. (Add `suite-term` to `clip/Cargo.toml` and `onepw/Cargo.toml` too.)

- [ ] **Step 5: Verify (acceptance checklist) + commit + reinstall**
```bash
cd ~/projects/launchers
cargo build --release 2>&1 | grep -c '^warning'   # expect 0
cargo test --no-run 2>&1 | grep -c '^warning'      # expect 0
cargo test 2>&1 | grep "test result"               # all pass
for b in gst clip proc; do install -m 0755 target/release/$b ~/.local/bin/$b; done
install -m 0755 target/release/onepw ~/.local/bin/1p
git add -A && git commit -m "refactor: use suite-term for clipboard + shell_quote"
```

### Task 6: `glance`

**Files:**
- Modify: `~/projects/glance/Cargo.toml`, `src/clip.rs`, `src/main.rs` (+ `src/bin/*.rs`), `src/crew/job.rs`, `src/bin/tasks.rs`

- [ ] **Step 1: Add the dep** (`features = ["clipboard", "panic-hook"]`).
- [ ] **Step 2: Clipboard.** Replace `src/clip.rs::copy` with a call to `suite_term::clipboard::copy`; replace `clip::b64`/local OSC52 with the crate. Update any caller that used the old return type.
- [ ] **Step 3: Panic hook.** In `src/main.rs` and each `src/bin/*.rs` that calls `std::panic::set_hook`, replace the hook block with `suite_term::panic::install_panic_hook();`, placed BEFORE `enable_raw_mode()`.
- [ ] **Step 4: Quoting.** In `src/crew/job.rs` and `src/bin/tasks.rs`, replace the inline `format!("cd '{}' && ...", c.replace('\'', "'\\''"), ...)` with `format!("cd {} && ...", suite_term::quote::shell_quote(c), ...)`, and ALSO wrap the resume id: `claude --resume {}` -> `claude --resume {}` with the id passed through `shell_quote`. Re-baseline the affected tests in `crew/job.rs` (lines ~203/225/230) and `crew/mod.rs` (~287) to expect the bare-normalized form for safe cwds (e.g. `cd /home/jane && ...` instead of `cd '/home/jane' && ...`); keep the apostrophe-path test expecting the quoted form.
- [ ] **Step 5: Verify + commit + reinstall** (`install -m 0755 target/release/glance ~/.local/bin/glance`). Acceptance checklist; commit `refactor: use suite-term (clipboard/panic/quote)`.

### Task 7: `recall`

**Files:**
- Modify: `~/projects/recall/Cargo.toml`, `src/actions.rs`, `src/main.rs`

- [ ] **Step 1: Add the dep** (`features = ["clipboard", "panic-hook"]`).
- [ ] **Step 2:** Replace `actions.rs::osc52_encode` + `copy_to_clipboard` with `suite_term::clipboard::emit_osc52` (this FIXES recall's missing cap). Replace `actions.rs::shell_quote` with `suite_term::quote::shell_quote`.
- [ ] **Step 3:** In `resume_command`, quote BOTH `cwd` and `session_id` via `shell_quote`. Re-baseline the `actions.rs` tests: recall currently always-quotes, so expected strings change to bare for safe cwds (e.g. `cd /home/jane/... && ...`). The `fts_error_recomputes_without_filter` and other tests are unaffected.
- [ ] **Step 4:** Replace the `set_hook` block in `main.rs` with `install_panic_hook()` before terminal setup.
- [ ] **Step 5: Verify + commit + reinstall** to `~/.cargo/bin` and `~/.local/bin`.

### Task 8: `roam`

**Files:**
- Modify: `~/projects/roam/Cargo.toml`, `src/actions.rs`, `src/main.rs`

- [ ] **Step 1: Add the dep** (`features = ["clipboard", "panic-hook"]`).
- [ ] **Step 2:** Replace `actions.rs::osc52_encode` + `copy_to_clipboard` (which returns `CopyResult`) with `suite_term::clipboard::emit_osc52`. Map the toast site: `CopyResult::Truncated { sent, total }` -> read `Osc52::truncated()` / `.sent_bytes` / `.total_bytes`. Replace `actions.rs::quote_path` with `suite_term::quote::quote_path`.
- [ ] **Step 3:** Quote the resume id in `claude_command` (already quotes the path; pass the id through `shell_quote` too if it interpolates one).
- [ ] **Step 3b:** Add the spec's consumer-level integration test (additional rec): a test that copying a multibyte string crossing `OSC52_CAP` reports exact `sent_bytes` (a few below the cap), not just `truncated == true`. Example: build a `"a".repeat(OSC52_CAP-1) + 'é'`, `emit_osc52` it (or `osc52_sequence`), assert `sent_bytes == OSC52_CAP-1` and `truncated()`.
- [ ] **Step 4:** ADD `install_panic_hook()` in `main.rs` before terminal setup (roam currently has NO panic hook — this closes the gap the audit found).
- [ ] **Step 5: Verify + commit + reinstall** to `~/.cargo/bin` and `~/.local/bin`. Confirm a standalone clone builds: `git clone ~/projects/roam /tmp/roam-clone && cd /tmp/roam-clone && cargo build --release` (the git-dep must resolve), then `rm -rf /tmp/roam-clone`.

### Task 9: `wt`

**Files:**
- Modify: `~/projects/wt/Cargo.toml`, `src/actions.rs`, `src/main.rs`

- [ ] **Step 1: Add the dep** (`features = ["clipboard", "panic-hook"]`).
- [ ] **Step 2:** Replace `actions.rs::osc52_encode` + `copy_to_clipboard` with `suite_term::clipboard::emit_osc52` (FIXES wt's missing cap). Replace `actions.rs::shell_quote` with `suite_term::quote::shell_quote`.
- [ ] **Step 3:** In `launch_command_for`, quote BOTH `cwd` (already done) and `resume_id` via `shell_quote`. Keep the `launch_command_quotes_path_with_spaces` test; add/adjust a test that a metachar resume id is quoted.
- [ ] **Step 4:** ADD `install_panic_hook()` in `main.rs` before terminal setup (wt has none).
- [ ] **Step 5: Verify + commit + reinstall** to `~/.cargo/bin`. Standalone-clone build check as in Task 8.

### Task 10: `atlas`

**Files:**
- Modify: `~/projects/atlas/Cargo.toml`, `src/ui/osc52.rs` (or its callers), `src/app.rs`, `src/main.rs`

- [ ] **Step 1: Add the dep** (`features = ["clipboard", "panic-hook"]`).
- [ ] **Step 2:** Replace `src/ui/osc52.rs::copy` (returns `bool`) with `suite_term::clipboard::emit_osc52`; update callers to read `Osc52::truncated()`. Delete `ui/osc52.rs` if nothing else lives there. Replace `app.rs::shell_escape` with `suite_term::quote::shell_quote` (atlas's is already conditional, so its existing tests should pass unchanged; verify).
- [ ] **Step 3:** Replace the `set_hook` block in `main.rs` with `install_panic_hook()` before terminal setup.
- [ ] **Step 4: Verify + commit + reinstall** to `~/.cargo/bin` and `~/.local/bin`. Standalone-clone build check.

### Task 11: `mandalas` (optional — panic hook only)

**Files:**
- Modify: `~/projects/mandalas/Cargo.toml`, `src/main.rs`

- [ ] **Step 1: Add the dep** with NO default features (proves the lean path):
```toml
suite-term = { git = "https://github.com/JaneAdora/suite-term", rev = "<REV>", default-features = false, features = ["panic-hook"] }
```
- [ ] **Step 2:** Replace the `set_hook` block in `main.rs` with `suite_term::panic::install_panic_hook()` before terminal setup. (Confirm mandalas's mouse-capture cleanup is covered by `DisableMouseCapture` in the hook; if mandalas also enables bracketed paste or another mode in its own cleanup, keep that local teardown on the normal-exit path.)
- [ ] **Step 3: Verify** that `cargo tree | grep base64` is empty (mandalas must NOT pull base64), then commit + reinstall to `~/.cargo/bin` and `~/.local/bin`. Standalone-clone build check.

---

## Post-rollout

- [ ] Update `ROADMAP.md`: note `suite-term` shipped and the consumers migrated; mark the shared-crate dedup item done.
- [ ] Optional follow-up (not this plan): an `rsuite update --shared` verb to bump the pinned rev across all manifest repos in one step.

## Self-review notes (for the executor)

- The Phase 1 crate code is compile-verified across all four feature permutations (10 / 11 / 6 / 7 tests). If a test count differs, something diverged — stop and reconcile.
- The one intentional behavior change is quote NORMALIZATION (recall + glance go from always-quoted to bare for safe paths). Those test re-baselines are expected, not regressions. Every other consumer's tests should pass unchanged or with only the clipboard return-type adaptation.
- Do NOT leave a consumer on a stale `<REV>` if `suite-term` changed after that consumer was migrated; bump it.
