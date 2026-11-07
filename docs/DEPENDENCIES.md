# Noonoo — Dependency Policy

The engine it replaces locks **634 packages** (with `bliss-dom` alone pulling
**192** normal-transitive crates). Noonoo's dependency story is a designed
outcome, not a byproduct.

## The rule: buy the engines, own the seams

A rendering engine is a browser's worth of solved problems. Re-deriving CSS,
flexbox/grid, text shaping, or GPU 2D is not where the value is. We **buy**
them and **own** the integration.

| Engine | Reached from | Verdict |
| --- | --- | --- |
| `stylo` (+ `selectors`, `cssparser`) | — | **Owned.** Phase A replaced it with a documented CSS subset (`noonoo-css` + `noonoo-style`); `stylo` is not a dependency. See [`DEPENDENCY-EXIT.md`](DEPENDENCY-EXIT.md). |
| `taffy` | — | **Owned.** Phase B replaced it with `noonoo-layout` (block / inline / flex); `taffy` is not a dependency. |
| `parley` (+ `fontique`/`swash`/`skrifa`) | `noonoo-text` | **Keep for shaping.** Line breaking + alignment are owned (`noonoo-text`) and font *metrics* are owned (`noonoo-ttf`); only shaping — kerning, ligatures, complex scripts, family resolution — stays borrowed behind the `Shaper` seam. |
| `anyrender` / `vello` / `wgpu` | `noonoo-paint`, `noonoo-shell` | **Keep (backend).** The command stream is owned (`noonoo-paint`); the GPU 2D backend is behind a feature, not wired yet. |
| `winit`, `accesskit` | `noonoo-shell` | **Keep.** Windowing + a11y, behind features. |

## The rules that keep the surface thin

1. **One module per borrowed engine.** `taffy` is named only in `noonoo-layout`;
   `parley` only in `noonoo-text`. The contact points are countable in one file,
   so "what would a swap cost?" has an answer.
2. **No abstraction-for-its-own-sake.** These engines will not be swapped, so
   they are *not* hidden behind traits. A wrap-for-wrap's-sake layer costs more
   than it saves.
3. **Feature-gate every backend.** A build that only needs the engine pays for
   nothing else. Default features must never pull a GPU, a window, or a screen
   reader.
4. **No forks, no vendoring, no `[patch]`.** If a boundary bug needs fixing, it
   is fixed by pinning or by a *boundary-sized* shim in-tree — never by copying
   an engine in.
5. **Pin deliberately, upgrade on purpose.** One lock file; no floating majors
   in the manifest for load-bearing engines.

## Incidental dependencies we decline

A non-exhaustive list of things the previous engine carried that Noonoo will not,
because the standard library or an already-present dependency covers them:

- `html-escape`, `percent-encoding` — subsumed by URL/string handling.
- `atomic_refcell`, `thread_local`, `fastrand`, `downcast-rs`, `slab` — the
  arena and ordinary std types cover these.
- `wuff` — font loading is `fontique`'s job.
- Vendored forks of `accesskit_xplat` / `anyrender_vello` — prefer a boundary
  shim over a published clone.
- Alpha-channel HTTP caching middleware as a hard default — opt-in at most.

## Owned syntax (the parser crates)

One class of dependency Noonoo *does* write itself: **format parsers**. The
parser crates — `noonoo-parse`, `noonoo-json`, `noonoo-css`, `noonoo-selectors`,
`noonoo-caison`, `noonoo-html`, `noonoo-markdown`, `noonoo-csv`, `noonoo-ini`,
`noonoo-toml`, `noonoo-xml`, `noonoo-ttf`, and `noonoo-svg` — are dependency-free
and exist because a parser is a *syntax* layer: small, spec-defined, and
load-bearing for the engine's own formats.

- **JSON** (`noonoo-json`) is needed to *project* other formats into a
  machine-readable shape (CAISON does this), so owning it removes a ubiquitous
  incidental dependency.
- **CAISON** (`noonoo-caison`) is the ecosystem's AI-native config format; a
  parser for it belongs beside the tree it feeds.
- **HTML** (`noonoo-html`) parses into `noonoo-dom` directly — parsing must never be
  a silent no-op (the replaced engine let consumers forget to install a parser
  provider).
- **XML** (`noonoo-xml`) also parses into `noonoo-dom` (strict nesting, entities,
  CDATA, comments); it is the honest base the SVG layer should sit on.
- **TOML** (`noonoo-toml`) projects into the JSON value model like INI and
  CAISON, so configuration has an owned parser with no incidental dependency.
- **Fonts** (`noonoo-ttf`) parses TrueType/OpenType metrics (`head`, `maxp`,
  `hhea`/`hmtx`, `cmap`) into real advance widths and line metrics, the legacy
  `kern` table into pair kerning, and TrueType glyph outlines (`loca`/`glyf`,
  simple and composite) into flattened polygons — so text can measure and render
  as vector paths without owning modern shaping (GSUB/GPOS).
- **CSS** (`noonoo-css` + `noonoo-style`) is **owned end to end** — tokens, selectors,
  the value/property model, and the cascade (`docs/DEPENDENCY-EXIT.md` Phase A).
  No `stylo`, no `cssparser`.

The line is deliberate: we own the small, spec-shaped syntax; we buy the
multi-year engines.

## Owned policy, borrowed crypto

Capability gating follows the same split as the parsers. **`noonoo-caps` is
owned and dependency-free** — the scope/grant model, the `Authorizer` seam, and
the gated DOM. The cryptography behind a real token (Ed25519, SHA-2, `chrono`,
`bs58`) is **borrowed, and opt-in only**: the `noonoo-xip` adapter verifies a
signed Exosphere Identity Protocol token through `xip-core`, and lives *outside*
the workspace so the engine's default build and lockfile stay free of it. See
[`XIP.md`](XIP.md).

## Dropping a borrowed engine

"Buy don't build" is the default, not a law. Each borrowed engine was scored on
what it costs and what owning it would take — measured, not guessed — in
[`DEPENDENCY-EXIT.md`](DEPENDENCY-EXIT.md). The short version:

- **Own (subset):** the **CSS engine** — `stylo` is the single biggest lever
  (108 crates) and a UI-grade subset is a few thousand lines.
- **Own:** **layout** (`taffy`, tiny in crates, moderate in code).
- **Own part of:** **text** — line-breaking yes, shaping no.
- **Keep:** font shaping, GPU rasterization, windowing, TLS, image codecs — the
  buy-line.

## How we measure

- **Locked package count** (`grep -c '^\[\[package\]\]' Cargo.lock`) — reported
  on every release.
- **Normal-transitive deps per crate** (`cargo tree -e normal`) — the number
  that actually matters for a downstream.
- **Default-feature graph** — kept minimal; the "full" graph is opt-in.

The first two numbers are the scoreboard. They are cheap to read and easy to
regress, so they are read at every slice boundary.
