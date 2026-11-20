# noonoo-layout

Box layout: computed styles into a positioned box tree (block, inline, flex, grid, positioned, floats, scroll containers).

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-dom`, `noonoo-geometry`, `noonoo-style`, `noonoo-text`. Used by: `noonoo-paint`, `noonoo-a11y`, `noonoo-engine`, `noonoo-headless`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
let doc = noonoo_html::parse(r#"<div id="box" style="width: 100px; height: 50px">hi</div>"#);
let styles = noonoo_style::compute(&doc, &[]);
let size = noonoo_geometry::Size::new(800.0, 600.0);
let layout = noonoo_layout::layout(&doc, &styles, size, &noonoo_layout::MetricShaper);
let id = doc.get_element_by_id("box").unwrap();
let rect = layout.bounding_client_rect(id).unwrap();
assert_eq!(rect.width, 100.0);
assert_eq!(rect.height, 50.0);
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
