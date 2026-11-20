# noonoo-events

The input event model and DOM event dispatch, walking capture, target and bubble phases.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`. Used by: `noonoo-headless`, `noonoo-hx`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_dom::Document;
use noonoo_events::{dispatch_path, EventPhase};

let mut doc = Document::new();
let root = doc.root();
let outer = doc.create_element("div");
let inner = doc.create_element("span");
doc.mutate().append_child(root, outer).unwrap();
doc.mutate().append_child(outer, inner).unwrap();

let path = dispatch_path(doc.tree(), inner);
assert_eq!(path.last().unwrap().1, EventPhase::Bubble);
assert!(path.iter().any(|&(node, phase)| node == inner && phase == EventPhase::Target));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
