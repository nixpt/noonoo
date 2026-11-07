# Noonoo — Dependency Exit Analysis

*Can we drop the borrowed engines — starting with stylo — and what would it take?*

This is a data-first answer. It measures what each dependency costs, separates
what is *feasible to own* from what is *not*, and lays out a phased path.

## 0. Method

`cargo tree -p <engine> -e normal --prefix none | sort -u | wc -l` on
`bliss-engine` (the engine Noonoo replaces), counting unique normal-transitive
crates per borrowed engine. Code-size figures are ballparks from the format
owners' own specs, not measured here.

## 1. The scoreboard

| Borrowed engine | Unique crates | Role | Can we own it? | Replacement size |
|---|---:|---|---|---|
| **stylo** | **108** | CSS: selectors, cascade, computed values | **Yes — a documented subset** | ~3–6k LOC for a UI-grade subset |
| vello | 82 | GPU 2D renderer | No | 100k+ LOC |
| winit | 82 | windowing / event loop | No | platform backends, 100k+ LOC |
| anyrender_svg | 71 | SVG render | Partly (subset) | large |
| reqwest | 66 | HTTP + TLS | Reduce (thin client) | TLS is the hard part |
| wgpu | 57 | GPU API | No | it *is* the GPU API |
| **parley** | **55** | text shaping + fonts | Partly (own line-breaking, keep shaping) | shaping is 100k+ LOC |
| usvg | 51 | SVG parse | Partly (subset) | moderate |
| image | 26 | image codecs | No (keep) | codecs are a swamp |
| **taffy** | **3** | flexbox + grid | **Yes** | ~3–5k LOC |
| accesskit | 3 | a11y protocol | Keep bridge, own the tree | — |

Reference: `bliss-dom` alone pulls **189** unique crates; the full workspace lock
is **634** (all backends/features).

**Two engines dominate: stylo (108) and, at the code level, parley.** One is
worth owning as a subset; the other is not.

## 2. Verdict per dependency

### stylo — **OWN (a subset)** ✅ the big lever
What it provides: stylesheet parsing, selector matching + specificity, the
cascade, inheritance, and `ComputedValues` for every CSS property.
What dropping it removes: **108 crates**, the `style::`-coupling (161 touchpoints
in the old engine), an MSRV/upgrade treadmill, and a `fontconfig`-at-build-time
dependency.

Feasible **only** as a subset. A spec-complete CSS engine is a multi-year effort;
a **UI-grade** one — the properties a widget toolkit actually uses — is a few
thousand lines. The honest framing: we are not replacing Servo's CSS engine, we
are **replacing it with a smaller, documented CSS dialect** and accepting the
coverage limit.

The subset (v1) to target:

- **Selectors:** type, `.class`, `#id`, `*`, attribute (`[a]`, `[a=v]`),
  descendant / child / adjacent / general-sibling combinators, `:first-child` /
  `:last-child` / `:nth-child` / `:not()` / `:is()`, `:hover` / `:focus` / `:active`.
- **Cascade:** origin (UA < user < author < inline), specificity, source order,
  `!important`, inheritance, `initial`/`inherit`/`unset`.
- **Properties:** `display`, `position` (`static`/`relative`/`absolute`/`fixed`),
  `width`/`height`/`min-`/`max-`, `margin`/`padding`/`border`, `box-sizing`,
  `flex-*`, `gap`, `align-items`/`justify-content`, `color`, `background-*`,
  `font-*` (incl. line-height), `text-align` / `text-decoration`,
  `overflow`, `opacity`, `border-radius`, `z-index`, `visibility`, `cursor`.
- **Non-goals (v1):** `@supports`, animations/keyframes, grid *containers'* full
  feature set (tracks, placement, and `fr` are owned in `noonoo-layout`; older
  grid sugar like named areas is not), media queries (parsed, ignored), SVG CSS.
  Custom properties landed since (`var()`, inheritance, cycles, fallbacks).

Already built: the **tokenizer + rule splitter** (`noonoo-css`). Remaining:
selector model + specificity, a property/value model, the cascade + inheritance,
computed values, and a default UA stylesheet.

### taffy — **OWN** ✅ (done: block, inline, flex, grid, floats)
Only 3 crates (taffy is nearly standalone), but the code to replace is real:
block + inline + flexbox first, grid later. A focused box-layout engine is
~3–5k LOC. Owning it also lets layout consume our CSS values directly (no
`stylo_taffy` bridge). *All of it landed — including `fr` tracks, placement,
auto-placement, floats with formatting contexts, and alignment.*

### parley (+ fontique/swash/skrifa) — **PARTIAL** ⚠️ keep shaping
Shaping (OpenType features, complex scripts, BiDi) is a 100k+-LOC swamp and a
genuine one. **Do not own shaping.** What we *do* own: simple line-breaking and
`text-align` (`noonoo-text`), the box/text integration, and — since — font
**file parsing to metrics** (`noonoo-ttf`: `head`/`maxp`/`hhea`/`hmtx`/`cmap`),
which feeds real advance widths and line metrics in behind the `Shaper` seam.
What stays delegated is GSUB/GPOS and font *management* (family resolution,
system enumeration) — `fontique`'s job, not ours.

### vello / wgpu / winit — **KEEP** ❌ (not a dependency to drop)
GPU 2D, the GPU API, and platform windowing are the "buy" line. What we own is
the **paint command stream** (`noonoo-paint`, done) and, optionally, a tiny CPU
rasterizer for headless. Dropping wgpu/vello/winit is not a project, it *is* the
project.

### reqwest — **REDUCE** ➖
`reqwest` drags 66 crates for what is, in this engine, "GET some bytes". A
webpage loader needs HTTP/1.1 + TLS; TLS is the only hard part. Options:
(a) keep reqwest; (b) a thin client over `rustls` + `hyper`, or over a minimal
hand-rolled HTTP/1.1 with `rustls` — drops the reqwest/middleware/cache stack.
Defer; low priority vs. CSS.

### usvg / anyrender_svg — **REDUCE (later)** ➖
own an SVG *subset* (shapes, paths, fills, transforms) or drop SVG from the
default build entirely.

### image — **KEEP** ❌
Codec support is a swamp with a security surface; a curated dependency is right.

### accesskit — **KEEP the bridge, own the tree** ➖
The protocol + platform bridges (AT-SPI/UiKit/WinRT) are not ours to write; the
a11y *tree* built from our DOM is (and the old engine's Linux path was a no-op
anyway). Shipped: `noonoo-a11y` owns the tree (roles, names, states, bounds) and
`integrations/noonoo-accesskit` projects it onto AccessKit. The Linux window
pass (`integrations/noonoo-winit`) creates the AT-SPI adapter and forwards
focus/click actions; platform delivery stays opt-in with the window backend.

## 3. The buy-line (do not build)

> **Own** the integration-critical, moderately-sized layers: the DOM, **CSS
> (subset)**, **layout**, text *line-breaking*, the paint command stream, events,
> headless tooling, the parsers, and the **accessibility tree**.
> **Buy** the genuinely enormous engines: **font shaping**, **GPU
> rasterization**, **platform windowing**, **TLS**, and **image codecs**.

That line is where "reduced dependencies" stops being a liability and becomes
the point: the owned layers are where the engine's identity and compatibility
live; the bought ones are solved problems nobody wins by re-deriving.

## 4. Phased plan

| Phase | Removes | Deliverable | Done when | Status |
|-------|---------|-------------|-----------|--------|
| **A. CSS engine** | **stylo (108)**, cssparser, selectors | `noonoo-css` grows selectors + specificity + a property/value model; `noonoo-style` owns the cascade, inheritance, computed values, UA stylesheet | a fixture's computed `color`/`display`/`margin` are asserted from our engine, with **stylo absent from `Cargo.lock`** | ✅ |
| B. Layout | taffy (3) | `noonoo-layout`: block + inline + flex, consuming `noonoo-style` values | a fixture yields expected box rects | ✅ (grid + floats too) |
| C. Text | part of parley | `noonoo-text`: line-breaking + text-align, shaping delegated | wrapped text lays out to expected lines | ✅ (+ `noonoo-ttf` metrics behind the seam) |
| D. Paint | (none) | `noonoo-paint` command stream + a recording backend; keep vello/wgpu | a fixture's commands match a golden file | ✅ (JSON is the recording backend; GPU + window passes are opt-in) |
| E. Net / SVG | reqwest (66) / usvg (51) | thin HTTP client over rustls; SVG subset | optional | ✅ (`file:`/`data:` owned, `http` behind a feature; SVG subset owned) |

Phase A was the headline and the biggest single win; all five phases are done.

## 5. Target after Phase A

Dropping stylo takes `bliss-dom` from ~189 crates to **~81** (upper bound; some
are shared), and removes the heaviest compile-time and MSRV pressure. The Noonoo
engine crates after A + B + C would be a DOM + CSS + layout + text stack whose
only heavy dependencies are the ones we deliberately keep (a rasterizer, a
shaper, windowing) — each behind a feature.

## 6. Risks

- **Coverage.** A CSS subset will not render every real web page (matters for a
  *browser*, surfer). Fine for a *UI toolkit* (arniko). Mitigation: keep the
  engine's CSS behind `noonoo-style`, and leave the door open to a
  `style-stylo` feature for a future full-fidelity profile.
- **Correctness.** We inherit the burden Servo carried. Mitigation: a golden
  conformance suite for the *documented subset* only.
- **Two paths.** Supporting both our engine and stylo doubles maintenance —
  decide per consumer, don't support both by default.
