# Noonoo — Backends (GPU & window)

Noonoo owns the **paint command stream** (`noonoo-paint`) end to end. Turning that
stream into pixels is the *borrowed* layer, and it is split into two opt-in
passes that live **outside the workspace** (root `exclude`), so the engine's
default build and lockfile never carry a GPU or a window.

```
CommandList ──▶ noonoo-vello ──▶ vello::Scene ──▶ [mustang effects] ──▶ winit + wgpu surface
                  (GPU pass)                         (optional)            (window pass)
```

## The GPU pass — `noonoo-vello` ✅

`integrations/noonoo-vello`. Translates a `noonoo_paint::CommandList` into a
`vello::Scene`, ready for Vello's wgpu renderer *and* for mustang, which consumes
a `vello::Scene`.

| Noonoo command | Vello |
|---|---|
| `FillRect` | `Scene::fill` a `kurbo::Rect` (rounded corners become a Bézier path) |
| `FillGradient` | `Scene::fill` a `kurbo::Rect` with a `peniko::Gradient` (`Extend::Repeat` for repeating; elliptical radial gradients via a brush transform) |
| `FillPolygon` | `Scene::fill` a closed `kurbo::BezPath` |
| `PushLayer` / `PopLayer` | `Scene::push_layer` (clip + alpha) / `pop_layer` |
| `DrawText` | *not drawn* — text is rendered as glyph outlines, so a bare run has no paths |

Depends on `vello` (`default-features = false` — no wgpu) plus the path
`noonoo-paint`/`noonoo-geometry`. **Tested headlessly** by asserting on
`Scene::encoding()` (paths and draws), including that a clip adds layer entries
and a text run adds none.

## The window pass — `noonoo-winit` ✅

`integrations/noonoo-winit`. A winit window with a wgpu surface, driving Vello's
renderer each frame, and converting platform input into `noonoo-events`.

**Versions to pin** (matching the ecosystem, see `arniko`/`mustang`):

| Crate | Version | Why |
|---|---|---|
| `vello` | `0.7` | matches mustang; renderer behind the `wgpu` feature |
| `wgpu` | `27` | vello 0.7 + mustang 0.3 both use it |
| `winit` | `=0.31.0-beta.2` | the fork the ecosystem pins |
| `accesskit`, `accesskit_unix` | `0.24`, `0.20` | Linux AT-SPI adapter (target-gated; no session bus required to build) |
| `anyrender` (opt.) | `0.7` | the `PaintScene` seam mustang consumes |

**Shape (as landed):**

1. **`input`** — a pure translator from `WindowEvent` to an `Action` the host
   performs (`Pointer`/`Key`/`Wheel`/`Resized`/`Redraw`/`Close`), tested
   headlessly against winit's own types. Coordinates divide by the scale factor
   (Noonoo works in logical pixels); wheel deltas flip sign (winit's positive
   delta means content moves down, `Host::wheel`'s positive delta scrolls
   forward); `LineDelta` lines convert at `LINE_HEIGHT` (40 logical px); named
   keys use `NamedKey`'s spec names, characters their text; dead/unidentified
   keys are skipped. Touch buttons are not dispatched.
2. **`render::Renderer`** — wgpu `Instance`/`Surface`/`Device`/`Queue` +
   Vello's renderer. Each frame composes one `noonoo_vello` scene per host
   layer, offset by the layer's `rect` and clipped to it. The surface is
   created from the window's raw handles (`SurfaceTargetUnsafe::from_window`)
   so it can be `'static` while the window keeps ownership — the one scoped
   `unsafe`, documented, with the renderer ordered to drop before the window.
3. **`App`** — winit's `ApplicationHandler`: window + surface in
   `can_create_surfaces` (recreated after `suspended`), `SurfaceResized` →
   `host.viewport` + a host-supplied resize hook, pointer *buttons* set
   keyboard focus and synthesize a `Click` when the release lands on the press
   target, pointer *moves* route to the DOM without stealing focus, wheel
   chains through `Host::wheel`, and rendering is redraw-driven with a dirty
   flag.
4. **`a11y`** (Linux) — an `accesskit_unix::Adapter` wired to `Host::a11y`.
   AccessKit's handlers run on another thread, so they only send messages and
   wake the loop; the event loop services them in `proxy_wake_up`/
   `about_to_wait`: a tree request pushes a full update, and `Click`/`Focus`
   actions are mapped back to `(layer, node)` and dispatched through the host.
   Window bounds and native-focus state stay in sync. With no session bus the
   adapter's thread has nothing to connect to and the window runs unchanged.

**Verification**: the input seam is unit-tested without a display (8 tests over
winit's real event types) and the a11y bridge is unit-tested for tree retention
and forwarding (1 test); the whole crate — handler, renderer, bridge, and a
runnable `examples/showcase.rs` — compiles against the pinned
`winit =0.31.0-beta.2`, `wgpu 27`, and `vello 0.7`. Presenting frames and
observing the AT-SPI tree need a desktop session and remain runtime-verified
only.

## mustang — the effect compositor

[`mustang`](https://github.com/nixpt/mustang) (`arniko-mustang`, in
`surfer-workspace/mustang`) is a **thin GPU effect compositor**: it receives a
`vello::Scene`, applies blur / drop-shadow / transform / colour-adjust effects
within regions-of-interest, and submits through wgpu. Its boundary doctrine is
explicit — **no layout, no text shaping, no DOM, no HTML parsing** — so it never
overlaps Noonoo's concerns; it sits *after* the GPU pass.

Its region targeting lines up naturally with two Noonoo features:

- **`PushLayer` regions** become effect regions (a blurred or shadowed sub-tree).
- **XIP capability gating** (`noonoo-caps`) is a policy layer over the same
  regions — "hardware-level region gating for isolated components" is the same
  shape as a capability-gated DOM segment.

Integration is a future opt-in crate (`noonoo-mustang` or a feature on the window
pass) that wraps the `vello::Scene` in a `MustangSceneBundle` with the effects
derived from the CSS synthetic features (`backdrop-filter`, `filter`, `transform`).

## Why outside the workspace

`docs/DEPENDENCIES.md` rule 3: *a build that only needs the engine pays for
nothing else; default features must never pull a GPU, a window, or a screen
reader.* Feature-gating a borrowed stack still puts it in the lockfile, so the
backends are **excluded** (like `integrations/noonoo-xip` and
`tools/api-snapshot`) and each carries its own lock. The engine lockfile stays
`noonoo-*` only.

## Status

- **GPU pass** (`noonoo-vello`): landed, headless-tested.
- **Window pass** (`noonoo-winit`): landed; input seam and a11y bridge
  headless-tested, frame presentation and AT-SPI delivery runtime-verified only
  (no display in CI).
- **Effects** (mustang): out of scope for now — it is a separate project with a
  clean seam (`vello::Scene`) to attach at.
