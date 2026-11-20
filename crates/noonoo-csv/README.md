# noonoo-csv

CSV and TSV parsing, with delimiter detection and an HTML table renderer.

## Where it sits

Depends on no other noonoo crate; it is a leaf of the stack. Used only by the `noonoo` facade, which re-exports it.

## Usage

```rust
let rows = noonoo_csv::parse_csv("a,b\n1,2");
assert_eq!(rows, vec![vec!["a", "b"], vec!["1", "2"]]);
assert!(noonoo_csv::to_html("a,b\n1,2").contains("<table"));
```

## Features

- `std`: enabled by default; currently gates no code

Default features: `std`.

## License

MIT OR Apache-2.0. Part of [noonoo](https://github.com/nixpt/noonoo).
