# noonoo-query

Full-selector DOM queries (`query_selector`, `query_selector_all`, `matches`) and element-scoped variants.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`, `noonoo-parse`, `noonoo-selectors`. Used by: `noonoo-style`.

## Usage

```rust
let doc = noonoo_html::parse("<ul><li class=a>1</li><li>2</li></ul>");
let all = noonoo_query::query_selector_all(&doc, "li").unwrap();
assert_eq!(all.len(), 2);
let first = noonoo_query::query_selector(&doc, "li.a").unwrap().unwrap();
assert!(noonoo_query::matches(&doc, first, "ul > li").unwrap());
```

## Features

None.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
