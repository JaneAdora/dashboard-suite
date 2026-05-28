---
type: spec
title: mandalas-design
status: approved
date: 2026-05-28
---

# mandalas Design

A standalone Rust/ratatui TUI app that renders animated mandalas in the terminal, purely for visual joy. Six mandala styles with per-style parameters and global motion controls. The user picks a style with `Tab`, watches it breathe, optionally tweaks parameters in a sidebar.

The seventh sibling of the dashboard-widget suite alongside `wt`, `recall`, `roam`, `glance`, `atlas`, `cal`.

## Aesthetic Direction

Mystical/cosmic. Editorial palette already used elsewhere in the suite (rose `#e88b9f`, lavender `#c5a3ff`, mustard `#d9a441`, sage `#9bb59a`, magenta `#ff6ec7`) against a deep cosmic background (`#0a0815`). Stippled Braille-rendered shapes give a luminous "dot-pattern" feel rather than crisp vector lines, which suits the mood.

## Mandala Roster (six)

Each was tuned in the brainstorm playground (`~/projects/dashboard-suite/.superpowers/brainstorm/.../playground-v2.html`) and is fully spec'd by its slider schema + animation behaviour.

1. **Sacred Geometry** — Concentric circles + nested Star-of-David triangles. Rings ripple radially (radius oscillates with sin phase offset by ring index); star pairs counter-rotate at golden-ratio frequencies and breathe scale.
2. **Lotus** — Concentric layers of petals. Petals bloom outward in cyclic waves; layer N is phase-offset by `N * π/2` so outer leads inner. Layers counter-rotate slowly.
3. **Spirograph** — Hypotrochoid curve sampled at `revs * 80` points. Parameters (`R`, `r`, `d`) sinusoidally slow-drift; echo trail draws two ghost copies at `t-0.35` and `t-0.7` at decreasing opacity.
4. **Star Lattice** — Nested N-pointed stars at decreasing scales. Layers counter-rotate at golden-ratio frequencies (`1 + i * 0.382`). Inner-point ratio breathes sinusoidally.
5. **Flower of Life** — Hex-grid of overlapping circles in concentric hex rings. Wave propagates outward from center: each circle's radius and opacity pulse with phase based on distance from origin.
6. **Interlace** — Ring of overlapping circles orbiting the center. Orbital radius breathes; per-ring radius pulses with phase offset; hue chases around the ring count.

## Layout

Two-pane responsive:

- **Wide (≥80 cols):** Header (2 rows: title + meta) / [Canvas square left | Sidebar 30 cols right] / Footer (1 row keymap).
- **Narrow (40–79 cols):** Header / Canvas full-width / Footer. Sidebar hidden by default; `H` toggles.
- **Tiny (<40 cols):** Single-column, canvas takes everything; help via `?` modal.

## Rendering Model

- ratatui `Canvas` widget with `Marker::Braille` (2×4 pixels per character cell).
- Each shape sampled as N points and passed to `ctx.draw(&Points { coords, color })`.
- Lines between two points: parametric sampling at `~r/2` step (Braille resolution-aware), not Bresenham (cheaper to compute, smoother on curves).
- Circles: `2 * π * r / step` sample points; default step 0.6.
- 60 fps target (16.6 ms frame budget). Render budget per frame: 1000–3000 points typical, ~5000 worst case (spirograph echo at high revs).
- Echo trail = past-time path recomputation + half-intensity color mix toward background (lerp toward `#0a0815`).
- Truecolor (24-bit RGB) required for smooth hue cycling. On 256-color terminals the hue shift is rounded to nearest palette entry, producing visible stepping — acceptable.

## Animation Primitives (Rust)

```rust
pub const TAU: f64 = std::f64::consts::TAU;
pub fn osc(t: f64, hz: f64, phase: f64) -> f64 { (t * hz * TAU + phase).sin() }
pub fn osc01(t: f64, hz: f64, phase: f64) -> f64 { 0.5 + 0.5 * osc(t, hz, phase) }
```

These are the SAME primitives the playground used. Per-renderer animation is a function of `(params, t, common)` returning nothing (writes to a `Context` via the ratatui closure).

## Controls

### Per-Mandala Sliders

Same six param sets as the playground. Each slider declares `key`, `label`, `min`, `max`, `step`, `def`, `unit`. Values stored as `f64` for uniformity; renderers cast to `usize` as needed.

| Mandala | Sliders |
|---|---|
| Sacred Geometry | rings (0–8), triangles (0–6), scale (20–90%) |
| Lotus | petals (4–18), layers (1–4), length (25–80), core (4–30) |
| Spirograph | R (30–90), r (4–50), d (5–60), revs (1–30) |
| Star Lattice | points (5–18), nested (1–5), inner (20–70%), phase (0–90°) |
| Flower of Life | layers (1–4), radius (15–50) |
| Interlace | rings (3–16), radius (15–55), orbit (15–60) |

### Common (Motion)

- `speed` — 0–3× multiplier on rotation-style motion (def 1.0)
- `pulse_depth` — 0–100% amplitude of breathing (def 60)
- `pulse_rate` — 0.1–2 Hz frequency of breathing (def 0.55)
- `hue_drift` — 0–60°/sec auto color cycle (def 0)
- `hue` — 0–360° manual shift (def 0)
- `stroke` — 0.5–3 visual weight multiplier (def 1.4); used as a width modifier for line/circle sampling density, not a pixel width

## Keymap

| Key | Action |
|---|---|
| `Tab` / `Shift+Tab` | cycle mandalas |
| `↑` / `↓` | move focus between sliders |
| `←` / `→` | adjust focused slider (one step per press) |
| `Shift+←` / `Shift+→` | adjust focused slider in larger steps (10×) |
| `Space` | pause/resume animation |
| `H` | toggle sidebar visibility |
| `R` | randomize all params for current mandala |
| `s` | save current config as a preset (prompts for slot 1–9) |
| `1`–`9` | load preset |
| `?` | help modal |
| `q` / `Esc` | quit |

Footer advertises the five essentials only: `Tab next · Space pause · ←→ adjust · ? help · q quit`. Full keymap lives in `?`.

## Config

`~/.config/mandalas/config.toml`:

```toml
default_mandala = "sacred"

[motion]
speed = 1.0
pulse_depth = 60
pulse_rate = 0.55
hue_drift = 0
hue = 0
stroke = 1.4

[[preset]]
slot = 1
name = "default"
mandala = "sacred"
params = { rings = 3, triangles = 2, scale = 55 }
motion = { speed = 1.0, pulse_depth = 60 }
```

Missing config is fine — defaults from `controls.rs` apply.

## File Structure

```
mandalas/
├── Cargo.toml
├── README.md
├── install.sh
└── src/
    ├── main.rs                # CLI entry, raw mode, panic hook, event loop
    ├── lib.rs                 # Re-exports so tests can hit internals
    ├── app.rs                 # AppState, key dispatch
    ├── theme.rs               # PALETTE
    ├── color.rs               # HSL shift, palette helpers + tests
    ├── motion.rs              # osc/osc01/TAU + tests
    ├── controls.rs            # Slider, Schema, Common, MANDALA_SCHEMAS, defaults
    ├── config.rs              # Config load/save
    ├── presets.rs             # Preset numbered 1–9
    ├── render/
    │   ├── mod.rs             # Mandala enum, render dispatch
    │   ├── sacred.rs
    │   ├── lotus.rs
    │   ├── spirograph.rs
    │   ├── star.rs
    │   ├── flower.rs
    │   └── interlace.rs
    └── ui/
        ├── mod.rs             # render_frame entry, Layout::vertical
        ├── canvas.rs          # Canvas widget setup, marker, bounds
        ├── sidebar.rs         # Slider list rendering
        ├── header.rs          # Title bar
        └── footer.rs          # Keymap hint
```

## Dependencies

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
anyhow = "1"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
dirs = "5"
rand = "0.8"        # for R (randomize)
```

No `chrono` / `jiff` needed — animation time is monotonic `Instant`.

## Verification

```bash
cd ~/projects/mandalas
cargo test                                              # all unit + integration tests
cargo build --release
./target/release/mandalas                               # smoke launch
COLUMNS=100 LINES=40 ./target/release/mandalas          # wide
COLUMNS=40  LINES=24 ./target/release/mandalas          # narrow
COLUMNS=30  LINES=20 ./target/release/mandalas          # tiny
```

Manual checks:
- [ ] All six mandalas render and animate at default params
- [ ] Sliders update live; canvas reflects on next frame
- [ ] `Space` pauses cleanly; `Tab` cycles; `q` quits without leaving raw-mode garbage
- [ ] Hue drift cycles colors continuously when `>0 °/s`
- [ ] Spirograph echo trail visible at default pulse depth
- [ ] Narrow width auto-hides sidebar; `H` toggles back
- [ ] `R` produces visibly different output for the current mandala
- [ ] `s`-then-`1` saves a preset; `1` reloads it after `R`-shuffling

## Out of Scope (v1)

- Mouse / touch input
- Recording animations to GIF / video export
- Custom mandala plugin system
- Multi-mandala compositions (split-screen, picture-in-picture)
- 256-color palette degradation (assume truecolor; just round-to-nearest happens automatically)
- Persistent "current params" (params reset to schema defaults on launch; only saved presets survive)
- Audio reactivity (someday, but not v1)

## Brainstorm Artifacts

- Playground v2 (final iteration): `~/projects/dashboard-suite/.superpowers/brainstorm/1631167-1779985955/content/playground-v2.html`
- Six animated mandalas with motion sliders and pulse/hue/stroke controls — the design surface this spec is locked against.
