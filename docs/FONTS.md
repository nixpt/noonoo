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

**Shaping** (`noonoo_text::Shaper`) — two questions, `advance` and `line_height`:

- `FontShaper` measures a single face (real advances, line metrics, and `kern`).
- `FontSetShaper` selects a face per call from a `FontSet` by family, weight, and
  style — so `font-weight: bold` picks a real bold face when one is registered.

**Glyphs** (`noonoo_paint::Glyphs`) — how a run is drawn:

- `TextRuns` (the default) emits `Command::DrawText`.
- `OutlineGlyphs` emits one `Command::FillPolygon` per glyph contour — **vector
  text** through the same command stream as every other shape, with no GPU text
  backend. It advances the pen with the *same* metrics as `FontShaper`, kerning
  included.

```sh
# measure with the font, paint with its outlines:
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
fixture font (deterministic, no embedded, licensed file). The `--paint --font`
path renders real fonts as vector text.
