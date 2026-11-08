# noonoo-bliss-diff

Lays pages out with noonoo and with bliss (the engine noonoo replaces) and
compares every element's border box. It is the yardstick for Phase 5's
"real pages render through noonoo, diffed against bliss".

It lives outside the workspace and off CI: it builds bliss-engine (stylo,
taffy, parley) from a sibling checkout at `../../../bliss-engine`. Nothing in
the engine depends on it; it only reads bliss's output.

## Running

```sh
cargo run --release -- pages/*.html            # our own pages (committed)
python3 fetch.py /tmp/web && cargo run --release -- /tmp/web/*.html
```

`fetch.py` downloads public pages with their stylesheets inlined and scripts
removed (neither engine loads `<link>` stylesheets or runs scripts here). They
are not committed: their licences vary.

Options: `--width`/`--height` (viewport, 1024 × 768), `--tolerance` (px, 1),
`--worst N` (offenders listed per page, 8), `--fonts DIR` (DejaVu Sans faces,
`/usr/share/fonts/TTF`).

## What it compares

- Both engines get the same viewport and fonts: DejaVu Sans, forced in bliss
  by a user-agent `!important` rule and registered as noonoo's
  match-anything face. Neither loads images.
- Elements from `<body>` down are aligned by their tag sequence (a longest
  common subsequence), so a parser disagreement shifts the alignment locally.
- *Boxes within N px*: all of x, y, width and height agree. *Sizes*: width and
  height agree, wherever the box is; one early vertical difference moves every
  box below it, so sizes say more about the boxes themselves.
- bliss lays non-atomic inline elements out inside their parent's text and
  gives them no box, so those are not compared.
- bliss rounds boxes to whole pixels and takes `line-height: normal` from the
  font; noonoo uses 1.2em. A paragraph's height can differ by a pixel or two
  without either being wrong.

## Baseline (2026-11-08, 1024 × 768, 1 px)

| | positions | sizes |
|---|---|---|
| our pages + 8 public pages | 3% | 17% |
| after `ec84ff1` (margins, centring, invalid `display`, percentage cells) | 3% | 24% |
| after `dd3a669` (text fields, preserved newlines, implied `html`/`body`; danluu.com now compares) | 20% | 47% |
| after the UA sheet's margins, list indents and block list | 23% | 62% |
