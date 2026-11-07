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
│ owned  │ │ taffy  │ │ parley   │ │ html5   │ │ http  │ │ bubble │
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
| `noonoo-css` | CSS: tokenizer, rule splitter, value & property model, shorthand expansion | — (owned) |
| `noonoo-selectors` | CSS selectors: parse, specificity, match | — (owned) |
| `noonoo-caison` | CAISON parse + JSON projection | — (owned) |
| `noonoo-html` | HTML tokenizer + tree builder → `noonoo-dom` | — (owned) |
| `noonoo-dom` | tree, `NodeId`, checked mutation, queries | — |
| `noonoo-events` | input model, dispatch + **bubbling** | — |
| `noonoo-text` | line breaking + alignment (shaping behind `Shaper`) | `parley` (via the seam, not yet wired) |
| `noonoo-motion` | springs, velocity-preserving retarget | — (owned) |
| `noonoo-style` | the cascade: matching → inheritance → computed values | — (owned; replaces `stylo`) |
| `noonoo-layout` | box layout: block, inline, flex, positioning | — (owned; replaces `taffy`) |
| `noonoo-geometry` | `Point` / `Size` / `Rect` | — |
| `noonoo-markdown` | Markdown (subset) → HTML | — (owned) |
| `noonoo-csv` | CSV/TSV → HTML table | — (owned) |
| `noonoo-ini` | INI → value model (JSON) | — (owned) |
| `noonoo-toml` | TOML (subset) → value model (JSON) | — (owned) |
| `noonoo-xml` | XML → DOM (strict nesting, entities, CDATA) | — (owned) |
| `noonoo-svg` | SVG subset → paint commands | — (owned) |
| `noonoo-hx` | htmx-like `hx-*` attributes → fetch + DOM swap | — (owned) |
| `noonoo-caps` | capability gating: scopes, policy seam, gated DOM | — (owned); XIP via the opt-in `noonoo-xip` |
| `noonoo-net` | loading: `file:` + `data:` owned; `http` behind a feature | TLS (borrowed, not wired) |
| `noonoo-engine` | the working document (parse→style→layout) | — (orchestrates) |
| `noonoo-headless` | tree dump, virtual clock, synthetic input | — |
| `noonoo-paint` | render command stream, scene effects, overlays | `anyrender`/`vello` (feature-gated) |
| `noonoo-shell` | multi-document host: layers, routing, focus, composition | window/GPU (borrowed, feature-gated) |
| `noonoo` | umbrella + CLI | — |

**Parsers and the style engine are owned, not borrowed.** The four format crates
(`noonoo-json`, `noonoo-css`, `noonoo-caison`, `noonoo-html`) are dependency-free,
and `noonoo-style` runs the cascade itself (`docs/DEPENDENCY-EXIT.md` Phase A) —
`stylo` is not a dependency. What stays borrowed is only the genuinely huge:
`noonoo-layout` (`taffy`), `noonoo-text` (`parley`), and the GPU/windowing stack.

## Data flow

```
 bytes ─▶ noonoo-html ─▶ noonoo-dom ─▶ noonoo-style ─▶ noonoo-layout ─▶ noonoo-paint ─▶ backend
                       (tree)      (owned cascade)  (noonoo + text)      (commands)
                                   ▲                                     │
                        noonoo-engine orchestrates all of the above      │
                                                                          ▼
 OS input ─▶ noonoo-shell ─▶ noonoo-events ─▶ noonoo-engine ─────────────▶ frame
```

Each arrow is a small, typed value crossing a crate boundary. Nothing downstream
of `noonoo-dom` knows where a node came from; nothing outside `noonoo-css` knows
how a selector is spelled.

## Invariants to keep true

- **Identity is defined once.** `noonoo_dom::NodeId` is the only node handle.
- **Borrowed engines stay behind their crate.** `grep -r parley crates` lands in
  `noonoo-layout` alone; `parley` in `noonoo-text`. `stylo` appears nowhere —
  the cascade is ours.
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
