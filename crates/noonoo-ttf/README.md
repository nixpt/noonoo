# noonoo-ttf

A dependency-free TrueType/OpenType parser: metrics, cmap, glyph outlines, kerning, and font-backed `Shaper` and glyph renderer.

## Where it sits

Depends on these noonoo crates: `noonoo-geometry`, `noonoo-paint`, `noonoo-parse`, `noonoo-text`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust,ignore
let bytes = std::fs::read("font.ttf").unwrap();
let font = noonoo_ttf::Font::parse(&bytes).unwrap();
let glyph = font.glyph_for_char('A').unwrap();
let advance = font.advance(glyph);
let _ = (font.units_per_em(), advance);
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
