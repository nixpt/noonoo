# noonoo-shell

A document host: several documents in one window, with input routing, focus, hover state and composition.

## Where it sits

Depends on these noonoo crates: `noonoo-a11y`, `noonoo-dom`, `noonoo-engine`, `noonoo-events`, `noonoo-geometry`, `noonoo-layout`, `noonoo-paint`, `noonoo-style`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
use noonoo_engine::Engine;
use noonoo_geometry::{Rect, Size};
use noonoo_layout::MetricShaper;
use noonoo_shell::Host;

let mut host = Host::new(Size::new(400.0, 300.0));
host.add_content(
    Engine::from_html("<p>hello</p>"),
    Rect::new(0.0, 0.0, 400.0, 300.0),
);
host.layout(&MetricShaper);
```

## Features

- `a11y`: builds the accessibility tree for the hosted documents (enables the optional `noonoo-a11y` dependency)
- `gpu`: declared for the GPU backend (an opt-in integration outside the workspace); currently gates no code
- `std`: enabled by default; currently gates no code
- `window`: declared for the windowed backend (an opt-in integration outside the workspace); currently gates no code

Default features: `std`, `a11y`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
