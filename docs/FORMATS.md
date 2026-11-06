# Noonoo — Formats

Noonoo is a renderer for *markup*. Every file format it handles is one of two
things, and this note records which — and the pattern for adding more.

## Two destinations

1. **Into the DOM.** Markup and documents that should be rendered (HTML,
   Markdown, and later an SVG subset) are parsed into `noonoo-dom`, then flow
   through the normal **parse → style → layout → paint** pipeline.
2. **Into a value model.** Configuration and data (JSON, CAISON, and future
   TOML/INI) become a typed value tree. They are *not* rendered as pages;
   they configure the engine or feed a host.

The rule of thumb: **if it has a visual form, parse it to the DOM; if it is
settings or data, parse it to a value model.**

## Owned parsers (dependency-free)

| Format | Crate | Destination |
|---|---|---|
| HTML | `noonoo-html` | DOM |
| Markdown | `noonoo-markdown` | DOM (via HTML) |
| CSV / TSV | `noonoo-csv` | DOM (via `<table>`) |
| JSON | `noonoo-json` | value model (`noonoo_json::Json`) |
| CAISON | `noonoo-caison` | value model + JSON projection |
| CSS | `noonoo-css` (+ `noonoo-selectors`) | stylesheets for `noonoo-style` |

## Markdown as a viewer

Because the engine already owns an HTML parser and the full pipeline, Markdown
is a **viewer** for free: convert `.md` → HTML, then render it.

```sh
noonoo --md notes.md        # Markdown -> HTML
noonoo --md-paint notes.md  # Markdown -> HTML -> layout -> paint commands
```

`noonoo-markdown` also ships `DEFAULT_STYLESHEET`, a small CSS for rendered
Markdown, which the viewer injects. Extending the parser (tables, task lists,
footnotes, reference links) is ordinary work inside `noonoo-markdown`.

## Other simple formats, and where they would go

| Format | Pattern | Sketch |
|---|---|---|
| **Plain text** (`.txt`) | DOM | wrap in `<pre>`; one function |
| **CSV / TSV** | DOM | ✅ `noonoo-csv`: parses and emits a `<table>` the engine lays out |
| **INI / TOML-lite** | value model | a `noonoo-ini` beside `noonoo-caison` |
| **Front matter** (`---` YAML-ish in `.md`) | value model | a tiny key/value block parsed *before* Markdown → metadata |
| **SVG** (subset) | DOM / draw commands | parse shapes → `noonoo-paint` commands (Phase E remainder) |

Each new format is its **own `noonoo-<format>` crate** (the same rule that keeps
`noonoo-css` and `noonoo-selectors` separate): one focused parser, no
dependencies, exposing either a DOM builder or a value type.

## Why this is cheap here

The pipeline is already format-neutral: `noonoo-dom` is the single tree every
renderable format targets, and `noonoo_json` is the single value model. A new
format is therefore a parser plus a *thin* adapter — not a new rendering path.
That is the payoff of owning the integration layer instead of borrowing a
monolithic engine.
