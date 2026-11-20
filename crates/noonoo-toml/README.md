# noonoo-toml

A parser for a documented TOML subset, projected into the `noonoo-json` value model.

## Where it sits

Depends on these noonoo crates: `noonoo-json`, `noonoo-parse`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let json = noonoo_toml::to_json("name = \"noonoo\"\n[server]\nport = 8080\n").unwrap();
assert!(json.get("name").is_some());
assert!(json.get("server").is_some());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
