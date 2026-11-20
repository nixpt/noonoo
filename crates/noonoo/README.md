# noonoo

A modular HTML/CSS rendering engine in small, owned Rust crates, and the `noonoo` command line.

Noonoo turns markup and stylesheets into laid-out, paintable, accessible form: parse, cascade, layout, paint, with an accessibility tree alongside. Every stage is its own crate behind a narrow seam, so a host takes only what it needs. The engine itself has no third-party dependencies; the GPU, window and platform-accessibility bridges are opt-in integration crates outside the workspace.

Pre-1.0: the API is still moving. See the [repository](https://github.com/nixpt/noonoo) for known gaps.

## Where it sits

This is the umbrella crate at the top of the stack. It depends on the other noonoo crates and re-exports each as a module (`noonoo::dom`, `noonoo::html`, `noonoo::engine`, `noonoo::layout`, `noonoo::paint`, and so on), and it also ships the `noonoo` binary. Nothing else in the workspace depends on it. Depend on an individual crate when you only need one stage.

## Usage

```rust
use noonoo::engine::Engine;
use noonoo::geometry::Size;
use noonoo::layout::MetricShaper;

let engine = Engine::from_html("<style>div { height: 20px; background-color: red }</style><div></div>");
let commands = engine.paint(Size::new(320.0, 200.0), &MetricShaper);
assert!(!commands.is_empty());
```

The command line:

```text
noonoo --tree page.html     # parse HTML, print the DOM as JSON
noonoo --a11y page.html     # parse, style, layout; print the accessibility tree
noonoo --paint page.html    # parse, style, layout, paint; print commands as JSON
noonoo --md notes.md        # Markdown to HTML
```

Run `noonoo --help` for the full list (CSV, INI, TOML, XML, SVG and CAISON are also supported).

## Features

- `std`: enabled by default; forwards to the member crates' `std` features, which gate no code
- `a11y`: enabled by default; builds the accessibility tree in the shell (`noonoo-shell/a11y`)
- `window`: forwards to `noonoo-shell/window`; currently gates no code
- `gpu`: forwards to `noonoo-shell/gpu`; currently gates no code
- `http`: forwards to `noonoo-net/http`; currently gates no code

Default features: `std`, `a11y`.

## Crates

| Crate | Responsibility |
| --- | --- |
| [`noonoo-parse`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-parse) | Shared parsing primitives: spans, positions, errors, cursor |
| [`noonoo-json`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-json) | JSON parser, value model, and writer |
| [`noonoo-css`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-css) | CSS: tokenizer, rule splitter, value/property model (incl. grouping), shorthand expansion, `@font-face` descriptors |
| [`noonoo-selectors`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-selectors) | CSS selectors: parse, specificity, matching |
| [`noonoo-query`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-query) | Full-selector and element-scoped DOM queries |
| [`noonoo-caison`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-caison) | CAISON parse and its JSON projection |
| [`noonoo-html`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-html) | HTML tokenizer and tree builder into `noonoo-dom` |
| [`noonoo-dom`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-dom) | The tree, stable `NodeId`s, checked mutation, queries |
| [`noonoo-geometry`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-geometry) | Points, sizes, and rectangles |
| [`noonoo-events`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-events) | Input model and DOM dispatch with bubbling |
| [`noonoo-text`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-text) | Line breaking + alignment (shaping behind a seam) |
| [`noonoo-motion`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-motion) | Springs, velocity-preserving retarget |
| [`noonoo-style`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-style) | Style ingestion and the cascade, including custom properties, `var()`, the math functions (`calc()`, `min()`, `max()`, `clamp()`), viewport units (`vw`, `vh`, `vmin`, `vmax`), and gradients |
| [`noonoo-layout`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-layout) | Box layout: computed style → boxes (block / inline / flex / grid / positioned / floated) + overflow & scroll containers |
| [`noonoo-a11y`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-a11y) | Accessibility tree: roles, names, descriptions, states, heading levels, and bounds from DOM + style + layout |
| [`noonoo-markdown`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-markdown) | Markdown (subset) → HTML |
| [`noonoo-csv`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-csv) | CSV/TSV → HTML table |
| [`noonoo-ini`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-ini) | INI → value model (JSON) |
| [`noonoo-toml`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-toml) | TOML (subset) → value model (JSON) |
| [`noonoo-xml`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-xml) | XML → DOM (strict nesting, entities, CDATA) |
| [`noonoo-ttf`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-ttf) | TrueType/OpenType → metrics, `cmap`, outlines, kerning; `FontSet` face selection behind the `Shaper`/`Glyphs` seams |
| [`noonoo-svg`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-svg) | SVG subset → paint commands |
| [`noonoo-hx`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-hx) | htmx-like declarative attributes → fetch + DOM swap |
| [`noonoo-caps`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-caps) | Capability gating: scopes, the policy seam, a gated DOM |
| [`noonoo-net`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-net) | Loading: `file:` + `data:` owned; `http` behind a feature |
| [`noonoo-engine`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-engine) | Orchestration: a working document |
| [`noonoo-headless`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-headless) | Tree dump, virtual clock, synthetic input |
| [`noonoo-paint`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-paint) | Layout → render commands (fills, gradients, text runs or glyph outlines, clip) |
| [`noonoo-shell`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo-shell) | Multi-document host: layers, input routing, focus, hover/active state, composition |
| [`noonoo`](https://github.com/nixpt/noonoo/tree/main/crates/noonoo) | Umbrella re-export + the `noonoo` command line |

## License

Dual-licensed under [MIT](https://github.com/nixpt/noonoo/blob/main/LICENSE-MIT) or [Apache-2.0](https://github.com/nixpt/noonoo/blob/main/LICENSE-APACHE), at your option (MIT OR Apache-2.0). Source: [github.com/nixpt/noonoo](https://github.com/nixpt/noonoo).
