# Dashboard Widget Suite Roadmap

Living roadmap for the suite of Rust/ratatui terminal widgets (`wt` / `recall` / `roam` / …) for tiled terminal dashboards, Termux, and SSH-from-mobile use cases.

**Last updated:** 2026-05-20

Originally extracted from `~/.claude/plans/jolly-crunching-teacup.md` (the roam design doc) on 2026-05-19. Maintain this file directly going forward.

---

Two interaction patterns now exist in the suite:

- **Action launchers** (the original mold — wt/recall/roam): open → pick → exit-with-command. State is ephemeral, output is a shell command on stdout + OSC 52 clipboard. `RunOutcome::PrintAndExit`. Quit on action or `q`.
- **Always-on tiles** (new pattern, introduced for cal/tasks/glance): sits in a tmux pane, refreshes on a tick, only exits on `q`. No `RunOutcome` — output is purely the rendered display. Different event loop with timer-driven refresh, optionally per-panel tick rate.

The shared scaffold (theme.rs, layout.rs, centered_rect, OSC 52 helper, footer-with-toast) carries across both. The tick / refresh model is what diverges.

### Action-launcher widgets (exit-with-command pattern)

**Dual-form (decided 2026-05-20):** every launcher below, plus the `mm` companion, ships in BOTH forms, built in parallel from one shared library crate:
- **Standalone binary:** the classic launcher. Open, pick, exit-with-command (`RunOutcome::PrintAndExit` + OSC 52). Run from a shell or phone, e.g. `eval "$(gst)"`.
- **glance panel:** an always-on tile of the same picker. A panel cannot exit-with-command (glance owns the process and quits only on `q`), so the panel form's action becomes copy-to-clipboard (OSC 52) and/or spawn (e.g. `tmux new-window`) instead of print-and-exit. Each launcher defines its own panel-mode action semantics.

Shared library holds the data source, list model, filter, and render; the standalone `main.rs` and the glance `Panel` impl are thin wrappers over it. `mm` and the launchers are separate concerns but follow the same dual-form rule. Effort estimates below assume this shared-crate approach.

### 1. `gst` — Git status / log browser
Pick a repo (auto-detect via `pwd` or arg), see status + recent log. Single-key actions: `c` checkout this commit, `d` print `git diff <sha>`, `b` branch list, `y` copy SHA, `r` revert hint. Pairs naturally with `wt`. **Data source:** `git2` crate or shell-out to `git`.

### 2. `ssh` — Host picker
Parse `~/.ssh/config`, list Hosts with last-connected timestamp. Single-key `o` print `ssh <host>` for shell to eval, `c` copy ssh command, `y` copy hostname only. **Data source:** parse `~/.ssh/config`. Pairs with mobile workflow especially well.

### 3. `note` — Scratchpad / journal
Single-key `n` opens $EDITOR on a fresh dated file in `~/vaults/sops/journal/YYYY-MM-DD.md` (configurable). Browse existing notes by date in main pane. `y` copies path. Pairs with bathtub thinking. **Data source:** filesystem.

### 4. `clip` — Clipboard ring buffer (NEW from UX review)
Solves the biggest mobile pain: "I copied something on my phone and now I want it in this SSH session." Maintains a ring buffer (watched file at `~/.local/share/clip/ring.jsonl` written by a tiny daemon or paste-into-stdin command). Single-key `Enter` paste-into-stdout (exit with selected entry printed), `y` re-copy via OSC 52, `d` delete entry. **Data source:** local file + optional OSC 52 inbound.

### 5. `net` — Connection / tunnel status (NEW from UX review)
One screen on the dashboard: current IP, ping latency to a few configured hosts, Tailscale peers (`tailscale status --json`), WiFi vs cell signal hint. Single-key `r` refresh, `c` copy IP, `t` toggle Tailscale (if writable). Useful constantly on a phone. **Data source:** `ip`, `ping`, `tailscale status`.

### 6. `1p` — 1Password vault picker (crate `onepw`)
Browse vaults/items via the 1Password CLI. Single-key `y` copy password (OSC 52, auto-clears after 30s if possible), `Y` copy whole field, `e` reveal in preview modal. Respects the existing `skai-agent-v2` token. **Data source:** `op item list` JSON, resolved by absolute path (never bare `op`). **Renamed from `op` to `1p` on 2026-05-20:** a cargo bin target named `op` clobbered `~/.local/bin/op`, then `Command::new("op")` self-recursed into a fork bomb (~16.8k procs, OOM, hard hang). Crate is `onepw`, installed as the command `1p`; the CLI is invoked by absolute path so it can never recurse into itself again.

### 7. `proc` — Process viewer / killer
htop-lite. List processes with CPU/mem, single-key `k` send SIGTERM, `K` SIGKILL, `9` SIGKILL by PID prompt, `/` filter by command. Two-step confirm for kills (like `wt`'s `x/X` pattern). **Data source:** `sysinfo` crate.

### 8. `port` — Network listeners
`ss -tlnp`-style view: process, port, address, state. Single-key `c` copy `curl localhost:<port>`, `k` kill owning process. **Data source:** parse `/proc/net/tcp*` or shell-out to `ss`. (Linux-only; not useful on Termux unless rooted.)

### 9. `gh` — GitHub PR triage (narrowed scope)
Not a general PR browser (lazygit/`gh pr list` already cover that). **Scope to triage only:** review-requested, assigned-to-me, your own open PRs. Single-key `c` checkout, `o` print URL for `xdg-open`, `y` copy URL, `Enter` preview body. **Data source:** `gh api` subprocess.

### Always-on tile widgets (live-refresh pattern, exit only on `q`)

These don't fit the action-launcher mold. They sit in a tmux pane and refresh on a tick. Different event loop, no `RunOutcome`. Shared theme/layout/footer with the launchers.

#### 10. `cal` — Calendar agenda tile
Today + upcoming events on one screen. Single-key `j` join meeting (copy Meet/Zoom URL via OSC 52, opt-in toast with `o` exits with `xdg-open <url>`), `n` next 7 days view, `r` refresh, `q` quit. Color today's events in pink, tomorrow+ in lavender. **Data source:** skai MCP (`skai_calendar_today`, `skai_calendar_upcoming`) — already wired and authed. Fallback: zele's `calendar_intel.py` direct invocation.

#### 11. `tasks` — Unified task list tile
Two sources, single view: Monday.com tasks assigned to Jane (via skai's `skai_my_work`) + local `~/vaults/sops/todo.md` (or configurable path). Renders as flat list with a source-glyph prefix (`◆` Monday, `·` local). Single-key `c` complete (writes back to source — Monday via API, local via markdown rewrite), `s` snooze 1d, `n` new task (prompts), `e` open source file in $EDITOR. **Data source:** skai MCP + filesystem. Completion semantics differ per source — explicit in design.

#### 12. `glance` — Multi-panel system + life dashboard (replaces 10 separate viz widgets)

Quick-look unified visualization tile. One binary, many panels.

One binary, multiple visualizations swapped via single-key toggle. Designed for the dashboard tile pattern — sits in a tmux pane, refreshes on a tick, switches panels with `1-9`/`0` or `n`/`p`. Shares theme + layout with the rest of the suite.

**Architecture:** `Panel` trait with `name() -> &str`, `render(&mut Frame, Rect)`, `tick(&mut self)`, `preferred_refresh_hz() -> u32`. Main app holds `Vec<Box<dyn Panel>>` and a current index. Tick loop runs at the fastest panel's rate; slower panels skip frames. Config at `~/.config/glance/panels.toml` selects which panels to enable and in what order — same binary, per-environment dashboards.

Built panels stay registered even if disabled — config just selects from the registry. Custom panels would need a plugin system (deferred — v1 ships built-in registry only).

**Top 10 panels (built-in):**

1. **`cpu`** — Sparkline per core (last 60s) + top-5 processes table. Refresh 2 Hz. Primitives: `Sparkline` × N + `Table`. Data: `sysinfo` crate.
2. **`mem`** — RAM gauge + swap gauge + 5-min usage sparkline below. Refresh 2 Hz. Primitives: `Gauge` × 2 + `Sparkline`.
3. **`disk-viz`** — Horizontal bar chart, one bar per mount, color graded by % full. Refresh 0.2 Hz (slow). Primitives: `BarChart`. Data: `statvfs`.
4. **`net-graph`** — ↑/↓ throughput sparkline per interface. Current rate inline. Refresh 2 Hz. Primitives: `Sparkline` × 2. Data: parse `/proc/net/dev` deltas.
5. **`ping-graph`** — Multi-host latency line chart over time (one colored line per host from config). Refresh 1 Hz. Primitives: `Chart` w/ Datasets. Data: `ping -c1` subprocess per host.
6. **`battery`** — Big gauge (charge %) + drain-rate sparkline (last hour). Mobile-essential. Refresh 0.1 Hz. Primitives: `Gauge` + `Sparkline`. Data: `/sys/class/power_supply/` or Termux `termux-battery-status`.
7. **`peon-log-viz`** — Sparkline of pushups/squats over last 30 days + weekly bar chart total. Refresh on tick when peon-ping log file changes. Primitives: `Sparkline` + `BarChart`. Data: peon-ping log file (already exists).
8. **`commits-heatmap`** — GitHub-style green-square calendar of daily commit counts (last 90 days, across all repos in `$WT_ROOTS` or configurable). Refresh 0.05 Hz (slow). Primitives: `Canvas` w/ filled rects. Data: `git log --since='90 days ago' --format=%cs` across each repo.
9. **`emails-per-day`** — Bar chart of inbox volume over recent 14 days, zele-driven. Refresh 0.05 Hz. Primitives: `BarChart`. Data: zele `mail_search` aggregated by day.
10. **`activity-clock`** — Radial 24-hour clock face with today's calendar events drawn as colored arc segments. Current time as a glowing marker. Refresh 1 Hz. Primitives: `Canvas` (arcs, lines). Data: skai_calendar_today.

**Keys:**
```
1-9, 0    jump to panel by slot       n / p    cycle next / prev
r         force refresh now           q        quit
```

**Theme mapping (uniform across panels for visual cohesion):**
- Pink (`#e88b9f`) — active/now/current values
- Lavender (`#c5a3ff`) — historical / averages / axis labels
- Magenta (`#ff6ec7`) — alerts / peaks / "this number is bad"

#### 13. `atlas` — Suite roadmap visualizer / navigator (self-referential)

Reads `~/projects/.dashboard-roadmap.md` and renders the suite as a navigable visualization. Sits in tile mode by default; Enter opens an action menu on the focused widget. Useful as a permanent "what's the suite at right now?" tile on the dashboard.

**Three togglable views (single-key `v` cycles):**

- **Kanban** — four columns: Planned / In Progress / Built / Dropped. Each widget a card with name, one-line description, and a glyph for interaction model (◆ launcher, ● tile).
- **Wave** — vertical bars per build wave: Wave 1 (3/5), Wave 2 (0/3), etc. Progress gauges. Best for "where am I in the plan."
- **Network** — canvas-drawn node graph. Widgets are nodes (colored by status), edges show shared scaffold (wt ← roam, wt ← gst, etc.) and merged-children (glance panels under glance). Most visually striking; leverages `Canvas` primitive.

**Action menu (on Enter for focused widget):**
- `o` → exit with `cd ~/projects/<widget>` (drops you into the project repo)
- `g` → exit with `gh repo view --web JaneAdora/<widget>` (opens in browser)
- `p` → open the widget's plan or README in `$EDITOR` if it exists
- `s` → exit with `gh issue list -R JaneAdora/<widget>` (or copy that command)
- `y` → copy widget name to clipboard

**Data source:** parse `~/projects/.dashboard-roadmap.md`. Status encoded as inline emoji or `status:` lines in a YAML frontmatter block per widget. File-watch via `notify` crate so atlas auto-refreshes when you mark something built. Fallback: walk `~/projects/` looking for known widget names and use git activity as a proxy for status.

**Meta-recursive note:** atlas should include itself as a widget in its own visualization. Once atlas is built, it'll show `✅ atlas` in the kanban. The first commit that builds atlas is also the commit that updates the roadmap to mark atlas as built.

---

## glance panel backlog (built + planned)

glance ships as one binary; new visualizations are added as Panel-trait impls registered in `default_registry()`. Status is per-panel inside this binary.

**Built (23 panels, as of 2026-05-20):**
`cpu` `mem` `net` `disk` `loadavg` `entropy` `fans` `ping` `commits` `peon` `temp` `tsmap` `pet` `moon` `clock` `weather` `alerts` `hurricane` `solar` `water` `mascot` `starfield` `launchers`
(plus `battery` — built but unregistered; no battery on the dev box. One-line registry edit to enable on a laptop.)

Notes on what shipped:
- `clock` — big block-digit clock, 12/24 toggle (`f`), TZ + ISO week + day-progress gauge. Vertically centered.
- `weather` — Open-Meteo current + 7-day forecast, big block-digit temp, WMO-code glyphs. Baton Rouge default via `$GLANCE_LAT/$GLANCE_LON/$GLANCE_LOCATION`.
- `alerts` — NWS active weather alerts, severity-colored cards.
- `hurricane` — NHC Atlantic-basin storms on a Map widget, off-season message.
- `solar` — sun-position arc with NOAA sunrise equation, golden-hour highlights. (This was the roadmap's `sun`.)
- `water` — local glasses tracker, `+`/`-`/`R` keys, midnight rollover. A single-activity prototype of `health`.
- `mascot` — rotating hand-drawn pixel-art creature (6 poses). Pure decoration.
- `launchers` — suite menu tile: a 16-row palette (every launcher + its hotkey) over live cards for `gst`/`clip`/`proc` (each polls `<bin> --summary --json` every 60s). Palette hotkeys copy the launcher name to the clipboard (OSC 52 + wl-copy); `proc`/`note` use `[P]`/`[N]` because lowercase `p`/`n` are the global panel-cycle keys.

Infrastructure shipped alongside: brightness control (`[`/`]`), tab-strip header, shared empty/loading/error widgets, `Panel::handle_key` for per-panel keys, `braille_aspect_bounds` for aspect-correct Canvas panels.

### glance UI sweep backlog

Polish items to batch into a dedicated pass over panel layouts (not blocking):
- `water` — center the progress bar in the true vertical middle and refine the bar/text balance. Bar was moved above the text 2026-05-20 as a quick pass; revisit for exact centering and spacing.

### roam backlog

Found while reviewing the launchers spec on mobile (2026-05-20):
- File previewer: full-screen modal is full-width + wraps (2026-05-21); `.md` renders styled (headings, bold/italic, code, lists, quotes, rules) and code is syntax-highlighted (js/ts/jsx/tsx/json/css/scss/less, python, + html/xml/vue/svelte) in both pane and modal. Remaining: tidy the inline preview pane (layout/readability).
- ✅ Recursive find shipped (2026-05-21): `R` walks the cwd to depth 3 (hidden-aware, symlink-safe, capped at 1000), shows matches with relative-path names + a `[find 'q': N]` header indicator; `Esc` clears. `/` remains the current-dir jump-filter.

---

## Still on the roadmap

### `health` — Custom goals tracker (REPLACES `peon`, absorbs `water`)
Big feature. Today's `peon` panel reads `peon-ping` trainer state (pushups + squats, single daily goal each). Expand into a full goals system **owned by glance**:
- **Configurable goals** in `~/.config/glance/health.toml`: arbitrary activities (pushups, squats, miles walked, minutes meditated, glasses of water), daily goal, unit string, optional weekly target.
- **Inline logging** via a key mode (`+` → pick activity → type count → enter). No shell trip to log.
- **Multi-day history** in `~/.local/share/glance/health.jsonl` (one JSON line per event). Enables 7-day sparkline + weekly bars per activity.
- **Multiple views** toggled by `v`: Today's gauges → Weekly bars → 30-day sparkline grid → All-time totals.
- **Migration**: drop `peon` and `water`; import existing peon-ping state on first run.

### glance system/hardware panels

### glance network panels

### glance time / decoration panels
- `waveform` — live mic-input waveform (Sparkline real-time, cpal audio dep)
- `missminutes` — animated Miss Minutes clock companion (Loki). Hand-drawn pixel-art character + big block-digit time, idle animation loop, hourly/quarter quips. Decoration sibling of `mascot` / `clock`. Shares its animation code with the standalone `mm` app (see separate binaries, Wave 0).

### glance work / data panels
- `emails-per-day` — inbox volume BarChart, zele-driven
- `activity-clock` — radial 24h clock with calendar event arcs (Canvas, skai cal)
- `issues` — GitHub assigned-issues tile, BarChart (`gh api`). (`prs` built.)
- `standup` — auto-summary of today's git + claude + calendar activity

### Suite: launchers + companion (dual-form: standalone binary + glance panel)
Shared `launcher-core` crate (theme, list + scroll `Selection`, OSC 52, filter, `--summary --json` envelope) under `~/projects/launchers`; each launcher is a thin binary over it plus a glance palette/card entry. The glance `launchers` panel is the panel form (see above).
- ✅ **Built (2026-05-20):** `gst` (git status/log), `clip` (clipboard ring, wraps `cliphist`), `1p` (1Password, crate `onepw`), `proc` (process killer). Plus the pre-existing OG launchers `roam` (dir nav), `wt` (worktrees), `recall` (cc-session browser) — all wired into the glance palette.
- **Companion:** `mm`, Miss Minutes. A bash toggle exists (`~/.local/bin/mm` -> `~/Projects/tinker/miss-minutes/scripts/mm`) and is in the palette; the Rust animated version + `missminutes` glance panel are still to build.
- **Remaining action launchers:** `ssh` (host picker), `note` (journal), `gh` (PR triage), `port` (listening ports), plus audit candidates `docker`, `svc`, `hub`, `agent`.
- **Meta:** `atlas`, self-referential roadmap visualizer (Kanban / Wave / Network views; parses this doc)

Tiles `cal` / `tasks` stay separate (Tier 4, skai-bridge dependent): they are live tiles, not pick-and-exit launchers.

### Launcher candidates: machine audit (2026-05-20)
Grounding the brainstorm in what is actually installed on muthur (this dev box):
- **git / gh:** `gh` is a wrapper; `gst` and `gh` launchers are solid. ~16 repos under `~/projects`, more under `~/Projects`.
- **docker:** 7 containers running (habitica stack, guacamole stack, limitless-bot). A `docker` launcher (start/stop/logs/exec/shell-in) is genuinely useful here. NEW candidate.
- **systemctl --user:** 41 user units plus system units. A `svc` launcher (status/restart/logs via journalctl) is viable; user units are mostly autostart noise, system units more useful. NEW candidate.
- **clip:** `cliphist` ALREADY runs as the clipboard daemon (existing `clipboard-picker` = `cliphist list | wofi | wl-copy`). So `clip` wraps `cliphist list/decode`: no daemon to build. Resolves the old "needs a watcher" open question. Wayland gives `wl-copy`/`wl-paste` natively, plus OSC 52 for SSH/mobile.
- **1p:** wrapper present (`skai-agent-v2` token); built as `1p` (crate `onepw`), renamed from `op` after the 2026-05-20 fork-bomb incident.
- **proc / port:** `sysinfo` + `ss`; `fzf` present for a shell-only fallback if ever wanted.
- **ssh:** WEAK on this box: no `~/.ssh/config`, only 6 `known_hosts` plus key files. Reframe around `known_hosts` + a hand-kept host list, or deprioritize.
- **task runners:** only 4 Cargo.toml + 2 package.json under `~/projects` (depth 3), no justfiles/Makefiles. A `run` launcher (cargo/npm scripts) is low surface now; defer.
- **DBs:** `psql` + `redis-cli` present (postgres/mongo/redis via the docker stacks). A `db` query launcher is possible but niche.
- Also present: `gcloud` (snap), `flatpak`/`snap`/`apt`, `bun`/`pnpm`/`npm`/`uv`/`pipx`, `nvim`/`vim`, `just`/`make`, `gpg`, `curl`, `jq`, `rg`/`fd`/`bat`/`eza`.

Custom `~/.local/bin` wrappers worth knowing: `zele` (gmail/slack/monday bridge), `skai`/`mu` (MUTHUR), `hermes`, `claude`/`claudesp`/`jcode`/`kimi` (agent CLIs), `g2md`/`hscms` (content), `roam`/`wt`/`glance` (the suite), `xurl`.

New candidates surfaced by the audit (for brainstorm): **`docker`**, **`svc`** (systemd), and a possible **`agent`/session** launcher over the many AI-CLI wrappers (overlaps `recall`).

---

## Development difficulty tiers

Everything remaining, ranked easiest → hardest to build. Tier reflects data-source complexity, new patterns/deps required, state management, and rough line count. Items within a tier are roughly equal.

### Tier 1 — Trivial — ✅ ALL BUILT (2026-05-20)
- ~~`loadavg`~~ ✅  ~~`entropy`~~ ✅  ~~`starfield`~~ ✅  ~~`fans`~~ ✅
- Next-cheapest now living in Tier 2.

### Tier 2 — Easy (≈half day; known pattern + one new wrinkle)
glance panels: ✅ ALL BUILT (2026-05-20) — ~~`io`~~ ~~`conn`~~ ~~`mandala`~~ ~~`timer`~~
Remaining = launcher binaries (separate repos, roam/wt scaffold):
- ~~`gst` *(launcher)*~~ ✅ Built 2026-05-20 — git status/log; bore the `launcher-core` scaffold cost so the rest are cheaper.
- `ssh` *(launcher)* — parse ~/.ssh/config, exit with `ssh <host>`. ~200 lines.
- `note` *(launcher)* — dated journal files, exit to $EDITOR. ~200 lines.

### Tier 3 — Moderate (≈1 day; new data source or external subprocess)
glance panels: ✅ MOSTLY BUILT (2026-05-20) — ~~`gpu`~~ ~~`world-ping`~~ ~~`traceroute`~~ ~~`music`~~ ~~`prs`~~
Remaining glance panel in this tier:
- `issues` — GitHub assigned-issues tile, BarChart (`gh api`). Trivial now that `prs` exists (same scaffold). ~120 lines.
- `emails-per-day` — DEFERRED to the skai/zele bridge work (zele has no JSON mode + slow cold start). See cross-cutting note.

Remaining launcher binaries (separate repos):
- `mm` *(companion)* — Miss Minutes standalone; pixel-art animation loop + block-digit clock, idle/hourly quips. New animation-render work over the shared scaffold; the `missminutes` glance panel reuses it. ~250 lines. (A bash `mm` already exists and is in the palette; the Rust version is still to build.)
- ~~`proc` *(launcher)*~~ ✅ Built 2026-05-20 — process killer, two-step confirm, sysinfo.
- `port` *(launcher)* — `ss` parse + kill; Linux-only. ~200 lines.
- `1p` *(launcher, crate `onepw`)* — 1Password `op` CLI; secret handling + auto-clear; security-sensitive. ~220 lines. ✅ Built (2026-05-20); renamed from `op`, CLI invoked by absolute path.
- ~~`clip` *(launcher)*~~ ✅ Built 2026-05-20 — wraps `cliphist list/decode`; no daemon needed.

### Tier 4 — Hard (multi-day; new architecture, integration, or multi-source)
- `waveform` — live mic capture via `cpal` (new audio dep + real-time thread). ~250 lines.
- `cal` *(tile)* — skai calendar; **glance has no MCP client**, so needs a shell bridge to zele/skai CLI. Integration question dominates. ~250 lines.
- `activity-clock` — same skai-bridge issue + radial Canvas arc layout math. ~280 lines.
- `standup` — multi-source aggregation (git + cc-session-index + calendar) + summarization. ~300 lines.
- `gh` *(launcher)* — full GitHub PR triage, checkout, multiple views. ~300 lines.
- `tasks` *(tile)* — Monday.com via skai + local file, **write-back** semantics, completion. ~350 lines + integration.

### Tier 5 — Flagship (own design pass before building)
- `health` — config schema + inline log-entry key mode + multi-day persistence + multi-view toggle + peon/water migration. ~400 lines. The highest-leverage remaining item: you'd use it daily, and it retires two existing panels.
- `atlas` *(meta binary)* — parse this markdown roadmap, three view modes (Kanban / Wave / Network-graph via Canvas), action menu, file-watch. ~450 lines. Most complex single thing; depends on the roadmap doc staying structured.

### Cross-cutting note: the skai/MCP bridge
`cal`, `tasks`, `activity-clock`, and the zele-driven `emails-per-day` all hit the same wall: glance is a plain Rust binary with no MCP client. Cleanest path is shelling out to the existing `zele` CLI wrapper or a thin skai bridge script and parsing its output. Solving this once unblocks all four. Worth a small spike before committing to any of them.

---

### Dropped / merged

- **`dash`** (original tile launcher idea) — duplicates tmux's `choose-tree`/`zellij`. Build a `.tmux.conf` preset instead. Zero Rust.
- **`logs` (journalctl variant)** — no systemd on Termux. If kept later, scope as plain file-tailer only.
- **Original 10 separate viz binaries** (`cpu`, `mem`, `disk-viz`, `net-graph`, `ping-graph`, `battery`, `peon-log-viz`, `commits-heatmap`, `emails-per-day`, `activity-clock`) — **merged into `glance`** as panels. Saves 9 binaries' worth of duplicate scaffolding and gives a unified dashboard.

### Build order recommendation

**Wave 0 — highest priority:**
`mm` (Miss Minutes) standalone app — animated clock companion. Build first, ahead of everything else. The `missminutes` glance panel reuses its animation code, so this also seeds that panel.

**Wave 1 — action launchers** (smallest data sources, biggest day-to-day wins):
`roam` ✅ → `gst` ✅ → `ssh` → `note` → `clip` ✅

**Wave 2 — tiles** (introduces the new event loop pattern):
`cal` (smallest tile, validates pattern) → `tasks` → `glance` (the big one, but trait-based so panels can be added incrementally — ship with 3-4 panels, grow from there)

**Wave 3 — auth-gated launchers**:
`net` (could go earlier — partial tile too) → `1p` (built) → `gh`

**Wave 4 — specialized**:
`proc` ✅, `port` (Linux-only)

---

