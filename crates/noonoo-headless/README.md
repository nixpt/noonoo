# noonoo-headless

Headless tooling: a JSON tree dump, a virtual clock, and scripted synthetic input.

## Where it sits

Depends on these noonoo crates: `noonoo-a11y`, `noonoo-dom`, `noonoo-events`, `noonoo-geometry`, `noonoo-json`, `noonoo-layout`, `noonoo-style`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let doc = noonoo_html::parse("<div><p>hi</p></div>");
let json = noonoo_headless::tree_json(&doc);
assert!(json.contains("\"p\""));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
