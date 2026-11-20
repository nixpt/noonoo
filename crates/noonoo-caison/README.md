# noonoo-caison

A parser for CAISON (Crush AI-native Semantic Object Notation) and its projection onto JSON.

## Where it sits

Depends on these noonoo crates: `noonoo-json`, `noonoo-parse`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let doc = noonoo_caison::Document::parse("@caison { version: \"1.0\" }\nkey: \"value\"\n").unwrap();
assert_eq!(doc.version, "1.0");
let json = doc.to_json().unwrap();
assert!(json.get("key").is_some());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
