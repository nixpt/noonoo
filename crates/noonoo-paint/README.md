# noonoo-paint

Paint: a layout tree into a backend-neutral stream of render commands (fills, gradients, text runs or glyph outlines, clips).

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-geometry`, `noonoo-json`, `noonoo-layout`, `noonoo-style`, `noonoo-text`. Used by: `noonoo-ttf`, `noonoo-svg`, `noonoo-engine`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
let doc = noonoo_html::parse(r#"<div id="box" style="width: 100px; height: 50px">hi</div>"#);
let styles = noonoo_style::compute(&doc, &[]);
let size = noonoo_geometry::Size::new(800.0, 600.0);
let layout = noonoo_layout::layout(&doc, &styles, size, &noonoo_layout::MetricShaper);
let commands = noonoo_paint::paint(&styles, &layout);
assert!(!commands.is_empty());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
