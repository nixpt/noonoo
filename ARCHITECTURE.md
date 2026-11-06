# Noonoo — Architecture

## Principles

1. **Own the integration, not the engines.** A spec-complete CSS cascade, a
   flexbox/grid solver, a text shaper, and a GPU 2D rasterizer are solved
   problems and each is a multi-person-year effort. Noonoo *buys* these and owns
   the seams between them. See [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md).
2. **Decompose the monolith.** The engine Noonoo replaces put the DOM, style,
   layout, text, events, parsing, and networking in one ~19k-LOC crate that every
   consumer pulled whole. Noonoo is a set of *small* crates, each with one reason
   to exist, so a consumer can take only what it needs.
3. **One boundary per borrowed engine.** Each third-party engine is named in
   exactly one crate. Its types do not leak. "What would a swap cost?" is
   answerable by reading one file.
4. **No `unsafe` in the engine.** Every crate is `#![forbid(unsafe_code)]` except
   `noonoo-shell`, where the windowing and GPU stacks require it.
5. **Headless is first-class.** Observe (tree dump), drive (synthetic input),
   and make deterministic (virtual clock) with no display.
6. **Fixes over ports.** Where consumers fought the old engine — silent no-ops,
   missing bubbling, unstable ids, a broken `set_inner_html`, un-wired
   accessibility — Noonoo changes the design instead of re-exporting the wound.
   See [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md).

## The crates

Seventeen crates in five tiers. Arrows point *downward only*.

```
┌─ facade ──────────────────────────────────────────────────────────────────┐
│  noonoo                       umbrella re-export + the `noonoo` CLI            │
└───────┬───────────────────────────────────────────────────────────────────┘
        │
┌─ output ────────────────┬──────────────────────────┬───────────────────────┐
│  noonoo-shell             │  noonoo-paint              │  noonoo-headless        │
│  window, event loop,    │  layout → render         │  tree dump, virtual   │
│  lifecycle, a11y, gpu   │  commands, scene effects │  clock, synthetic in  │
└───────┬─────────────────┴────────────┬─────────────┴───────────┬───────────┘
        │                              │                         │
┌─ orchestration ──────────────────────▼─────────────────────────▼───────────┐
│  noonoo-engine     the working document: root + tree + resolve caches         │
└───┬──────────┬───────────┬──────────┬───────────┬──────────┬───────────────┘
    │          │           │          │           │          │
┌───▼────┐ ┌───▼────┐ ┌────▼─────┐ ┌──▼──────┐ ┌──▼────┐ ┌───▼────┐
│ noonoo-  │ │ noonoo-  │ │ noonoo-    │ │ noonoo-   │ │ noonoo- │ │ noonoo-  │
│ style  │ │ layout │ │ text     │ │ html    │ │ net   │ │ events │
│ stylo  │ │ taffy  │ │ parley   │ │ html5   │ │ http  │ │ bubble │
└───┬────┘ └───┬────┘ └──────────┘ └──┬──────┘ └───────┘ └───┬────┘
    │          │                       │                      │
    └──────────┴───────────┬───────────┴──────────────────────┘
                           │
                    ┌──────▼──────┐      ┌────────────┐
                    │  noonoo-dom   │      │ noonoo-motion│
                    │  tree, ids, │      │ springs    │
                    │  mutate,    │      └────────────┘
                    │  queries    │
                    └─────────────┘
```

| Crate | Owns | Borrowed engine (the only place it is named) |
|---|---|---|
| `noonoo-parse` | shared parsing primitives: `Span`, `ParseError`, `Cursor` | — |
| `noonoo-json` | JSON parse + value model + writer | — (owned) |
| `noonoo-css` | CSS **syntax**: tokenizer + rule splitter | — (cascade still borrowed) |
| `noonoo-caison` | CAISON parse + JSON projection | — (owned) |
| `noonoo-html` | HTML tokenizer + tree builder → `noonoo-dom` | — (owned) |
| `noonoo-dom` | tree, `NodeId`, checked mutation, queries | — |
| `noonoo-events` | input model, dispatch + **bubbling** | — |
| `noonoo-text` | shaping, fonts, line breaking | `parley` (+ fontique/swash/skrifa) |
| `noonoo-motion` | springs, velocity-preserving retarget | — (owned) |
| `noonoo-style` | style ingest, cascade, incremental restyle | `stylo` (+ selectors/cssparser) |
| `noonoo-layout` | boxes and geometry | `taffy` |
| `noonoo-net` | loading: http/file/data/exo | `reqwest` (feature-gated) |
| `noonoo-engine` | the working document (parse→style→layout) | — (orchestrates) |
| `noonoo-headless` | tree dump, virtual clock, synthetic input | — |
| `noonoo-paint` | render command stream, scene effects, overlays | `anyrender`/`vello` (feature-gated) |
| `noonoo-shell` | window, event loop, a11y, GPU surface | `winit`, `wgpu`, `accesskit` |
| `noonoo` | umbrella + CLI | — |

**Parsers are owned, not borrowed.** The four format crates (`noonoo-json`,
`noonoo-css`, `noonoo-caison`, `noonoo-html`) are dependency-free and exist because a
parser is a *syntax* layer — small, spec-defined, and load-bearing for the
engine's own formats. What stays borrowed is the part that is genuinely huge:
`noonoo-style` drives `stylo` for selector matching and the cascade (it consumes
`noonoo-css` tokens only as far as its own syntax needs).

## Data flow

```
 bytes ─▶ noonoo-html ─▶ noonoo-dom ─▶ noonoo-style ─▶ noonoo-layout ─▶ noonoo-paint ─▶ backend
                       (tree)      (stylo)       (taffy+noonoo-text)  (commands)
                                   ▲                                     │
                        noonoo-engine orchestrates all of the above      │
                                                                          ▼
 OS input ─▶ noonoo-shell ─▶ noonoo-events ─▶ noonoo-engine ─────────────▶ frame
```

Each arrow is a small, typed value crossing a crate boundary. Nothing downstream
of `noonoo-dom` knows where a node came from; nothing outside `noonoo-style` knows
`stylo` exists.

## Invariants to keep true

- **Identity is defined once.** `noonoo_dom::NodeId` is the only node handle.
- **Borrowed engines stay behind their crate.** `grep -r stylo crates` lands in
  `noonoo-style` alone; `taffy` in `noonoo-layout`; `parley` in `noonoo-text`.
- **`forbid(unsafe_code)` stays on every crate except `noonoo-shell`.**
- **The default build stays dependency-light.** New backend work lands behind a
  feature; it does not become a default.
- **Every crate has one reason to exist.** If two crates cannot be described
  without the other's nouns, they are one crate.

## Clean-room discipline

- No file is copied from `bliss-engine` or Blitz. Licenses aside (standard
  MIT/Apache-2.0 boilerplate), everything is authored here.
- Design is taken from *behaviour*: consumer tickets, the specs, and what a
  browser does — not from another implementation's source.
- Where a consumer's need is real but the old engine's shape was wrong, Noonoo
  records the need and redesigns the shape (see
  [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md)).
