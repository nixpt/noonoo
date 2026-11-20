# noonoo-parse

Shared parsing primitives: byte spans, error type and a character cursor, used by every noonoo parser.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used by: `noonoo-html`, `noonoo-json`, `noonoo-css`, `noonoo-selectors`, `noonoo-query`, `noonoo-caison`, `noonoo-toml`, `noonoo-xml`, `noonoo-ttf`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_parse::Cursor;

let mut cursor = Cursor::new("ab=1");
assert_eq!(cursor.peek(), Some('a'));
assert!(cursor.eat('a'));
assert!(cursor.eat('b'));
assert!(cursor.eat('='));
assert_eq!(cursor.remaining(), "1");
let err = cursor.error("unexpected input");
assert_eq!(err.message, "unexpected input");
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
