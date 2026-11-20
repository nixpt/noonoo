# noonoo-css

CSS tokenizer, stylesheet parser and the value/property model used by the style engine.

## Where it sits

Depends on these noonoo crates: `noonoo-parse`. Used by: `noonoo-selectors`, `noonoo-paint`, `noonoo-layout`, `noonoo-style`, `noonoo-svg`, `noonoo-engine`, `noonoo-caps`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_css::{Color, Stylesheet};

let sheet = Stylesheet::parse("p { color: red; margin: 0 }").unwrap();
let _ = &sheet;
let declarations = noonoo_css::parse_declarations("color: red; width: 10px");
assert_eq!(declarations.len(), 2);
assert!(Color::from_hex("ff0000").is_some());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
