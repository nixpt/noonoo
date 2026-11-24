# noonoo-wpt-tree

Runs the tree-construction cases of the
[web-platform tests](https://github.com/web-platform-tests/wpt) through
noonoo's HTML parser. The cases are not vendored: point it at a checkout.

It lives outside the workspace and off CI. Nothing in the engine depends on
it; the engine's own tests spell out the trees they expect.

## Running

```sh
git clone --depth 1 --filter=blob:none --sparse https://github.com/web-platform-tests/wpt
git -C wpt sparse-checkout set html/syntax/parsing/resources
cargo run --release -- wpt/html/syntax/parsing/resources
```

It prints the cases that fail (with `-v`, both trees) and exits 1 if any do.
Arguments are directories of `.dat` files, or files.

At web-platform-tests `721fe4e`: 1874 pass, 4 fail, 95 skipped. The four are
`<selectedcontent>`, which noonoo doesn't fill in.

## What isn't compared

- The case of SVG and MathML element names, which noonoo stores in lower
  case, and a doctype's identifiers, which it doesn't store.
- Skipped: cases with scripting on, and those with processing instructions,
  which noonoo's DOM doesn't have.
