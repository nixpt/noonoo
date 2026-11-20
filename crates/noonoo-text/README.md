# noonoo-text

Text layer: line breaking, alignment and the `Shaper` seam, with a metric-only default shaper.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used by: `noonoo-ttf`, `noonoo-paint`, `noonoo-layout`, `noonoo-style`, and re-exported by the `noonoo` facade.

## Usage

```rust
// Three words of width 10, a 2px gap, wrapped at 25px: two lines.
let lines = noonoo_text::wrap(&[10.0, 10.0, 10.0], 2.0, 25.0);
assert_eq!(lines.len(), 2);
assert_eq!(lines[0], vec![0, 1]);
```

## Features

- `parley`: declared as the seam for a Parley-backed shaper; currently gates no code
- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
