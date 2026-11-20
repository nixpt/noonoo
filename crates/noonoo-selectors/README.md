# noonoo-selectors

CSS selector parsing, specificity and matching against any type that implements the `Element` trait.

## Where it sits

Depends on these noonoo crates: `noonoo-css`, `noonoo-parse`. Used by: `noonoo-query`, `noonoo-style`, and re-exported by the `noonoo` facade.

## Usage

```rust
use noonoo_selectors::SelectorList;

let list = SelectorList::parse("ul > li.active").unwrap();
let plain = SelectorList::parse("li").unwrap();
assert!(list.max_specificity() > plain.max_specificity());
assert!(SelectorList::parse("li >").is_err());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
