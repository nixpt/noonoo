# noonoo-json

A dependency-free JSON parser, value model and writer (RFC 8259), with object key order preserved.

## Where it sits

Depends on these noonoo crates: `noonoo-parse`. Used by: `noonoo-caison`, `noonoo-ini`, `noonoo-toml`, `noonoo-paint`, `noonoo-headless`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_json::Json;

let value = Json::parse(r#"{"name": "noonoo", "tags": [1, 2]}"#).unwrap();
assert_eq!(value.get("tags").unwrap().as_array().unwrap().len(), 2);
assert!(value.to_string_pretty().contains("\"name\""));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
