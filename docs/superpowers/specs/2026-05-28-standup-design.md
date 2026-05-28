---
type: spec
title: standup-glance-panel-design
status: draft
date: 2026-05-28
---

# `standup` glance panel — design

A compact today-scoreboard glance panel that synthesizes the day's activity across three sources: git commits, Claude Code sessions, and Google Calendar events. Answers "what did I do today" in a glance.

Listed on the suite roadmap (Tier 4, work / data panels) as "auto-summary of today's git + claude + calendar activity." Scope locked to those three sources for v1; tasks and health stay in their own panels.

---

## Goal

Render a tile that, at any moment of the day, answers:

- How many commits I've shipped today, across how many repos, and when the last one landed.
- How many Claude Code sessions I've touched today and when the most recent one was active.
- How many meetings I've had, how many remain, and what's next.

Plus a single dim "yesterday" line for at-a-glance comparison.

## Non-goals (v1)

- No drill-down / no detail modal (this is a tile, not a launcher).
- No exit-with-command actions.
- No multi-day history beyond yesterday's one-liner.
- No tasks / health rows. They have their own panels right next to this one.
- No zele / email / Slack data. zele has no JSON mode + slow cold start (per the cross-cutting note in `ROADMAP.md`); deferred.

---

## Layout

Vertical block, left-aligned, fixed columns:

```
TODAY · Wed May 28
─────────────────────────────────────────
  commits     6  across 3 repos · last 2:14 PM
  sessions    4  claude code · last 3:01 PM
  meetings    3  2 done · 1 left · next 4:00 PM
                 "thelma sync"

  yesterday   8c · 6s · 4m
```

### Color mapping

- **Pink** (`theme::now()`): the live counts (`6`, `4`, `3`), the next-meeting time (`4:00 PM`).
- **Lavender** (`theme::historical()`): source labels (`commits`, `sessions`, `meetings`), the "across N repos" and "last HH:MM" suffixes, the yesterday line.
- **Magenta** (`theme::alert()`): the entire next-meeting line goes magenta if the next event starts within 15 minutes.

### Empty / loading / error states

- Loading (before first scan returns): row shows `…` instead of the count.
- Zero counts: render `0`, not a hidden row.
- A failed source (e.g. CalCore returns an error): row shows `—` in place of the count, with a single dim status hint at the bottom of the panel (e.g. `meetings unavailable`).

---

## Data sources

### 1. Commits

Per repo under `project_roots()` (reuses the helper from `commits.rs`: `$WT_ROOTS`, else `~/projects` + `~/Projects`), shell out to:

```
git -C <repo> log --since=<midnight ISO> --until=<now ISO> \
    --format=%cI|%H|%s --all --no-merges
```

Parse each line into `(committer_time, sha, subject)`. Aggregate across repos into:

```rust
struct CommitsSnapshot {
    total: u32,
    repos_touched: u32,       // unique repo paths with >=1 commit
    last_at: Option<DateTime>, // max(committer_time)
}
```

Yesterday uses `--since=<yesterday midnight>` / `--until=<today midnight>` against the same per-repo enumeration; same struct.

Reuse — copy, don't refactor — the small helpers `project_roots()` and `find_repos()` from `commits.rs`. Both are <30 lines each and lifting them out would bloat `commits.rs`'s diff for no payoff. Mark with a `// keep in sync with commits.rs::project_roots` comment.

### 2. Claude Code sessions

Walk `~/.claude/projects/<slug>/`. For each `.jsonl` file, read `metadata()` → `modified()`. Count files where `mtime >= today_midnight`. Track `last_at = max(mtime)` for the "last 3:01 PM" suffix.

No SQLite, no MCP. The cc-session-index is overkill for "did I touch this today" — the filesystem already holds the answer.

```rust
struct SessionsSnapshot {
    count: u32,
    last_at: Option<DateTime>,
}
```

Yesterday: same walk, filter `yesterday_midnight <= mtime < today_midnight`.

### 3. Meetings

Reuse `CalCore`. The exact accessor is whatever `cal.rs` already exposes for today's events; if `CalCore::events_today()` doesn't exist yet, add it as a one-line method that filters the cached week down to today.

For yesterday, add a parallel `CalCore::events_for(date)` accessor returning the same shape.

```rust
struct MeetingsSnapshot {
    done: u32,        // events with end < now
    upcoming: u32,    // events with start >= now
    next: Option<EventLite>, // first upcoming event today
}

struct EventLite {
    start: DateTime,
    title: String,
}
```

Declined events excluded (matches `cal.rs` behavior). Cancelled events excluded. All-day events excluded — they bloat the count without representing meeting time.

---

## Architecture

Single file: `src/panels/standup.rs` (≈ 300 lines). No new shared crate, no edits to `commits.rs` or `cal.rs` beyond the one `CalCore::events_for(date)` accessor.

```rust
pub struct StandupPanel {
    today: Snapshot,
    yesterday: Snapshot,
    last_git_scan: Option<Instant>,
    last_session_scan: Option<Instant>,
    rx: mpsc::Receiver<Msg>,
    tx: mpsc::Sender<Msg>,
    loading_git: bool,
    loading_sessions: bool,
    cal: CalCore,
    error_hint: Option<String>,
}

struct Snapshot {
    commits: CommitsSnapshot,
    sessions: SessionsSnapshot,
    meetings: MeetingsSnapshot,
}

enum Msg {
    Commits { today: CommitsSnapshot, yesterday: CommitsSnapshot },
    Sessions { today: SessionsSnapshot, yesterday: SessionsSnapshot },
}
```

### Background threads

Same channel pattern as `commits.rs`:

- `kick_git_scan()` spawns a thread that walks `project_roots()`, runs `git log` per repo for today + yesterday ranges, builds both snapshots, sends one `Msg::Commits {today, yesterday}`.
- `kick_session_scan()` spawns a thread that walks `~/.claude/projects/`, builds both snapshots, sends one `Msg::Sessions {today, yesterday}`.

Single `mpsc::Sender<Msg>` shared by both threads — the `Msg` enum keeps them from colliding.

### `tick()`

- Drain `rx` non-blocking; for each `Msg`, update `today` + `yesterday` and clear the matching `loading_*` flag.
- If `last_git_scan` is None or older than 5 min and `!loading_git`, call `kick_git_scan()`.
- Same guard for `last_session_scan` and `kick_session_scan()`.
- Call `cal.tick()` — it self-manages its 5-min cache, no extra guard needed here.
- Refresh `today.meetings` and `yesterday.meetings` from CalCore on every tick (cheap, just filtering an in-memory Vec).

### `render()`

Pure formatter over `today` + `yesterday` + `cal` error state. Pseudo:

```rust
fn render(&self, f: &mut Frame, area: Rect) {
    // line 1: "TODAY · Wed May 28"
    // line 2: rule
    // line 3-5: three rows (commits / sessions / meetings)
    // line 6: optional next-event title (only when room)
    // line 7: blank
    // line 8: "  yesterday   8c · 6s · 4m"
    // bottom: optional dim error_hint or cal status line
}
```

### Slot in registry

Add to `mod.rs`:

- `pub mod standup;` near the other panels.
- `"standup"` in `DEFAULT_ORDER` between `"tasks"` and the end of the list. The "today" cluster reads `cal, crew, tasks, standup` — natural summary at the tail.
- `"standup"` in `ALL_PANELS`.
- `"standup" => Box::new(standup::StandupPanel::new())` in `build_panel`.
- `"standup"` added to `suite.toml` panel registry in `dashboard-suite/suite.toml`.

If a different slot is preferred (e.g. front of the cluster as a "morning glance" panel), that's a one-line reorder. Documented here as a deferred preference.

---

## Day boundary

"Today" means the local-timezone calendar day (matches `cal.rs` and how a human reads the dashboard). "Today midnight" is computed as the most recent local-midnight `DateTime` (00:00 of the current local date). Yesterday midnight is 24 hours before that. All three sources use the same boundary:

- Git: pass the ISO 8601 local-timezone string to `--since=` / `--until=`.
- Sessions: compare `metadata().modified()` (a `SystemTime`) to the boundary converted to `SystemTime`.
- Meetings: `CalCore` already keys by local date.

Rollover during a long-running glance session: detected on `tick()` by comparing the current local date to the date last rendered; if it changes, both scans are kicked immediately and `yesterday` becomes what was `today` (no extra git/session call required for the rollover itself).

## Refresh model

- `refresh_ms = 60_000` (1 minute). Drives the "next meeting in 12 minutes" countdown granularity and re-evaluates the magenta `<15 min` highlight.
- Git scan: 5-minute stale guard on `last_git_scan`. Idle CPU; the dev box has ~16 repos and `git log` is fast.
- Session scan: 5-minute stale guard. Read_dir + metadata is nearly free.
- Calendar: `CalCore` is the only authority; its existing 5-min cache covers refresh cadence.

First render shows `…` placeholders for commits and sessions; meetings populates instantly from `CalCore`'s cache if present, otherwise also shows `…`.

---

## Errors & edge cases

- **Missing `~/.claude/projects/`**: empty session count, no panic.
- **Missing `~/projects` and `~/Projects`**: empty commit count, no panic.
- **`git log` fails on a single repo**: skip that repo, continue scanning others. No error surfaced (this matches `commits.rs`).
- **`CalCore` failure**: meeting row shows `—`, `error_hint = "meetings unavailable"` for the bottom dim line.
- **Day rollover during a session**: detected on tick — if today's date has changed since last tick, force-kick both scans. Otherwise yesterday's line would silently become stale.
- **0 commits / 0 sessions / 0 meetings**: render `0`, not a hidden row.
- **All-day events**: excluded from meeting count.
- **Declined / cancelled events**: excluded.
- **Multiple commits per second**: `last_at` resolves to the max committer time; ties broken by source-list order (deterministic but uninteresting).

---

## Tests

In `cfg(test)` at the bottom of `standup.rs`. Three pure helpers, fixture-based:

### `summarize_commits(lines: &[&str], now: DateTime) -> CommitsSnapshot`

Takes pre-parsed `git log --format=%cI|%H|%s` output as a slice of strings. Asserts:

- Empty input → zero snapshot.
- Three commits across two repos (input includes repo prefix in subject for the test) → `total = 3`, `repos_touched = 2`, `last_at` = max parsed time.
- Malformed line skipped, not panicked.

### `count_sessions(times: &[SystemTime], since: SystemTime) -> SessionsSnapshot`

Pure function over a vec of mtimes (no I/O). Asserts:

- Empty → zero.
- All before `since` → zero.
- Mixed → correct count, correct `last_at`.

### `summarize_meetings(events: &[EventLite], now: DateTime) -> MeetingsSnapshot`

Hand-rolled vec of events, varied start times. Asserts:

- All past → `done = N`, `upcoming = 0`, `next = None`.
- All future → `done = 0`, `upcoming = N`, `next = first by start time`.
- Mixed → correct split.
- Events with same start time → stable ordering.

Render verified manually on the dev box via `glance` → switch to `standup`. No render test (matches every other panel).

---

## Out of scope

- Drill-down detail modal.
- Multi-day history beyond yesterday.
- Tasks completed / health goal rows (covered by `tasks` and `health` panels).
- Email / Slack activity (zele integration blocked per ROADMAP cross-cutting note).
- Exit-with-command actions (this is a tile, not a launcher).
- Configurable thresholds (15-min meeting highlight is hardcoded).
- Per-source toggles or layout config.
