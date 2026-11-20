# noonoo-html

An HTML tokenizer and tree builder that parses markup into a `noonoo-dom` document.

## Where it sits

Depends on these noonoo crates: `noonoo-dom`, `noonoo-parse`. Used by: `noonoo-engine`, `noonoo-hx`, and re-exported by the `noonoo` facade.

## Usage

```rust
let doc = noonoo_html::parse(r#"<p id="a">Hello <b>world</b></p>"#);
let p = doc.get_element_by_id("a").unwrap();
assert_eq!(doc.text_content(p), "Hello world");
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
