# Vaji

**A clean-room, modular HTML/CSS rendering engine.**

*Vaji* is shortened from *Vajigan*; the name traces to Sanskrit *vājin* (वाजिन्) —
"swift", "winged horse" — a fast courier between markup and pixels.

Vaji turns markup and stylesheets into laid-out, paintable form. It is a
from-scratch reimplementation of the ideas in `bliss-engine` (itself a fork of
upstream Blitz), decomposed into small crates and built to be the **substrate**
that a UI framework (arniko) and a browser (surfer) build on.

> **Clean room.** No code here is copied from `bliss-engine`, Blitz, or any other
> rendering engine. Vaji is written from behaviour and specification. The
> third-party engines it depends on (Servo's *stylo*, *taffy*, *parley*,
> *anyrender/vello*, *winit*) are ordinary crates.io dependencies, pinned in one
> place and reached through one crate each — never vendored, never forked.

## Crates

The engine replaces one ~19k-LOC `bliss-dom` monolith with small, single-purpose
crates. Depend on the one you need; reach for `vaji` only when you want all of it.

| Crate | Responsibility |
| --- | --- |
| [`vaji-dom`](crates/vaji-dom) | The tree, stable `NodeId`s, checked mutation, queries |
| [`vaji-events`](crates/vaji-events) | Input model and DOM dispatch with bubbling |
| [`vaji-text`](crates/vaji-text) | Text shaping, fonts, line breaking |
| [`vaji-motion`](crates/vaji-motion) | Springs, velocity-preserving retarget |
| [`vaji-style`](crates/vaji-style) | Stylesheet ingestion and the cascade |
| [`vaji-layout`](crates/vaji-layout) | Box layout: computed style → boxes |
| [`vaji-html`](crates/vaji-html) | Markup ingestion into the tree |
| [`vaji-net`](crates/vaji-net) | Document loading: `http(s)`/`file`/`data`/`exo` |
| [`vaji-engine`](crates/vaji-engine) | Orchestration: a working document |
| [`vaji-headless`](crates/vaji-headless) | Tree dump, virtual clock, synthetic input |
| [`vaji-paint`](crates/vaji-paint) | Layout → render commands, scene effects, overlays |
| [`vaji-shell`](crates/vaji-shell) | Window, event loop, a11y, GPU surface (feature-gated) |
| [`vaji`](crates/vaji) | Umbrella re-export + the `vaji` command line |

## Why it exists

Two consumers — **arniko** (a reactive UI framework) and **surfer-browser** —
build on the engine Vaji replaces, and both carry workarounds for it. Vaji is
built to be what they'd rather build on: a decomposed substrate with correct
events, stable node ids, checked mutation, a working `set_inner_html`, real
accessibility, and first-class sub-documents and headless tooling. See
[`docs/CONSUMERS.md`](docs/CONSUMERS.md) and
[`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md).

## Status

**Pre-alpha, scaffolding.** The workspace compiles with **zero third-party
dependencies**; the engine backends are declared as features and wired in later
slices. See [`docs/ROADMAP.md`](docs/ROADMAP.md) and
[`ARCHITECTURE.md`](ARCHITECTURE.md).

## Building

```sh
cargo build
cargo test --workspace
cargo run -p vaji -- --version
```

The workspace MSRV is **1.89**. CI checks it explicitly.

## Docs

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — crate map, data flow, invariants
- [`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md) — the dependency policy
- [`docs/CONSUMERS.md`](docs/CONSUMERS.md) — arniko & surfer: what they use, what they fight
- [`docs/FEATURE-OWNERSHIP.md`](docs/FEATURE-OWNERSHIP.md) — what belongs in Vaji vs. the consumer
- [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md) — the drop-in switch and the api-drift gate
- [`docs/ROADMAP.md`](docs/ROADMAP.md) — slices

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
