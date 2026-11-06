# Vaji — Architecture

## Principles

1. **Own the integration, not the engines.** A spec-complete CSS cascade, a
   flexbox/grid solver, a text shaper, and a GPU 2D rasterizer are solved
   problems and each is a multi-person-year effort. Vaji *buys* these and owns
   the seams between them. See [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md).
2. **One boundary per borrowed engine.** Each third-party engine is named in
   exactly one module. Its types do not leak. This is what makes the dependency
   surface *legible* — you can measure what a swap would cost because the
   contact points are counted in one file, not scattered across thirty.
3. **No `unsafe` in the engine.** `vaji-core`, `vaji-paint`, and `vaji-net` are
   `#![forbid(unsafe_code)]`. `unsafe` is confined to `vaji-shell`, where the
   windowing and GPU stacks require it.
4. **Headless is first-class.** The engine can be observed (tree dump), driven
   (synthetic input), and made deterministic (virtual clock) with no display.
   An engine an agent can verify is an engine that can be tested.
5. **Small, honest surfaces.** A parser that only knows the commands it can
   honour beats one full of stubs.

## Crate map

```
            ┌─────────────────────────────────────────────┐
            │                  vaji                        │  umbrella + CLI
            └───────┬───────────┬───────────┬─────────┬────┘
                    │           │           │         │
             ┌──────▼───┐ ┌─────▼────┐ ┌────▼─────┐   │
             │vaji-shell│ │vaji-paint│ │ vaji-net │   │
             └──────┬───┘ └─────┬────┘ └────┬─────┘   │
                    │           │           │         │
                    └───────────┴─────┬─────┴─────────┘
                                      │
                             ┌────────▼────────┐
                             │    vaji-core    │
                             └────────┬────────┘
                                      │  (the only crate reaching engines)
              ┌───────────────────────┼───────────────────────┐
              │            │          │          │            │
           ┌──▼──┐     ┌───▼───┐  ┌───▼───┐  ┌───▼───┐   ┌────▼────┐
           │ dom │     │ style │  │layout │  │ html  │   │ events  │
           └─────┘     └───┬───┘  └───┬───┘  └───────┘   └─────────┘
                          │          │
                       stylo    taffy + parley      (borrowed, one module each)
```

## Data flow

```
 bytes ──▶ html::parse ──▶ dom::Tree ──▶ style::cascade ──▶ layout::resolve ──▶ paint::* ──▶ backend
                                          (stylo)          (taffy+parley)      (commands)
```

Each arrow is a small, typed value crossing a module boundary. Nothing
downstream of `dom` knows where a node came from; nothing outside `style` knows
`stylo` exists; nothing outside `layout` knows `taffy` or `parley` exist.

## Invariants to keep true

- **Identity is defined once.** `dom::NodeId` is the only node handle in the
  workspace. If a second one appears, a boundary has leaked.
- **Borrowed engines stay behind their module.** `cargo tree` and a grep for
  `stylo`/`taffy`/`parley` should each land in one place per crate.
- **`forbid(unsafe_code)` stays on the engine crates.** If a slice appears to
  need `unsafe` in `vaji-core`, the design is wrong — say so before relaxing it.
- **The default build stays dependency-light.** New backend work lands behind a
  feature; it does not become a default.

## Clean-room discipline

- No file is copied from `bliss-engine` or Blitz. Licenses aside (standard
  MIT/Apache-2.0 boilerplate), everything is authored here.
- Design inspiration is taken from *behaviour*: ticket text, specs, and what a
  browser does — not from another implementation's source.
- When borrowing a concept (headless dumping, virtual clock, spring motion),
  it is re-derived from the behaviour it must exhibit and written fresh.
