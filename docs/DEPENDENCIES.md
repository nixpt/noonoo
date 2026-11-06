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
| `stylo` (+ `selectors`, `cssparser`, `style_*`) | `noonoo-core::style` | **Keep.** The CSS cascade is Servo's crown jewel; re-deriving it is out of scope. |
| `taffy` | `noonoo-core::layout` | **Keep.** Flexbox + grid, a solved problem. |
| `parley` (+ `fontique`/`swash`/`skrifa`) | `noonoo-core::layout` | **Keep.** Shaping, line breaking, font management. |
| `anyrender` / `vello` / `wgpu` | `noonoo-paint`, `noonoo-shell` | **Keep.** GPU 2D, behind a feature. |
| `winit`, `accesskit` | `noonoo-shell` | **Keep.** Windowing + a11y, behind features. |

## The rules that keep the surface thin

1. **One module per borrowed engine.** `stylo` is named only in
   `noonoo-core::style`; `taffy`/`parley` only in `noonoo-core::layout`. The contact
   points are countable in one file, so "what would a swap cost?" has an answer.
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

One class of dependency Noonoo *does* write itself: **format parsers**. The four
parser crates — `noonoo-parse`, `noonoo-json`, `noonoo-css`, `noonoo-caison`,
`noonoo-html` — are dependency-free and exist because a parser is a *syntax*
layer: small, spec-defined, and load-bearing for the engine's own formats.

- **JSON** (`noonoo-json`) is needed to *project* other formats into a
  machine-readable shape (CAISON does this), so owning it removes a ubiquitous
  incidental dependency.
- **CAISON** (`noonoo-caison`) is the ecosystem's AI-native config format; a
  parser for it belongs beside the tree it feeds.
- **HTML** (`noonoo-html`) parses into `noonoo-dom` directly — parsing must never be
  a silent no-op (the replaced engine let consumers forget to install a parser
  provider).
- **CSS** (`noonoo-css`) owns the *syntax* (CSS Syntax Level 3 tokens + a rule
  splitter). The **cascade** — selector matching and computed values — stays
  borrowed (`stylo`), because that is the genuinely huge part.

The line is deliberate: we own the small, spec-shaped syntax; we buy the
multi-year engines.

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
