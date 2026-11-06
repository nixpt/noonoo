# Vaji

**A clean-room, modular HTML/CSS rendering engine.**

*Vaji* is shortened from *Vajigan*; the name traces to Sanskrit *vājin* (वाजिन्) —
"swift", "winged horse" — a fast courier between markup and pixels.

Vaji turns markup and stylesheets into laid-out, paintable form. It is a
from-scratch reimplementation of the ideas in `bliss-engine` (itself a fork of
upstream Blitz), written to a smaller, more honest set of boundaries and a
deliberately thin dependency surface.

> **Clean room.** No code in this repository is copied from `bliss-engine`,
> Blitz, or any other rendering engine. Vaji is written from behaviour and
> specification. The third-party engines it depends on (Servo's *stylo*,
> *taffy*, *parley*, *anyrender/vello*) are ordinary crates.io dependencies,
> pinned in one place and reached through one module each — never vendored,
> never forked, never re-derived.

## Crates

| Crate | Responsibility |
| --- | --- |
| [`vaji-core`](crates/vaji-core) | The document tree, style resolution, the layout pipeline, events, headless tooling |
| [`vaji-paint`](crates/vaji-paint) | Layout boxes → a backend-neutral render command stream |
| [`vaji-shell`](crates/vaji-shell) | Native windowing, event loop, interactive driver (feature-gated) |
| [`vaji-net`](crates/vaji-net) | Document loading: `http(s)`, `file`, `exo` |
| [`vaji`](crates/vaji) | Umbrella re-export + the `vaji` command line |

Five crates, one direction of dependency: everything points at `vaji-core`, and
`vaji-core` points at nobody.

## Status

**Pre-alpha, scaffolding.** The workspace compiles with **zero third-party
dependencies** today; the engine backends are declared as features and wired in
later slices. See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the plan and
[`ARCHITECTURE.md`](ARCHITECTURE.md) for the design and invariants.

## Building

```sh
cargo build
cargo test --workspace
cargo run -p vaji -- --version
```

The workspace MSRV is **1.89** (it will follow the style/layout stack once that
lands); CI checks it explicitly.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option.
