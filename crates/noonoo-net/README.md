# noonoo-net

Document loading: `file:` and `data:` URLs, URL resolution, and a loader trait.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used by: `noonoo-engine`, `noonoo-hx`, and re-exported by the `noonoo` facade.

## Usage

```rust
let url = noonoo_net::resolve("file:///site/index.html", "style.css");
assert!(url.ends_with("/site/style.css"));
let bytes = noonoo_net::load("data:text/plain,hello").unwrap();
assert_eq!(bytes, b"hello");
```

## Features

- `http`: declared for `http(s)` loading; currently gates no code, so only `file:` and `data:` load
- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
