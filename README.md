# dashboard-suite

Meta-repo and installer for Jane's terminal widget suite: single-purpose
Rust/ratatui TUIs for tiled terminal dashboards, Termux, and SSH from mobile.

The widgets live in their own repos:
- `wt`: worktree picker (github.com/JaneAdora/wt)
- `recall`: Claude session browser (github.com/JaneAdora/recall)
- `roam`: file browser (github.com/JaneAdora/roam)
- `glance`: multi-panel tile dashboard (github.com/JaneAdora/glance)
- `atlas`: plan/roadmap markdown reader (github.com/JaneAdora/atlas)
- `mandalas`: animated mandala viewer (github.com/JaneAdora/mandalas)
- `launchers`: gst / clip / 1p / proc action launchers (github.com/JaneAdora/launchers)
- `suite-term`: shared crate for clipboard, shell-quoting, and panic hook (github.com/JaneAdora/suite-term)

## Install

One command on a fresh Linux machine:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/JaneAdora/dashboard-suite/main/install.sh | bash
```

This installs Rust (via rustup, if needed), builds the `rsuite` picker, and
launches it. `rsuite` clones the component repos you select into `~/projects`
(clone URLs live in `suite.toml`) and builds them. Binaries install to
`~/.local/bin` (and `~/.cargo/bin` for `wt` and `recall`).

Non-interactive and maintenance commands:

```bash
rsuite --defaults   # install the default set, no prompt
rsuite --all        # install everything
rsuite list         # list components + missing deps
rsuite doctor       # health check: PATH, installed bins, deps, glance config
rsuite update       # rebuild + reinstall everything currently installed
```

Prerequisites: `git` and a C toolchain / `pkg-config` (e.g. `build-essential
pkg-config` on Debian/Ubuntu) to build the TUI crates. Optional per-launcher
runtime deps: `cliphist` (clip), `op` (1p), `python3` plus the skai-work
calendar bridge (cal, which is Jane-specific and degrades to an "unavailable"
message elsewhere). After install, restart your shell or run
`source ~/.cargo/env` so the bin dirs are on `PATH`.

## Contents
- `ROADMAP.md`: living roadmap, built panels + backlog + difficulty tiers.
- `install.sh` and `src/`: the `rsuite` installer (picker, then clone/build/install).
- `suite.toml`: component manifest (repos, clone URLs, glance panels).
- `scripts/check-suite.sh`: fmt/test/clippy health checks across the suite repos.

`atlas` parses `ROADMAP.md` to render suite status, so keep the doc's heading
structure intact.

## Suite checks

```bash
scripts/check-suite.sh
```

By default this checks `suite-term`, `glance`, `wt`, `recall`, `roam`, `atlas`, `mandalas`, and `launchers` under `~/projects`. Override the project root with `RSUITE_PROJECTS=/path/to/projects`. Limit the check set with `SUITE_CHECKS="fmt test"`.
