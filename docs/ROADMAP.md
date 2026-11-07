# Noonoo — Roadmap

Slices are ordered so each ends in something observable and testable. Nothing
lands behind a stub that pretends to work. The compatibility track
([`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md)) runs alongside: the reference
API snapshots are captured early, and the port is measured against them.

## Slice 0 — Scaffold *(this slice)*

A workspace of small crates, decomposed along the boundaries in
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

Own the layers outright rather than wire them: `noonoo-text` owns line breaking
(`Shaper` seam for metrics), `noonoo-layout` consumes `noonoo-style` + `noonoo-text`.
*Done — and the "wire parley/taffy" framing above was superseded: both engines
were replaced (see Phase B/C below).*

**Done when:** a fixture yields expected box rectangles and line boxes, asserted
numerically. ✅

## Slice 5 — Document + events

`noonoo-engine` ties parse→style→layout into a working `Engine`;
`noonoo-events` dispatches with bubbling into a `Document`.

**Done when:** a click on a nested node reaches an ancestor handler.

## Slice 6 — Paint

`noonoo-paint` turns layout into commands; a recording backend makes them
diff-able; scene effects + dirty regions are exposed.

**Done when:** a fixture's command stream matches a golden file.

## Slice 7 — Shell  ✅ shipped

The **multi-document host** (`noonoo-shell`): first-class layers (chrome +
content), region routing, hit-testing, focus, per-layer composition, and origin
gating — all headless-testable, zero deps.

The **accessibility tree** is owned (`noonoo-a11y`) and on by default. It
projects roles, accessible names/descriptions, values, states, heading levels,
and bounds from the DOM, computed style, and layout; `Host::a11y` composes every
layer under one window root with offset bounds and unique ids. The opt-in
`integrations/noonoo-accesskit` projects that tree onto AccessKit, and the
**window pass** (`integrations/noonoo-winit`) creates an AT-SPI adapter on
Linux, pushes updates after every layout, forwards focus/click actions back to
the host on the event-loop thread, and falls back cleanly when no session bus
exists. A consumer never re-enables a no-op Linux path.

The **borrowed backend** (a feature-gated window via `winit` + `wgpu`/`vello`)
is the other half of this slice, landed in the window pass.

**Done when:** `cargo run -p noonoo --features window,gpu` opens a window and the
a11y tree reaches AT-SPI on Linux without consumer re-enabling. *(Met; runtime
AT-SPI delivery still needs a desktop session to observe.)*

## Slice 8 — Sub-documents  ◑ anchored layers + scroll chaining shipped

Host a document inside another as a first-class sub-document. Realized as an
**anchored layer**: `Host::add_sub_document(parent, host_node, engine)` attaches
a `Send` document to a host node; the parent **sizes** it from that node's
content box, and routing, hit-testing, and input reach it through its own
document — no blitted bitmap.

**Wheel-scroll chaining** is in: `Host::wheel(point, delta)` scrolls the layer
under the pointer and passes leftover to the parent when it hits its edge.

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
  *(A1 selectors ✅, A2 value/property model ✅, A3 cascade ✅ — done; `stylo` was never added)*
- **Phase B — layout (drops `taffy`).** block + inline + flex over our values.
  *✅ (v1: block, inline, flex — row/column, grow/shrink, justify, gaps;
  grid — tracks, `fr`, `repeat()`, placement + auto-placement, implicit
  tracks, gaps; `position`; floats + `clear`; vertical margin collapsing;
  overflow clipping, two-axis scroll containers, and `z-index` stacking order)*
- **Phase C — text (trims `parley`).** line-breaking + `text-align`; shaping
  stays borrowed.
  *✅ (line breaking, wrapping, and alignment owned; real font metrics via
  `noonoo-ttf`; shaping behind the `Shaper` seam)*
- **Phase D — paint.** the command stream + a recording backend (keep the GPU).
  *✅ (backend-neutral command stream + JSON recording; GPU backend deferred)*
- **Phase E — net / SVG.** thin HTTP client; SVG subset.
  *✅ (owned `file:`/`data:` loaders + seam; `http`/TLS behind a feature; SVG subset → paint commands)*

## Later

SVG, networking capabilities (`exo://`), rendering capsules, and the remaining
ARIA/HTML edge cases in the a11y tree.
Each enters behind its feature and its own crate boundary.
