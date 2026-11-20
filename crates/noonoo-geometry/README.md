# noonoo-geometry

Points, sizes and rectangles in `f32` pixels, shared by layout, paint and the shell.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used by: `noonoo-ttf`, `noonoo-paint`, `noonoo-layout`, `noonoo-svg`, `noonoo-a11y`, `noonoo-engine`, `noonoo-headless`, `noonoo-shell`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_geometry::{Point, Rect};

let rect = Rect::new(10.0, 10.0, 100.0, 50.0);
assert!(rect.contains(Point::new(20.0, 20.0)));
assert!(!rect.contains(Point::new(0.0, 0.0)));
let inner = rect.inset(5.0, 5.0, 5.0, 5.0);
assert_eq!(inner.width, 90.0);
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
