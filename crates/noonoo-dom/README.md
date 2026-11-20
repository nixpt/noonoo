# noonoo-dom

The document tree: stable node identities, checked mutation, and queries.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used by: `noonoo-html`, `noonoo-events`, `noonoo-query`, `noonoo-xml`, `noonoo-layout`, `noonoo-style`, `noonoo-svg`, `noonoo-a11y`, `noonoo-engine`, `noonoo-headless`, `noonoo-hx`, `noonoo-caps`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_dom::Document;

let mut doc = Document::new();
let root = doc.root();
let div = doc.create_element("div");
doc.mutate().append_child(root, div).unwrap();
assert_eq!(doc.element(div).unwrap().name, "div");
assert_eq!(doc.tree().children(root).len(), 1);
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
