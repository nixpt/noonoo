# noonoo-markdown

A CommonMark-subset Markdown to HTML converter, with front matter splitting.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let html = noonoo_markdown::markdown_to_html("# Title\n\nSome *text*.");
assert!(html.contains("<h1>"));
let (front, body) = noonoo_markdown::split_front_matter("---\ntitle: Hi\n---\nbody\n");
assert_eq!(front.get("title"), Some("Hi"));
assert!(body.contains("body"));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
