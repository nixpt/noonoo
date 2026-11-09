# Noonoo

**A modular HTML/CSS rendering engine in small, owned Rust crates.**

*Noonoo* — a deliberately unmistakable name: short, memorable, and free on
every registry this project publishes to.

Noonoo turns markup and stylesheets into laid-out, paintable, accessible form:
parse → cascade → layout → paint, with an accessibility tree alongside. Every
stage is its own crate behind a narrow seam, so a host takes only what it
needs — a DOM and selectors, the cascade, layout, or the whole engine — and
embeds it in a UI framework, a document viewer, or a browser.

The engine itself has **no third-party dependencies**: HTML, CSS, selectors,
layout, text, fonts and images are all owned code. What is borrowed is
borrowed deliberately (GPU rasterization, windowing, the platform
accessibility bridge): ordinary crates.io dependencies, each reached through
one opt-in integration crate and kept out of the default build.

## Features

- **HTML.** An owned tokenizer and tree builder: implied `html`/`head`/`body`,
  implied end tags and table sections, character references, quirks-mode
  detection from the doctype, and `set_inner_html`.
- **DOM.** Stable `NodeId`s, checked mutation, `querySelector`-style queries
  with full selectors, and event dispatch with bubbling.
- **CSS cascade.** Selectors with specificity, `@media` (sizes, ranges,
  orientation, resolution, interaction and preference features),
  `@font-face`, custom properties and `var()`, `calc()`/`min()`/`max()`/
  `clamp()`, viewport units, gradients, `:hover`/`:focus`/`:active`, and
  HTML's presentational attributes (`width`, `height`, `align`, `border`,
  `cellpadding`, `cellspacing`).
- **Layout.** Block flow with margin collapsing; inline formatting with
  baselines and `vertical-align`; floats; flexbox (including the flexible
  length freeze loop and auto margins); grid; tables (auto layout,
  `colspan`, `col`/`colgroup` widths, percentage columns,
  `border-spacing` and `border-collapse`); positioned boxes including
  `sticky`; overflow and scroll containers; quirks-mode behaviour that
  legacy pages rely on.
- **Replaced content and controls.** Images (PNG, JPEG, GIF's first frame,
  WebP) sized from their attributes before they load; text fields sized by
  `size`, textareas by `cols`/`rows`, in the browser control font.
- **Text and fonts.** Line breaking and alignment over a `Shaper` seam;
  TrueType/OpenType parsing (metrics, `cmap`, kerning, outlines) and
  per-element face selection from a `FontSet`.
- **Paint.** Layout to a display list: fills, borders, gradients, text runs or
  glyph outlines, images, clips, opacity and 2D transforms
  (`translate`/`scale`/`rotate`).
- **Accessibility.** An owned accessibility tree (roles, names,
  descriptions, states, heading levels, bounds) on by default, with an
  opt-in AccessKit bridge that activates AT-SPI on Linux.
- **Hosting.** A multi-document shell (layers, input routing, focus,
  composition), headless tooling (tree dumps, a virtual clock, synthetic
  input), asynchronous resource loading, htmx-like declarative swaps, and a
  capability-gated DOM.
- **More than HTML.** Markdown, CSV/TSV, plain text, XML, SVG (subset), JSON,
  INI, TOML and CAISON, each into the DOM or a value model.

## Crates

Small, single-purpose crates. Depend on the one you need; reach for `noonoo`
only when you want all of it.

| Crate | Responsibility |
| --- | --- |
| [`noonoo-parse`](crates/noonoo-parse) | Shared parsing primitives: spans, positions, errors, cursor |
| [`noonoo-json`](crates/noonoo-json) | JSON parser, value model, and writer |
| [`noonoo-css`](crates/noonoo-css) | CSS: tokenizer, rule splitter, value/property model (incl. grouping), shorthand expansion, `@font-face` descriptors |
| [`noonoo-selectors`](crates/noonoo-selectors) | CSS selectors: parse, specificity, matching |
| [`noonoo-query`](crates/noonoo-query) | Full-selector and element-scoped DOM queries |
| [`noonoo-caison`](crates/noonoo-caison) | CAISON parse and its JSON projection |
| [`noonoo-html`](crates/noonoo-html) | HTML tokenizer and tree builder into `noonoo-dom` |
| [`noonoo-dom`](crates/noonoo-dom) | The tree, stable `NodeId`s, checked mutation, queries |
| [`noonoo-geometry`](crates/noonoo-geometry) | Points, sizes, and rectangles |
| [`noonoo-events`](crates/noonoo-events) | Input model and DOM dispatch with bubbling |
| [`noonoo-text`](crates/noonoo-text) | Line breaking + alignment (shaping behind a seam) |
| [`noonoo-motion`](crates/noonoo-motion) | Springs, velocity-preserving retarget |
| [`noonoo-style`](crates/noonoo-style) | Style ingestion and the cascade, including custom properties, `var()`, the math functions (`calc()`, `min()`, `max()`, `clamp()`), viewport units (`vw`, `vh`, `vmin`, `vmax`), and gradients |
| [`noonoo-layout`](crates/noonoo-layout) | Box layout: computed style → boxes (block / inline / flex / grid / positioned / floated) + overflow & scroll containers |
| [`noonoo-a11y`](crates/noonoo-a11y) | Accessibility tree: roles, names, descriptions, states, heading levels, and bounds from DOM + style + layout |
| [`noonoo-markdown`](crates/noonoo-markdown) | Markdown (subset) → HTML |
| [`noonoo-csv`](crates/noonoo-csv) | CSV/TSV → HTML table |
| [`noonoo-ini`](crates/noonoo-ini) | INI → value model (JSON) |
| [`noonoo-toml`](crates/noonoo-toml) | TOML (subset) → value model (JSON) |
| [`noonoo-xml`](crates/noonoo-xml) | XML → DOM (strict nesting, entities, CDATA) |
| [`noonoo-ttf`](crates/noonoo-ttf) | TrueType/OpenType → metrics, `cmap`, outlines, kerning; `FontSet` face selection behind the `Shaper`/`Glyphs` seams |
| [`noonoo-svg`](crates/noonoo-svg) | SVG subset → paint commands |
| [`noonoo-hx`](crates/noonoo-hx) | htmx-like declarative attributes → fetch + DOM swap |
| [`noonoo-caps`](crates/noonoo-caps) | Capability gating: scopes, the policy seam, a gated DOM |
| [`noonoo-net`](crates/noonoo-net) | Loading: `file:` + `data:` owned; `http` behind a feature |
| [`noonoo-engine`](crates/noonoo-engine) | Orchestration: a working document |
| [`noonoo-headless`](crates/noonoo-headless) | Tree dump, virtual clock, synthetic input |
| [`noonoo-paint`](crates/noonoo-paint) | Layout → render commands (fills, gradients, text runs or glyph outlines, clip) |
| [`noonoo-shell`](crates/noonoo-shell) | Multi-document host: layers, input routing, focus, hover/active state, composition |
| [`noonoo`](crates/noonoo) | Umbrella re-export + the `noonoo` command line |

## Status

**Working engine, pre-alpha API (0.20.0, unpublished).** The whole pipeline
runs on owned code and renders real pages; the API is pre-1.0 and still
moving. The GPU and window backends are opt-in integrations outside the
workspace ([`docs/BACKEND.md`](docs/BACKEND.md)).

**Known gaps** — ordinary HTML and CSS that this engine will not render
correctly yet:

- Tables: captions, `table-layout: fixed`, and wrapping stray content inside
  a table or row are not modelled (table parts outside a table do get an
  anonymous table); a `colspan` cell's demand spreads evenly across its
  columns.
- `@media`: `or`, nested conditions and host-supplied preferences (dark mode,
  HiDPI) are not supported; preference queries answer for a light desktop
  window.
- The HTML parser does not implement foster parenting or active formatting
  elements.
- `@keyframes` animations, `srcset`, `<picture>`, `object-fit`, image
  animation and CSS `url()` backgrounds are not implemented.
- 3D transforms are not supported.

See [`docs/ROADMAP.md`](docs/ROADMAP.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md).

## Building

```sh
cargo build
cargo test --workspace
cargo run -p noonoo -- --version
```

Benchmarks and fuzzing live outside the workspace: `tools/bench/` (dependency-free)
and `fuzz/` (cargo-fuzz targets for the parsers) — see those READMEs.

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
noonoo --a11y page.html     # parse → style → layout, print the accessibility tree
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

And a real window (the opt-in integration, GPU-composited):

```sh
cargo run --example showcase --manifest-path integrations/noonoo-winit/Cargo.toml
```

The workspace MSRV is **1.89**. CI checks it explicitly.

## Docs

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate map, data flow, invariants
- [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md) — the dependency policy
- [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md) — what belongs in Noonoo vs. the consumer
- [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) — the drop-in switch and the api-drift gate
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — slices
- [`docs/FORMATS.md`](docs/FORMATS.md) — file formats: DOM vs. value model, and Markdown
- [`docs/FONTS.md`](docs/FONTS.md) — the font layer: what Noonoo owns, the seams, what is borrowed
- [`docs/BACKEND.md`](docs/BACKEND.md) — the GPU and window passes (opt-in, outside the workspace)
- [`docs/SHELL.md`](docs/SHELL.md) — the multi-document host: layers, routing, composition
- [`docs/A11Y.md`](docs/A11Y.md) — the accessibility tree: roles, names, states, and the opt-in bridge
- [`docs/XIP.md`](docs/XIP.md) — capability-gated DOM: the owned seam and the opt-in XIP adapter

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
