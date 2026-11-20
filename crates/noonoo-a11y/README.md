# noonoo-a11y

The accessibility tree: roles, names, descriptions, states and bounds projected from DOM, style and layout.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`, `noonoo-geometry`, `noonoo-layout`, `noonoo-style`. Used by: `noonoo-headless`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_a11y::{build, Role};

let doc = noonoo_html::parse(r#"<button aria-label="Save">x</button>"#);
let styles = noonoo_style::compute(&doc, &[]);
let tree = build(&doc, &styles);
assert_eq!(tree.root.role, Role::Document);
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
