# Noonoo — Fonts

Noonoo owns its **font format parsing** and text placement, and borrows only the
genuinely enormous *shaping* engine. The line is the same one drawn everywhere
else: own the small, spec-shaped syntax; buy the multi-year engine
(`docs/DEPENDENCY-EXIT.md`).

## The crates

| Crate | Owns |
|---|---|
| **`noonoo-ttf`** | TrueType/OpenType **file parsing** — metrics, `cmap`, glyph outlines, legacy kerning — plus a `Shaper` and a `Glyphs` built on it |
| `noonoo-text` | The `Shaper` **seam** and line breaking / alignment |
| `noonoo-paint` | The `Glyphs` **seam** and the command stream |

`noonoo-ttf` is a format parser in the same spirit as `noonoo-xml` and
`noonoo-svg`: it depends only on `noonoo-parse`, `noonoo-text`, `noonoo-geometry`,
and `noonoo-paint`, and adds no third-party dependency.

## What it parses

| Table | Gives |
|---|---|
| `head`, `maxp` | units per em, glyph count |
| `hhea` / `hmtx` | ascender, descender, line gap, per-glyph advance widths |
| `cmap` | character → glyph (formats **4** and **12**) |
| `loca` / `glyf` | TrueType outlines, **simple** and **composite**, flattened to polygons |
| `kern` | legacy pair kerning (version 0, format 0, horizontal) |

`Font` exposes `units_per_em`, `ascender`/`descender`/`line_gap`,
`glyph_for_char`, `advance`, `kern`, `outline`, `has_outlines`, and
`text_polygons`.

## The two seams

**Shaping** (`noonoo_text::Shaper`) — two questions, `advance` and `line_height`,
each taking a `FontRequest` (the element's family list, weight, and style):

- `MetricShaper` (the default) ignores the request — a deterministic baseline.
- `FontShaper` measures a single face (real advances, line metrics, and `kern`).
- `FontSetShaper` selects a face per call from a `FontSet`: the first family in
  the request with any registered face wins, matching style preferred, then
  nearest weight — so `font-weight: bold` picks a real bold face. A face
  registered under the empty family name matches every request, which is how a
  host offers a universal fallback. No match at all falls back to the
  deterministic heuristic.

**Glyphs** (`noonoo_paint::Glyphs`) — how a run is drawn:

- `TextRuns` (the default) emits `Command::DrawText`.
- `OutlineGlyphs` emits one `Command::FillContours` per glyph, its contours
  filled together so holes stay open — **vector
  text** through the same command stream as every other shape, with no GPU text
  backend. It selects a face from a `FontSet` per request, the same way
  `FontSetShaper` does, and advances the pen with the same metrics, kerning
  included — so a document laid out with a `FontSetShaper` paints with the very
  faces it was measured against.

## `@font-face`

CSS may declare faces directly:

```css
@font-face {
  font-family: "Demo";
  src: url(fonts/Demo.ttf) format("truetype");
  font-weight: 700;
  font-style: italic;
}
```

`noonoo-css` parses these into `FontFace` descriptors (`Stylesheet::font_faces`),
and `Engine::font_faces()` aggregates them in source order. Loading the bytes is
the host's job — the ownership line stays clean: the engine never touches the
filesystem. The CLI's `--paint` shows the pattern: each descriptor's first
resolvable `src` (`file:`/`data:` via `noonoo-net`, relative paths joined to the
document's directory) is loaded, parsed, and registered under its
family/weight/style; `--font` additionally registers a universal fallback.

```sh
# `@font-face` in the page resolves on its own; --font is a fallback for
# anything the page does not declare:
noonoo --paint page.html
noonoo --paint page.html --font /path/to/Font.ttf
```

## What is deliberately *not* here

These are the borrowed, 100k+-LOC concerns — kept out on purpose:

- **GSUB / GPOS** — ligatures and contextual substitution.
- **Complex scripts and BiDi** — Arabic, Indic, Thai, and right-to-left runs.
- **CFF outlines.** TrueType outlines are *quadratic* (our `Outline` stores
  on/off-curve points); CFF (`OTTO`) uses *cubic* Béziers. Reading CFF needs both
  a Type 2 CharString interpreter and a path model that carries cubic segments —
  a real slice, not a patch. Until then a CFF font's **metrics still parse**;
  only `outline` returns `None`.
- **System font enumeration and loading** (`fontique`'s job). A host registers
  the bytes it has into a `FontSet`; Noonoo does not scan the filesystem.
- **Hinting and antialiasing** — the rasterizer's job.

## Status

`noonoo-ttf`: metrics, `cmap`, `glyf` outlines, `kern`, `FontShaper`,
`FontSetShaper`, `FontSet`, and `OutlineGlyphs`, all tested against a hand-built
fixture font (deterministic, no embedded, licensed file). `@font-face`
declarations parse into descriptors and the `--paint` path loads and registers
them, rendering real fonts as vector text with per-element face selection.
