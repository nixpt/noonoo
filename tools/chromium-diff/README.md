# noonoo-chromium-diff

Lays a page out with noonoo and with headless Chromium and compares every
element's border box, in DOM order. It is noonoo's cross-check against the
browser it aims to match; consumers (yantra's dashboard fragments) run it on
their own pages too.

It lives outside the workspace and off CI: it reads system fonts and needs a
Chromium binary. Nothing in the engine depends on it.

## Running

```sh
cargo build --release
TOL=1 python3 -I cmp.py page.html [page.html ...] [w=1024]
```

Run it with no other cargo build beside it: a busy machine slows Chromium
past its timeout. `cmp.py` prints every element whose box differs by more
than `TOL` px (default 1), `ALIGN` where the two DOMs differ, and `NO BOX`
where Chromium has a box and noonoo none; it exits 1 if any page has a
mismatch.

The environment configures it:

| Variable | Default | |
|---|---|---|
| `XDUMP` | `target/release/noonoo-chromium-diff` (or under `$CARGO_TARGET_DIR`) | noonoo's side |
| `CHROMIUM` | `/usr/bin/chromium` | Chromium's side |
| `FONT_DIR` | `/usr/share/fonts/TTF` | the four DejaVu Sans faces |
| `FALLBACK_FONTS` | none | `:`-separated font files for scripts DejaVu lacks |
| `TOL` | `1` | px; Chromium rounds text widths to 1/64 px |
| `SVG_SKIPS` | unset | see below |

`python3 -I chrome.py page.html [width]` prints Chromium's side alone, and
`target/release/noonoo-chromium-diff page.html [width]` noonoo's.

## What it compares

- Both engines get a 600px-tall viewport of the given width (1024 by
  default) and DejaVu Sans: forced in Chromium by an `!important` rule, and
  registered in noonoo under every family name a page may ask for.
- Chromium is measured once the page's `load` event has fired, with the
  network off (`--host-resolver-rules`), so remote images fail to load, as
  they do for noonoo here. Its broken-image layout (alt text after a 16px
  icon) is what noonoo matches.
- Elements are matched by position in DOM order from `<body>`, so the first
  parser disagreement shows up as `ALIGN` lines.
- An element with no box in either engine (`<br>`, table parts, a zero-size
  box) isn't counted.

### `<svg>` contents

noonoo lays an inline `<svg>` out as one replaced box and models no SVG
geometry, so the shapes inside it get no box, while Chromium reports each
shape's bounding box. With `SVG_SKIPS=N`, an element Chromium places inside an
`<svg>` that noonoo gives no box is printed as `SKIP` and not counted, and the
run fails unless exactly `N` were skipped, so a new skip shows up as a change.
The `<svg>`'s own box is always compared. Unset or empty, they count as
`NO BOX`; a value that isn't a whole number exits 2.
