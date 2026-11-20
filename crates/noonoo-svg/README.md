# noonoo-svg

An SVG subset renderer that turns SVG source into paint commands.

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-dom`, `noonoo-geometry`, `noonoo-paint`, `noonoo-xml`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
use noonoo_geometry::Size;

let commands = noonoo_svg::render(
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="red"/></svg>"#,
    Size::new(20.0, 20.0),
);
assert!(!commands.is_empty());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
