# Vaji — Roadmap

Slices are ordered so that each one ends in something observable and testable.
Nothing lands behind a stub that pretends to work.

## Slice 0 — Scaffold *(this slice)*

Workspace, crate split, design docs, CI, and a compiling, dependency-free
skeleton. The DOM arena is the one working primitive, with tests.

**Done when:** `cargo check/test/clippy` pass with an empty dependency graph;
the `vaji` CLI reports its version.

## Slice 1 — Ingest + tree dump

HTML parsing into `dom::Tree` (behind the `html` boundary), and the headless
tree dump as JSON. First observable artifact.

**Done when:** `vaji --tree fixture.html` prints the parsed tree as JSON, and a
test asserts on the structure without a screenshot.

## Slice 2 — Style

Wire `stylo` behind `vaji-core::style`: parse a stylesheet, cascade against the
tree, expose Vaji-shaped computed values.

**Done when:** a fixture's computed `color`/`display` are asserted from a test.

## Slice 3 — Layout

Wire `taffy` and `parley` behind `vaji-core::layout`: boxes and geometry, text
shaped and broken into lines.

**Done when:** a fixture yields expected box rectangles, asserted numerically.

## Slice 4 — Paint

`vaji-paint` turns the layout output into render commands; a recording backend
makes them diff-able.

**Done when:** a fixture's command stream matches a golden file.

## Slice 5 — Shell

Feature-gated window: `winit` event loop + `wgpu`/`vello` surface. Headless is
still the default path.

**Done when:** `cargo run -p vaji --features window,gpu` opens a window.

## Slice 6 — Input + events

The event surface, fed identically by a real device and by synthetic input.
Virtual clock for deterministic frames.

**Done when:** a scripted click produces the same result as a recorded one.

## Slice 7 — Motion

Spring timing with velocity-preserving retarget for transitions. Owned, not
borrowed.

**Done when:** an interrupted animation does not jump.

## Later

Accessibility tree, SVG, sub-documents, networking, networking capabilities.
Each enters behind its feature and its own module boundary.
