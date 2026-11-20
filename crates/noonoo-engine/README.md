# noonoo-engine

Orchestration: a parsed, styled and laid-out engine over a DOM document.

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-dom`, `noonoo-geometry`, `noonoo-html`, `noonoo-layout`, `noonoo-net`, `noonoo-paint`, `noonoo-style`. Used by: `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_engine::Engine;
use noonoo_geometry::Size;
use noonoo_layout::MetricShaper;

let engine = Engine::from_html("<style>div { height: 20px; background-color: red }</style><div></div>");
let commands = engine.paint(Size::new(320.0, 200.0), &MetricShaper);
assert!(!commands.is_empty());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
