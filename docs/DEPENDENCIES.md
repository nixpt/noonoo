# Vaji — Dependency Policy

The engine it replaces locks **634 packages** (with `bliss-dom` alone pulling
**192** normal-transitive crates). Vaji's dependency story is a designed
outcome, not a byproduct.

## The rule: buy the engines, own the seams

A rendering engine is a browser's worth of solved problems. Re-deriving CSS,
flexbox/grid, text shaping, or GPU 2D is not where the value is. We **buy**
them and **own** the integration.

| Engine | Reached from | Verdict |
| --- | --- | --- |
| `stylo` (+ `selectors`, `cssparser`, `style_*`) | `vaji-core::style` | **Keep.** The CSS cascade is Servo's crown jewel; re-deriving it is out of scope. |
| `taffy` | `vaji-core::layout` | **Keep.** Flexbox + grid, a solved problem. |
| `parley` (+ `fontique`/`swash`/`skrifa`) | `vaji-core::layout` | **Keep.** Shaping, line breaking, font management. |
| `anyrender` / `vello` / `wgpu` | `vaji-paint`, `vaji-shell` | **Keep.** GPU 2D, behind a feature. |
| `winit`, `accesskit` | `vaji-shell` | **Keep.** Windowing + a11y, behind features. |

## The rules that keep the surface thin

1. **One module per borrowed engine.** `stylo` is named only in
   `vaji-core::style`; `taffy`/`parley` only in `vaji-core::layout`. The contact
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

A non-exhaustive list of things the previous engine carried that Vaji will not,
because the standard library or an already-present dependency covers them:

- `html-escape`, `percent-encoding` — subsumed by URL/string handling.
- `atomic_refcell`, `thread_local`, `fastrand`, `downcast-rs`, `slab` — the
  arena and ordinary std types cover these.
- `wuff` — font loading is `fontique`'s job.
- Vendored forks of `accesskit_xplat` / `anyrender_vello` — prefer a boundary
  shim over a published clone.
- Alpha-channel HTTP caching middleware as a hard default — opt-in at most.

## How we measure

- **Locked package count** (`grep -c '^\[\[package\]\]' Cargo.lock`) — reported
  on every release.
- **Normal-transitive deps per crate** (`cargo tree -e normal`) — the number
  that actually matters for a downstream.
- **Default-feature graph** — kept minimal; the "full" graph is opt-in.

The first two numbers are the scoreboard. They are cheap to read and easy to
regress, so they are read at every slice boundary.
