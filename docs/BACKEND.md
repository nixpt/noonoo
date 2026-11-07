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
| `FillRect` | `Scene::fill` a `kurbo::Rect` |
| `FillPolygon` | `Scene::fill` a closed `kurbo::BezPath` |
| `PushClip` / `PopClip` | `Scene::push_layer` / `pop_layer` |
| `DrawText` | *not drawn* — text is rendered as glyph outlines, so a bare run has no paths |

Depends on `vello` (`default-features = false` — no wgpu) plus the path
`noonoo-paint`/`noonoo-geometry`. **Tested headlessly** by asserting on
`Scene::encoding()` (paths and draws), including that a clip adds layer entries
and a text run adds none.

## The window pass — `noonoo-winit` (planned)

The remaining opt-in crate: a winit window with a wgpu surface, driving Vello's
renderer each frame, and converting platform input into `noonoo-events`.

**Versions to pin** (matching the ecosystem, see `arniko`/`mustang`):

| Crate | Version | Why |
|---|---|---|
| `vello` | `0.7` | matches mustang; renderer behind the `wgpu` feature |
| `wgpu` | `27` | vello 0.7 + mustang 0.3 both use it |
| `winit` | `=0.31.0-beta.2` | the fork the ecosystem pins |
| `anyrender` (opt.) | `0.7` | the `PaintScene` seam mustang consumes |

**Shape:**

1. `winit` `ApplicationHandler` → on `resumed`, create an `Arc<Window>`, a wgpu
   `Instance`/`Surface`/`Device`/`Queue`, and Vello's renderer.
2. Each frame: `noonoo_vello::scene(&layer.commands())` per host layer, compose
   them (offset by each layer's `rect`), and render to the surface.
3. Input: convert `WindowEvent` (`CursorMoved`, `MouseInput`, `MouseWheel`,
   `KeyboardInput`, `Resized`) into `noonoo-events`, then call the matching
   `Host` method — `hit`/`dispatch_pointer`/`dispatch_key`/`wheel` — using the
   window-size viewport. This reuses the host model unchanged.
4. Lifecycle: honor `resumed`/`suspended` for real (the old engine's `resumed`
   was a no-op stub — see `docs/SHELL.md` §5).

**Open risks** (why this is its own slice): the winit `0.31-beta` API is a moving
target and cannot be verified without a display; the wgpu/vello renderer entry
points in 0.7 differ from 0.6; and the crate is heavy to compile. It is a
*mechanical* integration once the API is pinned — the engine-facing seam is
already the `CommandList`, which the GPU pass consumes today.

## mustang — the effect compositor

[`mustang`](https://github.com/nixpt/mustang) (`arniko-mustang`, in
`surfer-workspace/mustang`) is a **thin GPU effect compositor**: it receives a
`vello::Scene`, applies blur / drop-shadow / transform / colour-adjust effects
within regions-of-interest, and submits through wgpu. Its boundary doctrine is
explicit — **no layout, no text shaping, no DOM, no HTML parsing** — so it never
overlaps Noonoo's concerns; it sits *after* the GPU pass.

Its region targeting lines up naturally with two Noonoo features:

- **`PushClip` regions** become effect regions (a blurred or shadowed sub-tree).
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
- **Window pass** (`noonoo-winit`): planned; the versions and shape are above.
- **Effects** (mustang): out of scope for now — it is a separate project with a
  clean seam (`vello::Scene`) to attach at.
