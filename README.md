# dashboard-suite

Meta-repo for Jane's terminal widget suite: cross-cutting docs, roadmap, and
(eventually) the `atlas` roadmap-visualizer + a tmux dashboard preset.

The widgets themselves live in their own repos:
- `wt` — worktree picker (github.com/JaneAdora/wt)
- `recall` — Claude session browser
- `roam` — file browser (github.com/JaneAdora/roam)
- `glance` — multi-panel tile dashboard (github.com/JaneAdora/glance)

## Contents
- `ROADMAP.md` — living roadmap, built panels + backlog + difficulty tiers.
- `scripts/check-suite.sh` — runs fmt/test/clippy health checks across the eight suite repos.

`atlas` (planned) parses `ROADMAP.md` to render suite status. Keep the
doc's heading structure intact so atlas can parse it.

## Suite checks

```bash
scripts/check-suite.sh
```

By default this checks `suite-term`, `glance`, `wt`, `recall`, `roam`, `atlas`, `mandalas`, and `launchers` under `~/projects`. Override the project root with `RSUITE_PROJECTS=/path/to/projects`. Limit the check set with `SUITE_CHECKS="fmt test"`.
