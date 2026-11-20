# noonoo-style

The CSS cascade, inheritance and computed values: a DOM plus stylesheets into per-node computed styles.

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-dom`, `noonoo-query`, `noonoo-selectors`, `noonoo-text`. Used by: `noonoo-paint`, `noonoo-layout`, `noonoo-a11y`, `noonoo-engine`, `noonoo-headless`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
let doc = noonoo_html::parse(r#"<p id="p">hi</p>"#);
let styles = noonoo_style::compute(&doc, &[]);
let p = doc.get_element_by_id("p").unwrap();
assert!(styles.get(p).is_some());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
