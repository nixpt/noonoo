# Vaji — Architecture

## Principles

1. **Own the integration, not the engines.** A spec-complete CSS cascade, a
   flexbox/grid solver, a text shaper, and a GPU 2D rasterizer are solved
   problems and each is a multi-person-year effort. Vaji *buys* these and owns
   the seams between them. See [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md).
2. **Decompose the monolith.** The engine Vaji replaces put the DOM, style,
   layout, text, events, parsing, and networking in one ~19k-LOC crate that every
   consumer pulled whole. Vaji is a set of *small* crates, each with one reason
   to exist, so a consumer can take only what it needs.
3. **One boundary per borrowed engine.** Each third-party engine is named in
   exactly one crate. Its types do not leak. "What would a swap cost?" is
   answerable by reading one file.
4. **No `unsafe` in the engine.** Every crate is `#![forbid(unsafe_code)]` except
   `vaji-shell`, where the windowing and GPU stacks require it.
5. **Headless is first-class.** Observe (tree dump), drive (synthetic input),
   and make deterministic (virtual clock) with no display.
6. **Fixes over ports.** Where consumers fought the old engine — silent no-ops,
   missing bubbling, unstable ids, a broken `set_inner_html`, un-wired
   accessibility — Vaji changes the design instead of re-exporting the wound.
   See [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md).

## The crates

Thirteen crates in four tiers. Arrows point *downward only*.

```
┌─ facade ──────────────────────────────────────────────────────────────────┐
│  vaji                       umbrella re-export + the `vaji` CLI            │
└───────┬───────────────────────────────────────────────────────────────────┘
        │
┌─ output ────────────────┬──────────────────────────┬───────────────────────┐
│  vaji-shell             │  vaji-paint              │  vaji-headless        │
│  window, event loop,    │  layout → render         │  tree dump, virtual   │
│  lifecycle, a11y, gpu   │  commands, scene effects │  clock, synthetic in  │
└───────┬─────────────────┴────────────┬─────────────┴───────────┬───────────┘
        │                              │                         │
┌─ orchestration ──────────────────────▼─────────────────────────▼───────────┐
│  vaji-engine     the working document: root + tree + resolve caches         │
└───┬──────────┬───────────┬──────────┬───────────┬──────────┬───────────────┘
    │          │           │          │           │          │
┌───▼────┐ ┌───▼────┐ ┌────▼─────┐ ┌──▼──────┐ ┌──▼────┐ ┌───▼────┐
│ vaji-  │ │ vaji-  │ │ vaji-    │ │ vaji-   │ │ vaji- │ │ vaji-  │
│ style  │ │ layout │ │ text     │ │ html    │ │ net   │ │ events │
│ stylo  │ │ taffy  │ │ parley   │ │ html5   │ │ http  │ │ bubble │
└───┬────┘ └───┬────┘ └──────────┘ └──┬──────┘ └───────┘ └───┬────┘
    │          │                       │                      │
    └──────────┴───────────┬───────────┴──────────────────────┘
                           │
                    ┌──────▼──────┐      ┌────────────┐
                    │  vaji-dom   │      │ vaji-motion│
                    │  tree, ids, │      │ springs    │
                    │  mutate,    │      └────────────┘
                    │  queries    │
                    └─────────────┘
```

| Crate | Owns | Borrowed engine (the only place it is named) |
|---|---|---|
| `vaji-dom` | tree, `NodeId`, checked mutation, queries | — |
| `vaji-events` | input model, dispatch + **bubbling** | — |
| `vaji-text` | shaping, fonts, line breaking | `parley` (+ fontique/swash/skrifa) |
| `vaji-motion` | springs, velocity-preserving retarget | — (owned) |
| `vaji-style` | stylesheet ingest, cascade, incremental restyle | `stylo` (+ selectors/cssparser) |
| `vaji-layout` | boxes and geometry | `taffy` |
| `vaji-html` | markup → tree | an HTML parser |
| `vaji-net` | loading: http/file/data/exo | `reqwest` (feature-gated) |
| `vaji-engine` | the working document (parse→style→layout) | — (orchestrates) |
| `vaji-headless` | tree dump, virtual clock, synthetic input | — |
| `vaji-paint` | render command stream, scene effects, overlays | `anyrender`/`vello` (feature-gated) |
| `vaji-shell` | window, event loop, a11y, GPU surface | `winit`, `wgpu`, `accesskit` |
| `vaji` | umbrella + CLI | — |

## Data flow

```
 bytes ─▶ vaji-html ─▶ vaji-dom ─▶ vaji-style ─▶ vaji-layout ─▶ vaji-paint ─▶ backend
                       (tree)      (stylo)       (taffy+vaji-text)  (commands)
                                   ▲                                     │
                        vaji-engine orchestrates all of the above      │
                                                                          ▼
 OS input ─▶ vaji-shell ─▶ vaji-events ─▶ vaji-engine ─────────────▶ frame
```

Each arrow is a small, typed value crossing a crate boundary. Nothing downstream
of `vaji-dom` knows where a node came from; nothing outside `vaji-style` knows
`stylo` exists.

## Invariants to keep true

- **Identity is defined once.** `vaji_dom::NodeId` is the only node handle.
- **Borrowed engines stay behind their crate.** `grep -r stylo crates` lands in
  `vaji-style` alone; `taffy` in `vaji-layout`; `parley` in `vaji-text`.
- **`forbid(unsafe_code)` stays on every crate except `vaji-shell`.**
- **The default build stays dependency-light.** New backend work lands behind a
  feature; it does not become a default.
- **Every crate has one reason to exist.** If two crates cannot be described
  without the other's nouns, they are one crate.

## Clean-room discipline

- No file is copied from `bliss-engine` or Blitz. Licenses aside (standard
  MIT/Apache-2.0 boilerplate), everything is authored here.
- Design is taken from *behaviour*: consumer tickets, the specs, and what a
  browser does — not from another implementation's source.
- Where a consumer's need is real but the old engine's shape was wrong, Vaji
  records the need and redesigns the shape (see
  [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md)).
