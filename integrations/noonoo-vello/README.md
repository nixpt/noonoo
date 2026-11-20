# noonoo-vello

Opt-in GPU pass: translates a `noonoo_paint::CommandList` into a `vello::Scene`, ready for Vello's wgpu renderer.

## Where it sits

Depends on these noonoo crates: `noonoo-paint`, `noonoo-geometry`, `noonoo-ttf`, plus the third-party `vello` (with `default-features = false`) and the `png`, `zune-jpeg`, `zune-core`, `gif` and `image-webp` image decoders. Nothing in the noonoo workspace depends on it. It is published from outside the workspace so the engine's own build and lockfile stay free of the GPU stack, and it is the one place Vello is named.

Text must arrive as glyph outlines (paint with `noonoo-ttf`'s `OutlineGlyphs`); a bare text run is drawn as a placeholder box. `noonoo_vello::fonts::system()` looks for common system font faces.

## Usage

```rust
let commands = noonoo_paint::CommandList(vec![noonoo_paint::Command::FillRect {
    rect: noonoo_geometry::Rect::new(0.0, 0.0, 10.0, 10.0),
    color: noonoo_paint::Color::rgb(255, 0, 0),
    radius: (0.0, 0.0, 0.0, 0.0),
}]);
let scene = noonoo_vello::scene(&commands);
assert!(!scene.encoding().path_tags.is_empty());
```

## Features

None.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
