# noonoo-ini

An INI parser: sections and key/value pairs, with a projection to the JSON value model.

## Where it sits

Depends on these noonoo crates: `noonoo-json`. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let ini = noonoo_ini::parse("[server]\nport = 8080\n");
assert_eq!(ini.get("server", "port"), Some("8080"));
let json = noonoo_ini::to_json("[server]\nport = 8080\n");
assert!(json.get("server").is_some());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
