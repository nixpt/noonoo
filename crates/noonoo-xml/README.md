# noonoo-xml

A strict XML parser (nesting, entities, CDATA) that builds a `noonoo-dom` document.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`, `noonoo-parse`. Used by: `noonoo-svg`, and re-exported by the `noonoo` facade.

## Usage

```rust
let doc = noonoo_xml::parse("<root><item id=\"a\">x</item></root>").unwrap();
let item = doc.get_element_by_id("a").unwrap();
assert_eq!(doc.text_content(item), "x");
assert!(noonoo_xml::parse("<root><item></root>").is_err());
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
