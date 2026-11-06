# Noonoo — Multi-document windows & the shell

This note records what the engine Noonoo replaces got wrong about hosting more
than one document in a window, and the model Noonoo adopts instead. It is drawn
from the surfer-browser / arniko history (`RECONCILE-browser-engine-consolidation`,
`SURF-ENGINE-2`, `SURF-SUBDOC-1`, the `SHELL-SPLIT-*` arc, and the origin-gating
lesson), and it decides the shape of `noonoo-shell`, `noonoo-engine`, `noonoo-paint`,
and `noonoo-dom`.

## 1. What went wrong

The browser needed **one window showing two things at once**: trusted chrome
(the tab strip, toolbar, address bar) and untrusted page content. The engine
could not express that, because:

1. **One root document per view.** `bliss-shell`'s `View` hosted exactly one
   root `HtmlDocument`. A page could not be installed as a second root.
2. **Single-slot overlays.** Both overlay paths
   (`set_overlay_scene`, later `set_overlay_texture`) replaced *any* existing
   overlay — only one could exist.
3. **`Document` was `!Send`.** So a page could not be moved off the UI thread to
   be hosted as a document; it had to live on the UI thread or be rasterized
   elsewhere and blitted.

The workaround (`SHELL-SPLIT-0..3`, 2026-07-11) rendered each page to an
offscreen `wgpu::Texture` on a per-tab worker and blitted it over the chrome.
That fixed the *cost* problem (main-thread block 182 ms → 13.6 µs) but created a
capability problem, stated plainly in `SURF-ENGINE-2`:

> bliss-shell's `View` (scroll, hover, focus, IME, text editing) **never runs for
> pages** → no page scrolling, no hover/focus, no real `<input>` editing.

Chrome changes also rebuilt the whole shell document (`replace_document`), which
dropped focus and scroll for the *chrome*.

The intended destination (`SURF-SUBDOC-1`) is the opposite of the workaround:
**pages as first-class sub-documents**, so the shell's own machinery sizes,
paints, hit-tests, scrolls and forwards input to the page document — which needs
a `Send` document and real sub-document support.

## 2. The model Noonoo adopts

- **A window hosts many documents.** One chrome document plus zero-or-more
  content documents, each first-class. No "one root + one overlay slot."
- **Documents are `Send`.** Noonoo's tree owns no `Rc`/`RefCell` and promises
  nothing `unsafe`, so a document is `Send` whenever its payload is. Content
  documents can therefore be **worker-owned** — the property the old engine
  lacked and the reason the texture workaround existed.
- **Sub-documents are a primary capability, not a bolt-on.** A document can be
  attached to a host node in another document. The host node's box sizes the
  sub-document's viewport; the parent paints it inline, hit-tests it, forwards
  pointer/key/wheel/IME with offset coordinates and focus, and chains wheel
  scroll outward.
- **Composition is a seam, not a side effect.** Per-document render targets,
  overlay/blit, scene effects and **dirty regions** are first-class in
  `noonoo-paint` — so a host can decouple one document's GPU cost from another's
  frame without a fork.
- **Origin is bound to the document at creation, never derived from a URL.**
  A document carries a trust context; navigation is gated against it. (The old
  engine learned this the hard way: a URL-string check was escape-able; the
  document/JS context is the boundary content cannot cross.)

## 3. Ownership across the crates

| Concern | Crate | Shape |
|---|---|---|
| Host node that a sub-document attaches to | `noonoo-dom` | a node's optional sub-document slot |
| Document + trust context + sub-document orchestration | `noonoo-engine` | `Engine { dom, origin, subs }` |
| Size a sub-document viewport from the host node's box | `noonoo-layout` | layout of an embedded viewport |
| Paint a sub-document inline; render targets; overlays; dirty regions | `noonoo-paint` | `RenderTarget`, overlay/blit, `DirtyRegions` |
| Host N documents; input routing, focus, IME, lifecycle | `noonoo-shell` | `WindowHost { chrome, content }` |
| Navigation gated by a document's origin | `noonoo-net` | `navigation_allowed(origin, target)` |

## 4. Surface to add

### `noonoo-engine`
- **`Origin`** — a document's trust context: `Shell` (trusted chrome),
  `Content` (untrusted), `Custom(name)` (an app's own privileged scheme). Set at
  construction; readable, not URL-derived. Default for `Engine::new` is
  `Content` (fail-safe).
- **Sub-documents** — `Engine` holds host-node → child-`Engine` attachments;
  `attach_sub_document(host: NodeId, child: Engine)` and lookup. A sub-document
  is an `Engine`, so it carries its own origin and DOM.

### `noonoo-dom`
- A host node may carry a **sub-document marker** so layout/paint/queries can
  see that a node hosts an embedded document (the tree stays the single source
  of identity).

### `noonoo-layout`
- Given a host node with a sub-document, size the embedded viewport from the
  node's computed box and resolve it.

### `noonoo-paint`
- `RenderTarget` (per-document rasterization), a **blit/overlay** path, and a
  **dirty-region** channel so a host re-renders only what changed. Cap the
  overlay count at *many*, not one.

### `noonoo-shell`
- `WindowHost` owning the chrome document and a set of content documents, with
  input routed to the right document at the right offset/focus; **real**
  `resumed`/suspend lifecycle hooks and a public event conversion (the old
  `convert_events` was private and `resumed` was a no-op stub).

### `noonoo-net`
- `navigation_allowed(origin, target) -> bool`, consulted before following a
  navigation, so an internal command scheme is reachable only from `Shell`.

## 5. Lessons to encode (bugs that cost real time)

| Lesson | Evidence | Noonoo's stance |
|---|---|---|
| `resumed()` was a no-op → device published from the wrong hook | `app/event_handling.rs` | real lifecycle hooks in `noonoo-shell` |
| `watch::send` silently drops the value with zero receivers; needs `send_replace` | `app/cache.rs` | host-side, but documented in the shell |
| `register_texture` needs `COPY_SRC` **and** `STORAGE_BINDING` | `app/worker/paint.rs` | encoded + tested in `noonoo-paint` |
| Single-slot overlays forced a scene-merge hack | `set_overlay_scene` docs | N overlays |
| `replace_document` per chrome keystroke dropped focus/scroll | `RENDERING_PERFORMANCE_PLAN.md` | retained docs + mutation vocabulary (`noonoo-dom::mutate`) |
| Origin check bolted on after a real escape | SURF-ORIGIN-1 | origin bound to the document at creation |
| Bubbling absent in the old shell; `UiEvent` routed into a dead map | `event_routing.rs` | `noonoo-events` bubbles; `noonoo-shell` routes to the owning document |

## 6. Roadmap impact

The single "Shell" slice becomes three, in this order (see
[`ROADMAP.md`](ROADMAP.md)):

1. **Composition** — `noonoo-paint` render targets, overlays, dirty regions.
2. **Multi-document shell** — `WindowHost` with chrome + content documents,
   input routing, focus, real lifecycle.
3. **Sub-documents** — attach a `Send` document to a host node and have the
   parent size, paint, hit-test, scroll and forward input to it.

Sub-documents can only be built once (a) documents are `Send` (already true) and
(b) the composition seam exists — which is exactly the dependency order the old
engine got backwards.
