# Noonoo

**A clean-room, modular HTML/CSS rendering engine.**

*Noonoo* — a deliberately unmistakable name: short, memorable, and free on
every registry this project publishes to.

Noonoo turns markup and stylesheets into laid-out, paintable form. It is a
from-scratch reimplementation of the ideas in `bliss-engine` (itself a fork of
upstream Blitz), decomposed into small crates and built to be the **substrate**
that a UI framework (arniko) and a browser (surfer) build on.

> **Clean room.** No code here is copied from `bliss-engine`, Blitz, or any other
> rendering engine. Noonoo is written from behaviour and specification. The
> third-party engines it depends on (Servo's *stylo*, *taffy*, *parley*,
> *anyrender/vello*, *winit*) are ordinary crates.io dependencies, pinned in one
> place and reached through one crate each — never vendored, never forked.

## Crates

The engine replaces one ~19k-LOC `bliss-dom` monolith with small, single-purpose
crates. Depend on the one you need; reach for `noonoo` only when you want all of it.

| Crate | Responsibility |
| --- | --- |
| [`noonoo-parse`](crates/noonoo-parse) | Shared parsing primitives: spans, positions, errors, cursor |
| [`noonoo-json`](crates/noonoo-json) | JSON parser, value model, and writer |
| [`noonoo-css`](crates/noonoo-css) | CSS: tokenizer, rule splitter, value/property model (incl. grouping), shorthand expansion |
| [`noonoo-selectors`](crates/noonoo-selectors) | CSS selectors: parse, specificity, matching |
| [`noonoo-caison`](crates/noonoo-caison) | CAISON parse and its JSON projection |
| [`noonoo-html`](crates/noonoo-html) | HTML tokenizer and tree builder into `noonoo-dom` |
| [`noonoo-dom`](crates/noonoo-dom) | The tree, stable `NodeId`s, checked mutation, queries |
| [`noonoo-geometry`](crates/noonoo-geometry) | Points, sizes, and rectangles |
| [`noonoo-events`](crates/noonoo-events) | Input model and DOM dispatch with bubbling |
| [`noonoo-text`](crates/noonoo-text) | Line breaking + alignment (shaping behind a seam) |
| [`noonoo-motion`](crates/noonoo-motion) | Springs, velocity-preserving retarget |
| [`noonoo-style`](crates/noonoo-style) | Style ingestion and the cascade, including custom properties, `var()` and `calc()` |
| [`noonoo-layout`](crates/noonoo-layout) | Box layout: computed style → boxes (block / inline / flex / positioned) + overflow & scroll containers |
| [`noonoo-markdown`](crates/noonoo-markdown) | Markdown (subset) → HTML |
| [`noonoo-csv`](crates/noonoo-csv) | CSV/TSV → HTML table |
| [`noonoo-ini`](crates/noonoo-ini) | INI → value model (JSON) |
| [`noonoo-toml`](crates/noonoo-toml) | TOML (subset) → value model (JSON) |
| [`noonoo-xml`](crates/noonoo-xml) | XML → DOM (strict nesting, entities, CDATA) |
| [`noonoo-ttf`](crates/noonoo-ttf) | TrueType/OpenType → metrics, `cmap`, outlines, kerning (a `Shaper` + `Glyphs`) |
| [`noonoo-svg`](crates/noonoo-svg) | SVG subset → paint commands |
| [`noonoo-hx`](crates/noonoo-hx) | htmx-like declarative attributes → fetch + DOM swap |
| [`noonoo-caps`](crates/noonoo-caps) | Capability gating: scopes, the policy seam, a gated DOM |
| [`noonoo-net`](crates/noonoo-net) | Loading: `file:` + `data:` owned; `http` behind a feature |
| [`noonoo-engine`](crates/noonoo-engine) | Orchestration: a working document |
| [`noonoo-headless`](crates/noonoo-headless) | Tree dump, virtual clock, synthetic input |
| [`noonoo-paint`](crates/noonoo-paint) | Layout → render commands (fills, text runs or glyph outlines, clip) |
| [`noonoo-shell`](crates/noonoo-shell) | Multi-document host: layers, input routing, focus, composition |
| [`noonoo`](crates/noonoo) | Umbrella re-export + the `noonoo` command line |

## Why it exists

Two consumers — **arniko** (a reactive UI framework) and **surfer-browser** —
build on the engine Noonoo replaces, and both carry workarounds for it. Noonoo is
built to be what they'd rather build on: a decomposed substrate with correct
events, stable node ids, checked mutation, a working `set_inner_html`, real
accessibility, and first-class sub-documents, headless tooling, and
capability-gated DOM. See
[`docs/CONSUMERS.md`](docs/CONSUMERS.md) and
[`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md).

## Status

**Pre-alpha, scaffolding.** The workspace compiles with **zero third-party
dependencies**; the engine backends are declared as features and wired in later
slices. See [`docs/ROADMAP.md`](docs/ROADMAP.md) and
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## Building

```sh
cargo build
cargo test --workspace
cargo run -p noonoo -- --version
```

## Try it

The parser crates are wired into the engine, so the CLI works today. First, an
end-to-end walk-through of the stack (htmx-like swap + multi-document host):

```sh
cargo run -p noonoo --example demo
```

Then the CLI:

```sh
noonoo --tree page.html     # parse HTML (own tokenizer), print the DOM as JSON
noonoo --tree -             # …or read from stdin
noonoo --paint page.html    # parse → style → layout → paint, print commands as JSON
noonoo --paint page.html --font DejaVuSans.ttf   # …drawing text as glyph outlines
noonoo --md notes.md        # Markdown → HTML
noonoo --md-paint notes.md  # Markdown viewer: layout + paint commands
noonoo --csv data.csv       # CSV/TSV → HTML table
noonoo --csv-paint data.csv # CSV/TSV viewer: layout + paint commands
noonoo --txt note.txt       # plain text → HTML (<pre>)
noonoo --ini app.ini        # INI → JSON
noonoo --toml app.toml      # TOML → JSON
noonoo --xml feed.xml       # XML → DOM tree as JSON
noonoo --svg icon.svg       # SVG subset → paint commands
noonoo --caison app.caison  # parse CAISON, print its JSON projection
```

The workspace MSRV is **1.89**. CI checks it explicitly.

## Docs

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate map, data flow, invariants
- [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md) — the dependency policy
- [`docs/CONSUMERS.md`](docs/CONSUMERS.md) — arniko & surfer: what they use, what they fight
- [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md) — what belongs in Noonoo vs. the consumer
- [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) — the drop-in switch and the api-drift gate
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — slices
- [`docs/FORMATS.md`](docs/FORMATS.md) — file formats: DOM vs. value model, and Markdown
- [`docs/FONTS.md`](docs/FONTS.md) — the font layer: what Noonoo owns, the seams, what is borrowed
- [`docs/BACKEND.md`](docs/BACKEND.md) — GPU & window backends: the opt-in Vello pass and the planned window pass
- [`docs/XIP.md`](docs/XIP.md) — capability-gated DOM: the owned seam and the opt-in XIP adapter

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
