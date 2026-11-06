# Noonoo — Roadmap

Slices are ordered so each ends in something observable and testable. Nothing
lands behind a stub that pretends to work. The compatibility track
([`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md)) runs alongside: the reference
API snapshots are captured early, and the port is measured against them.

## Slice 0 — Scaffold *(this slice)*

Seventeen-crate workspace, decomposed along the boundaries in
[`ARCHITECTURE.md`](ARCHITECTURE.md), design docs, CI, and a compiling,
dependency-free skeleton. Working primitives with tests: the `noonoo-dom` arena +
checked mutation, `noonoo-events` dispatch path, `noonoo-motion` spring, `noonoo-paint`
command list, `noonoo-engine` (the `Engine` type).

**Done when:** `cargo check/test/clippy` pass with an empty dependency graph;
`noonoo --version` works.

## Slice 1 — Compatibility baseline

Capture `api-drift` reference snapshots of the released engine for the
consumer-visible subset (`bliss-traits`, `bliss-dom`, `bliss-html`, `bliss-paint`,
`bliss-shell`, `bliss-net`), under `api-drift/bliss/`. Stand up the
`tools/api-snapshot` helper and the diff gate.

**Done when:** the snapshots are committed and `api-drift check` passes; the
diff pipeline runs against a Noonoo shim (even an empty one).

## Slice 2 — Ingest + tree dump  ✅ shipped

`noonoo-html` parses into `noonoo-dom`; `noonoo-engine::from_html` collects and
parses `<style>` blocks with `noonoo-css`; `noonoo-headless` dumps the tree as
JSON (built on `noonoo-json`). The CLI is the end-to-end path:
`noonoo --tree <file|->`.

**Done when:** `noonoo --tree fixture.html` prints JSON and a test asserts on the
structure with no screenshot. *(met)*

## Slice 3 — Style

Wire `stylo` behind `noonoo-style`: parse a stylesheet, cascade, expose Noonoo-shaped
computed values. Incremental restyle is the default.

**Done when:** a fixture's computed `color`/`display` are asserted from a test,
and a second edit restyles incrementally.

## Slice 4 — Text + layout

Wire `parley` behind `noonoo-text`; wire `taffy` behind `noonoo-layout`, consuming
`noonoo-style` + `noonoo-text`.

**Done when:** a fixture yields expected box rectangles and line boxes, asserted
numerically.

## Slice 5 — Document + events

`noonoo-engine` ties parse→style→layout into a working `Engine`;
`noonoo-events` dispatches with bubbling into a `Document`.

**Done when:** a click on a nested node reaches an ancestor handler.

## Slice 6 — Paint

`noonoo-paint` turns layout into commands; a recording backend makes them
diff-able; scene effects + dirty regions are exposed.

**Done when:** a fixture's command stream matches a golden file.

## Slice 7 — Shell

Feature-gated window (`winit` + `wgpu`/`vello`); accessibility on by default;
real `resumed`/suspend hooks. Headless stays the default path.

**Done when:** `cargo run -p noonoo --features window,gpu` opens a window and the
a11y tree reaches AT-SPI on Linux without consumer re-enabling.

## Slice 8 — Sub-documents

Host a `Document` inside another as a first-class sub-document, worker-ownable
because the tree is `Send`. Replace the texture-overlay pattern.

**Done when:** an embedded document scrolls, chains wheel scroll to its parent,
and receives input through the document's own paths.

## Slice 9 — Motion

Spring transitions with velocity-preserving retarget, wired to style animations.

**Done when:** an interrupted animation does not jump.

## Slice 10 — Compat shims + the switch

Publish the `compat/` shim crates (lib names matching the `bliss-*` crates),
re-exporting Noonoo. Prove the switch on a consumer fork behind a transient
`[patch.crates-io]`.

**Done when:** a consumer compiles against Noonoo shims with a manifest-only
change and the `api-drift` diff for the consumer-visible subset shows no
unrecorded `Breaking` item.

## Track — owning the engines (see [`DEPENDENCY-EXIT.md`](DEPENDENCY-EXIT.md))

The borrowed engines are scored in `DEPENDENCY-EXIT.md`. The path to dropping
`stylo` and friends, in order:

- **Phase A — CSS engine (drops `stylo`, ~108 crates).** `noonoo-css` grows a
  selector model + specificity and a property/value model; `noonoo-style` owns
  the cascade, inheritance, computed values, and a UA stylesheet. Done when our
  engine computes `color`/`display`/`margin` for a fixture with stylo absent.
- **Phase B — layout (drops `taffy`).** block + inline + flex over our values.
- **Phase C — text (trims `parley`).** line-breaking + `text-align`; shaping
  stays borrowed.
- **Phase D — paint.** the command stream + a recording backend (keep the GPU).
- **Phase E — net / SVG.** thin HTTP client; SVG subset.

## Later

SVG, networking capabilities (`exo://`), rendering capsules, a11y-complete tree.
Each enters behind its feature and its own crate boundary.
