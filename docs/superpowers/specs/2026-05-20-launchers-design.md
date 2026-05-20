# Launcher Family - Design Spec

Date: 2026-05-20
Status: Approved (brainstorm); pending implementation plan
Home: a new `~/projects/launchers` cargo workspace. This spec lives in `dashboard-suite` as a cross-cutting suite doc.

## 1. Overview

The launcher family adds the suite's "action launcher" tier (the wt / roam / recall mold) plus a glance-side quick reference. Each launcher is a single-purpose picker that you open, filter, and exit with a command (printed to stdout and copied via OSC 52), so it works from a desktop shell, tmux, Termux, or SSH from mobile.

Reframe that shaped this design: the glance side is NOT a full picker per launcher. It is one lightweight, vertical, mobile-first `launchers` panel: a palette of every launcher plus a few live preview cards. Its job is to remind you what exists, show a glanceable status, and point you to the real standalone launcher.

Goals:
- One shared scaffold, finally de-duplicated from the wt / roam / glance copies.
- 12 launchers covering this machine's actual tools (audited 2026-05-20).
- Dual-form: standalone binary plus glance quick-reference, built in parallel, decoupled.
- Mobile-first: single-column layouts, OSC 52 with a 4 KiB cap.

Non-goals: replacing tmux, full TUIs for things already well served (lazygit and friends), the parked Miss Minutes companion.

## 2. Architecture

New cargo workspace at `~/projects/launchers`:

```
launchers/
  Cargo.toml            # workspace
  launcher-core/        # shared lib
  gst/ clip/ op/ proc/ docker/ svc/ ssh/ note/ gh/ port/ agent/ hub/   # bins
  install.sh|justfile   # build + copy bins to ~/.local/bin
```

- `launcher-core` (lib): theme (Rep Cap palette), single-column responsive layout helpers, OSC 52 helper (4 KiB cap), `RunOutcome` + exit-with-command plumbing, a generic list model + fuzzy filter, footer-with-toast, and the shared `--summary --json` convention plus its JSON envelope type.
- Each launcher crate: a `source` module (data access + parse) and a thin `main.rs` that wires the source into launcher-core's event loop. Each also implements `--summary --json`.
- glance integration: glance depends on NOTHING from this workspace. Its `launchers` panel:
  - holds the static palette (names, descriptions, shortcuts) as a compiled-in table, and
  - fetches live card data by shelling out to `<bin> --summary --json` on a background thread (the existing weather / commits pattern: mpsc channel + inflight guard), cached and refreshed on a slow tick.

Rationale: keeps glance's binary lean and its release cycle independent, reuses a proven concurrency pattern, and lets each launcher own its (sometimes heavy) data deps (git2, docker, op) without leaking them into glance.

## 3. Dual-form contract

Each launcher provides:

1. Interactive standalone (default invocation): raw-mode TUI. Open, type to filter, move with j/k plus arrows, press an action key. Actions either:
   - print a shell command to stdout AND OSC 52 copy it, then exit (eval-able: `eval "$(gst)"`), or
   - perform a direct effect (e.g. `hub` switching default, `proc` killing) with a confirm where destructive.
2. `--summary --json`: non-interactive. Prints a small bounded JSON envelope for glance cards and exits. MUST NOT emit secrets.
3. (Implicit) glance never runs the interactive form; it only calls `--summary --json` and renders the palette.

Uniform glance action rule: in the `launchers` panel, `y` copies the launch command for the focused launcher; `Enter` spawns it in a new tmux window when `$TMUX` is set, otherwise falls back to copy plus a toast.

Shared JSON envelope:
```json
{ "launcher": "gst", "headline": "3 dirty", "items": ["a1b2c3 weather: wrap forecast"], "count": 16 }
```

## 4. launcher-core API (the shared unit)

- theme: color accessors (pink / lavender / magenta plus dim / historical). Launchers do not need glance's brightness control.
- layout: `single_column(area)`, `centered_rect`, footer renderer with a 3s transient toast.
- clipboard: `osc52_copy(&str)` with a 4 KiB pre-base64 cap and a truncation signal.
- exit: `RunOutcome { Quit, PrintAndExit(String) }`; `print_and_exit` writes stdout plus OSC 52 (belt-and-suspenders).
- list: `ListState<T>` with fuzzy substring filter, selection, j/k/g/G.
- summary: `Summary { launcher, headline, items, count }` plus `emit_json()` and an arg parser that detects `--summary --json`.

Each piece is independently testable.

## 5. glance `launchers` panel

Single panel, vertical, single column always (mobile-first; no multi-column). Top to bottom:
1. palette list: every launcher as `name  description  [shortcut]`.
2. divider.
3. live cards: only for the launchers worth previewing (gst, clip, proc, docker, svc, gh, port, hub, note). Each card is a headline plus up to ~3 items.
4. footer: `y copy   Enter spawn`.

Responsive: when the area is short, cards drop from the bottom up until only the palette remains; the palette itself scrolls if needed. Card data is fetched per-launcher via background subprocess, cached in the panel, refreshed on a slow tick (30 to 60s; proc faster). `op`, `ssh`, and `agent` are palette-only (no card). A shortcut key focuses a launcher row; `y` / `Enter` act on the focused one.

## 6. The 12 launchers

| name | data source | key standalone actions | glance card |
|------|-------------|------------------------|-------------|
| gst | `git` shell-out across WT_ROOTS / ~/projects / ~/Projects | copy SHA, print diff, checkout, branch list, cd-to-repo | dirty repos + recent commits |
| clip | `cliphist list` / `cliphist decode` | paste-to-stdout, re-copy (wl-copy / OSC 52), delete | last entry (preview; see note) |
| op | `op item list --format=json` (wrapper) | copy password (auto-clear), copy field, reveal in modal | none (palette-only) |
| proc | `sysinfo` crate | SIGTERM, SIGKILL (two-step), filter | top CPU processes |
| docker | `docker ps -a` + inspect | logs, start, stop, exec sh, copy id | running containers |
| svc | `systemctl --user` / system + `journalctl` | restart, logs, copy unit | failed units |
| ssh | `~/.ssh/known_hosts` + `~/.config/launchers/ssh_hosts.toml` | `ssh <host>`, copy host | none (palette-only) |
| note | journal files under `~/vaults/sops/journal` | new dated note to $EDITOR, copy path | today's note status |
| gh | `gh api` (review-requested / assigned / mine) | checkout, open URL, copy URL, preview | counts per bucket |
| port | `ss -tlnp` | curl localhost:port, kill owner (two-step) | listener count |
| agent | non-CC agent CLIs (kimi, jcode, hermes, claudesp) + recents | copy / launch chosen agent in a cwd | none (palette-only) |
| hub | read `~/.hscli/config.yml` | switch default (`hs accounts use`), copy id, open portal, info | default portal + counts |

Notes:
- gst: reuse commits.rs repo discovery. Exit-with-command examples: `cd <repo>`, `git checkout <sha>`, `git diff <sha>`.
- clip security: the clipboard can hold secrets. Default card shows a short truncated preview; a `~/.config/launchers/clip.toml` flag hides content (count plus age only). The standalone always shows full entries.
- op: never renders secrets in `--summary --json`; copy auto-clears the clipboard after 30s (spawn a delayed clear). Uses the skai-agent-v2 token via `~/.local/bin/op`.
- hub: `--summary --json` redacts personalAccessKey and tokens; emits name, accountId, accountType, and the default flag. Open-URL form: `https://app.hubspot.com/contacts/<accountId>`. Audit 2026-05-20: 8 portals, default rep-cap-sandbox (50543830).
- agent: least-defined; overlaps `recall` (Claude Code sessions). Scope to launching / resuming non-CC agent CLIs only; recall keeps CC. Refine during its wave.
- Termux / off-host: docker / svc / port detect missing tooling and render an empty card with a "not available here" note; the palette still lists them.

## 7. Cross-cutting decisions

- Spawn: tmux when `$TMUX` is set, else copy plus toast.
- Naming: short, lowercase. `hub` is chosen because `hs` is the HubSpot CLI itself.
- Palette shortcuts: single keys, collisions resolved at build time.
- Secrets: op, hub, and clip must never emit secrets in JSON; clip card content is opt-out.
- Config: per-launcher optional config under `~/.config/launchers/<name>.toml`.

## 8. Out of scope / parked

- `mm` (Miss Minutes): leave the existing webview app alone; the ratatui companion is parked.
- `run` (task runner) and `db` launchers: low surface on this machine; deferred.
- ssh is weak here (no `~/.ssh/config`): known_hosts plus a manual list only.
- Sharing launcher-core back into wt / roam / glance: tempting but out of scope; revisit later.

## 9. Build phasing (input to the implementation plan)

- Wave 0: `launcher-core` + `gst` (exemplar) + glance `launchers` palette panel + the gst live card. Proves the whole pattern end to end.
- Wave 1: `clip`, `op`, `proc`.
- Wave 2: `docker`, `svc`, `hub`.
- Wave 3: `ssh`, `note`, `gh`, `port`, `agent`.

Each wave: build the standalone, add its `--summary --json`, wire its glance card (if any), install, smoke-test at desktop and mobile widths.

## 10. Testing

- launcher-core unit tests: fuzzy filter, OSC 52 cap / truncation, PrintAndExit formatting, summary JSON round-trip, single_column layout at narrow widths.
- per-launcher: data-source parsing against fixtures (sample hubspot config, `ss` output, `docker ps` output, cliphist list) plus a `--summary --json` schema test.
- manual: COLUMNS=32 / 58 standalone runs; glance panel at narrow width with cards collapsing; OSC 52 paste round-trip; tmux spawn vs copy fallback.

## 11. Risks / open questions

- `agent` scope is the fuzziest; it may shrink or merge into recall.
- clip on an always-on tile risks secret exposure (mitigated by opt-out plus truncation).
- 12 launchers is a large surface; phasing plus the shared core keep churn contained, but the core API must stabilize in Wave 0 before Wave 1+.
- Cross-repo coupling is intentionally avoided, but the static palette table in glance must stay in sync with installed launchers (a future `launchers --list` cross-check could guard this).
