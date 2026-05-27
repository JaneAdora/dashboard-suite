# atlas: Flagship TUI Reader for Plans, Roadmaps, and Tasks

**Status:** Design approved 2026-05-27. Implementation planning to follow.
**Author:** Jane Adora (brainstormed with Claude)
**Repo:** `~/projects/atlas/` (to be created); GitHub `JaneAdora/atlas`.

---

## Purpose

`atlas` is a single-purpose Rust/ratatui terminal application: a polished reader for plan-shaped markdown. It reads a curated library of plans, roadmaps, and task lists; renders them with care; and lets you toggle checkbox tasks inline.

Unlike other widgets in the dashboard suite (`cal`, `tasks`, `roam`, `glance`), atlas is the **flagship**. The goal is not utility-first; it is what really designed terminal UX feels like. Atlas spends design budget on whitespace, typography hierarchy, palette restraint, and reading ergonomics that the other widgets do not.

The earlier roadmap entry framed atlas as a "suite roadmap visualizer / navigator." That brief is superseded by this spec. Atlas is not a navigator into widget projects; it is a reader for plan documents (one of which happens to be the suite roadmap).

## Constraints

- Rust 1.78+, ratatui 0.29, crossterm 0.28.
- Single standalone binary. No `glance` panel form (broken from the suite's dual-form convention by design; the reader needs full-screen real estate).
- Mobile-first: must read well in Termux on a phone and over SSH from Blink at 30–40 columns. Never widens into a split-pane layout.
- Rep Cap aesthetic family but its own palette ("editorial"; see Theme).
- Zero new heavy deps. The suite's existing crates (ratatui, crossterm, anyhow, serde, dirs, base64, jiff) plus `pulldown-cmark` for markdown parsing, `serde_yaml` for frontmatter, and `toml` for config.
- No `tokio`. Synchronous IO; the library is small enough that parsing it on startup is instant.

## Architecture

```
atlas/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs               # CLI entry, raw-mode setup, panic hook
    ├── app.rs                # AppState, View enum (Index | Reader | OutlineModal),
    │                         #   event loop, top-level key dispatch
    ├── config.rs             # Load ~/.config/atlas/library.toml + state.json
    ├── library/
    │   ├── mod.rs
    │   ├── discover.rs       # Walk globs, find candidates
    │   ├── doc.rs            # Doc struct + helpers
    │   ├── parse.rs          # Markdown + frontmatter parser
    │   └── progress.rs       # Checkbox count / frontmatter fallback
    ├── views/
    │   ├── mod.rs
    │   ├── index.rs          # Library index render + filter
    │   ├── reader.rs         # Doc reader render + section scroll
    │   └── outline.rs        # Outline modal (heading tree popup)
    ├── theme.rs              # Editorial palette constants
    ├── edit.rs               # Checkbox toggle + backup file write
    └── ui/
        ├── centered_rect.rs  # Lifted from wt::ui (modal centering)
        └── osc52.rs          # Clipboard for `y` (4 KiB cap, suite convention)
```

### View state machine

```
        ┌──────────┐
        │  Index   │◄─────── (open) ──── startup
        └────┬─────┘
             │ Enter / l / →
             ▼
        ┌──────────┐
        │  Reader  │
        └────┬─────┘
             │ o
             ▼
        ┌────────────────┐
        │ OutlineModal   │  ── Enter (jump) ──► back to Reader at section
        └────────────────┘  ── h / Esc        ─► back to Reader (no jump)
```

The top-level `App` owns `Vec<Doc>` (the parsed library), an active `View` enum, and `state: AppState` (focus indices, scroll offsets, filter text, modal open/closed).

## Data Model

### Library config

`~/.config/atlas/library.toml`:

```toml
roots = [
    "~/projects/*/docs/superpowers/plans/*.md",
    "~/projects/*/ROADMAP.md",
    "~/vaults/sops/plans/*.md",
    "~/vaults/sops/todo*.md",
]
```

`~` is expanded; standard glob patterns supported (`*`, `**`, `?`, `[abc]`).

### Frontmatter gate

Only docs with this YAML frontmatter block are surfaced in the library:

```yaml
---
type: plan          # required: one of {plan, roadmap, task}
title: cal-design   # optional; falls back to file stem
status: active      # optional: {active, shipped, dropped}; defaults to active
progress: 80        # optional; only used if no checkboxes present in doc
---
```

A doc that matches a glob root but lacks frontmatter (or has `type:` set to something other than the three values) is silently skipped.

### Doc struct

```rust
pub struct Doc {
    pub path: PathBuf,
    pub rel_label: String,      // "thelma/sms-bridge" (parent-dir + stem)
    pub doc_type: DocType,      // Plan | Roadmap | Task
    pub title: String,          // from frontmatter or file stem
    pub status: Status,         // Active | Shipped | Dropped (default Active)
    pub progress: u8,           // 0..=100
    pub headings: Vec<Heading>,
    pub tasks: Vec<TaskLine>,
    pub body: String,           // full markdown sans frontmatter
    pub mtime: SystemTime,
}

pub struct Heading {
    pub level: u8,              // 1..=6
    pub text: String,
    pub line: usize,            // 0-indexed line in body
}

pub struct TaskLine {
    pub line: usize,
    pub checked: bool,
    pub text: String,
}
```

### Progress calculation

```rust
fn progress(doc: &ParsedDoc) -> Option<u8> {
    if !doc.tasks.is_empty() {
        let done = doc.tasks.iter().filter(|t| t.checked).count();
        let pct = (done as f32 / doc.tasks.len() as f32 * 100.0).round() as u8;
        Some(pct)
    } else if let Some(p) = doc.frontmatter.progress {
        Some(p)
    } else {
        None  // no bar shown
    }
}
```

`None` renders as a dim em-dash `–` in place of the progress bar.

### Discovery flow

1. Read `library.toml`, expand `~` and globs → candidate paths.
2. For each candidate:
   - Read first 4 KiB.
   - Sniff for leading `---\n` frontmatter block.
   - If present and `type` is one of `plan|roadmap|task`: full-parse the file.
   - Otherwise skip.
3. Build `Vec<Doc>` sorted into the three sections (see Index Surface).

Atlas runs discovery on startup and on the manual `r` reparse. File-watching via `notify` is deferred to v2.

## Surface 1: Index

### Render

Single column, full screen, sections stacked vertically:

```
┌─ atlas · library ────────────────┐
│                                  │
│ ▎ PLANS                          │
│                                  │
│ ▸ cal-design        ▰▰▰▰▱    80% │
│   roam-plan         ▰▰▰▰▰   100% │
│   sms-bridge        ▰▰▱▱▱    40% │
│                                  │
│ ▎ ROADMAPS                       │
│                                  │
│   suite-roadmap     ▰▰▰▰▰    95% │
│   thelma-roadmap    ▰▰▰▱▱    60% │
│                                  │
│ ▎ TASKS                          │
│                                  │
│   weekly-todo       ▰▰▱▱▱    35% │
│                                  │
│ 7 docs indexed                   │
├──────────────────────────────────┤
│ j/k move · enter · / find · q    │
└──────────────────────────────────┘
```

The bar and percentage right edges align on every row. Percentage is right-padded to a fixed 4-char column (`" 80%"`, `"100%"`, `" 40%"`) so the bar's right edge stays put regardless of the number's digit count.

### Section order

Fixed: PLANS → ROADMAPS → TASKS. Within each section, sort by:

1. `status` ascending: `active` first, `shipped` next (grouped+dimmed), `dropped` last (dimmed further).
2. `mtime` descending (most recently touched first).

### Index keymap

| Key | Action |
|---|---|
| `j` / `k` / `↓` / `↑` | Move focus within the active section; wraps across sections |
| `g` / `G` | First / last doc |
| `Enter` / `l` / `→` | Push into reader (lands at last-known scroll for this doc) |
| `h` / `←` | No-op at top level |
| `/` | Enter filter mode (substring match on `title` and `rel_label`); `Esc` cancels, `Enter` commits focus |
| `e` | Exit + print `$EDITOR <path>` (suite's `eval "$(atlas)"` pattern) |
| `y` | Copy doc's absolute path via OSC 52 (4 KiB cap, toast confirms) |
| `r` | Reparse the library |
| `?` | Help modal (full keymap) |
| `q` / `Ctrl-C` | Quit |

## Surface 2: Reader

### Render

```
┌─ atlas · cal-design.md ──────────┐
│ cal: Calendar agenda tile       │
│ plan · 1653 lines · 80% done     │
├──────────────────────────────────┤
│                                  │
│ ▎ Architecture                   │
│                                  │
│     Pure tile. No PrintAndExit.  │
│     Reads cached events from a   │
│     Python REST shim under       │
│     skai-work/scripts/zele/.     │
│                                  │
│ ▎ Task list         8/12 done    │
│                                  │
│     [x] Wire bridge.rs cache     │
│     [x] Event/Attendee structs   │
│   ▸ [ ] Detail modal links       │
│     [ ] c=copy, d=show-done      │
│                                  │
├──────────────────────────────────┤
│ j/k · space · e · o outline · h ←│
└──────────────────────────────────┘
```

- Top header band: title (editorial-magenta italic-bold) and meta line (`type · N lines · X% done`).
- Body: `▎`-prefixed section headings, one blank line above and below each. Prose paragraphs indented 4 cols from the gutter so the left margin breathes.
- Checkboxes:
  - Unchecked: dim `[ ]` + dim text.
  - Checked: green `[x]` + slightly muted text (still readable; signals "done").
  - Focused: subtle bg highlight, `▸` marker prefix.
- Section-progress badge shown to the right of the heading when the section contains checkboxes (e.g. `8/12 done`).
- At wide widths, body is centered with generous side margins. Layout never splits into a second column.

### Section-aware scroll

- `j`/`k` and arrows: scroll one line at a time.
- `J`/`K`: jump to next/prev section heading.
- `g`/`G`: doc start / end.
- `Ctrl-d`/`Ctrl-u`: half-page.
- Scroll position is persisted per-doc in `state.json`; popping back to the index and re-entering returns to the same line.

### Outline modal

`o` opens a centered modal: the doc's heading tree (H1/H2/H3 indented by level). The current section is highlighted; per-section progress is shown when that section contains tasks.

```
        ┌─ outline ───────────────────────┐
        │                                 │
        │   Architecture                  │
        │ ▸ Task list           8/12 done │
        │   Verification                  │
        │   Out of scope                  │
        │                                 │
        │ j/k move · enter jump · esc     │
        └─────────────────────────────────┘
```

- `j`/`k` moves within the modal.
- `Enter` jumps the reader to that heading's line and closes the modal.
- `h` / `Esc` closes without jumping.

### Checkbox toggle (the only mutation)

`space` on a focused `- [ ]`/`- [x]` line flips it.

Sequence:

1. **Backup**: copy current file to `~/.cache/atlas/backups/<basename>.<unix_ts>.md`. Prune to 20 most recent backups per file.
2. **Surgical line rewrite**: read full file, swap exactly `- [ ]` ↔ `- [x]` on the target line, write to a temp file in the same directory, `fs::rename` atomically into place.
3. **Re-derive**: bump `tasks[i].checked`, recompute `progress`. The outline modal's per-section count updates too.
4. **Toast**: `✓ checked` or `↺ unchecked`, 3s.

### Reader keymap

| Key | Action |
|---|---|
| `j`/`k`/`↓`/`↑` | Scroll one line |
| `J`/`K` | Next / prev section heading |
| `g`/`G` | Doc start / end |
| `Ctrl-d`/`Ctrl-u` | Half-page down/up |
| `space` | Toggle focused checkbox (with backup) |
| `o` | Open outline modal |
| `/` | In-doc text search (substring; `n`/`N` next/prev match) |
| `e` | Exit + print `$EDITOR +<line> <path>` |
| `y` | Copy doc path via OSC 52 |
| `Y` | Copy current section text via OSC 52 (4 KiB cap) |
| `h`/`←`/`Backspace`/`Esc` | Pop back to index |
| `?` | Help modal |
| `q`/`Ctrl-C` | Quit atlas |

## Theme (Editorial palette)

Atlas defines its own constants in `theme.rs`. Does NOT consume `~/.config/dashboard-suite/theme.toml`. Theme override file at `~/.config/atlas/theme.toml` is a v2 feature.

```rust
// theme.rs: Editorial palette
pub const BG_BASE:    Color = Color::Rgb(15, 14, 19);    // #0f0e13 near-black
pub const BG_FOCUS:   Color = Color::Rgb(26, 24, 37);    // #1a1825 row highlight
pub const TEXT:       Color = Color::Rgb(212, 208, 224); // #d4d0e0 body
pub const DIM:        Color = Color::Rgb(74, 69, 85);    // #4a4555 secondary
pub const ROSE:       Color = Color::Rgb(201, 126, 140); // #c97e8c section gutter
pub const LAV:        Color = Color::Rgb(139, 126, 184); // #8b7eb8 doc names
pub const TITLE:      Color = Color::Rgb(184, 90, 142);  // #b85a8e doc titles
pub const TITLE_ITAL: Modifier = Modifier::ITALIC | Modifier::BOLD;
pub const CHECK:      Color = Color::Rgb(107, 149, 104); // #6b9568 done
pub const ACCENT:     Color = Color::Rgb(212, 168, 73);  // #d4a849 mustard
```

Less saturated than the suite dashboard palette. Inkier near-black background, dustier rose/lavender, italic mag for titles. Stays dark (good in tmux), distinctly atlas.

## Persistence

`~/.config/atlas/state.json`:

```json
{
  "last_doc": "/home/jane/projects/glance/docs/superpowers/specs/2026-05-26-cal-design.md",
  "scroll": {
    "/home/jane/.../cal-design.md": 142,
    "/home/jane/.../sms-bridge-design.md": 0
  }
}
```

Loaded on startup. `--no-resume` CLI flag bypasses restoring `last_doc` and drops you on the index.

Backups: `~/.cache/atlas/backups/<basename>.<unix_ts>.md`. 20 max per file. Plain copies, no compression.

## OSC 52 clipboard

Lifted from the suite convention: base64-encoded `\x1b]52;c;<payload>\x07`. Hard cap at 4 KiB of raw bytes pre-base64 (Termux pty buffer limit; Blink's clipboard handler times out on more). On truncation, toast says `copied (truncated)`.

## Error handling

- **No `library.toml`**: write a default one with `roots = []` and a comment pointing the user at the spec. Run with empty library.
- **Malformed `library.toml`**: log error to stderr, fall back to empty library, render index with a footer toast `library.toml malformed (see stderr)`.
- **Frontmatter parse error on a single doc**: skip that doc, continue. Footer toast on startup reports count (`3 docs skipped`).
- **File modified externally while atlas open**: on next interaction with that doc, mtime mismatch triggers a reparse (cheap). v2 will add live `notify` watching.
- **Backup directory full / unwritable**: refuse the checkbox toggle; toast `cannot write backup; toggle aborted`.
- **Atomic rename fails (cross-filesystem)**: fall back to copy + delete, with explicit error if either fails.
- **Non-UTF-8 file content**: display via `String::from_utf8_lossy` with replacement chars. No panic.
- **`&str` byte-slicing**: never. All character-level rendering uses `char_indices()`. (See the suite's UTF-8 lesson; non-breaking spaces and emoji crash byte-indexed loops.)

## Testing

- `library/parse.rs`: frontmatter detection, checkbox counting, heading extraction. Table-driven on fixture markdown strings.
- `library/progress.rs`: checkbox-wins-over-frontmatter rule. Cases: zero tasks + frontmatter present, zero tasks + no frontmatter, all-done, all-unchecked, mixed.
- `library/discover.rs`: glob expansion + frontmatter gate. Real-fs test using `tempfile` to build a fixture tree.
- `edit.rs`: checkbox toggle on the exact line, backup file written before mutation, atomic rename pattern. Real-fs test.
- `views/index.rs`: section ordering, sort within section, filter substring matching.
- `views/outline.rs`: heading-tree construction with mixed H1/H2/H3 depth, per-section progress calculation.
- Smoke test at the bin level: render a fixture doc to a `Buffer` at 36/80/120 cols, compare output via `insta` snapshots or hand-written assertions.

Target: ~30 unit tests, fast (no fs in most). Real-fs tests gated to `discover` and `edit`.

## Out of scope (v1)

- `notify`-based file-watching. Manual `r` reparse only.
- Multi-file outline / cross-doc search.
- Theme picker / runtime palette switching.
- Pretty rendering of non-plan markdown features:
  - Tables: render verbatim (don't try to draw a real grid).
  - Code blocks: monospace block with a dim left-border line.
  - Links: rendered as text (`[label](url)`); no hot-link / xdg-open.
- Editing beyond `[ ]` ↔ `[x]` toggle.
- Glance panel form. Standalone-only.
- Theming the suite roadmap doc specifically as a kanban / wave / network visualization. Atlas reads it as a roadmap-type doc, that's all. The old "atlas as roadmap visualizer" brief is superseded.

## Open questions (none blocking)

- Whether `Y` (copy current section) earns its keep. Easy to drop in v1 if review says it crowds the keymap.
- Whether `--no-resume` is needed or if `state.json` should just be opt-in via flag. Default chosen: state.json always writes; `--no-resume` flag suppresses restore on startup.

## Related docs

- `~/projects/dashboard-suite/ROADMAP.md`: the suite roadmap (also a doc atlas will read once frontmatter is added).
- `~/projects/glance/docs/superpowers/specs/2026-05-26-cal-design.md`: `cal` spec (also a doc atlas will read).
- Original brief: `~/projects/dashboard-suite/ROADMAP.md` § "atlas: Suite roadmap visualizer / navigator." That brief is now historical context only.
