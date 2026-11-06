# Vaji — Consumers

This documents who consumes the engine Vaji replaces, what they touch, and what
that means for Vaji's surface. **Scope:** the two consumers Vaji is being built
*for* — **surfer-browser** and **arniko**. Exosphere and the rest of the fleet
are **out of scope here** and carry their own tickets (see the note at the end).

Canonical engine today: `nixpt/bliss-engine` (`/workspace/projects/libs/bliss-engine`),
published to crates.io as `bliss-*` **0.3.x** (umbrella package `arniko-bliss`).
Both consumers pin `^0.3`, resolved to **0.3.1**, and neither patches — a clean
registry dependency.

---

## 1. surfer-browser — the flagship consumer

Root: `/workspace/projects/surfer-browser`. Members: `crates/surfer`,
`crates/surfer-page`, `crates/super-surfer`, `crates/surfer/surfer-core`.

### 1.1 Dependency & feature map

| Crate | bliss crates | optional? | features |
|---|---|---|---|
| `surfer-page` | `bliss-dom`, `bliss-html` | **no** | — |
| | `bliss-net`, `bliss-traits`, `bliss-paint`, `bliss-anyrender-vello` | yes (`bliss`) | — |
| `surfer` | `bliss-dom`, `bliss-html` | **no** | — |
| | `bliss`, `bliss-net`, `bliss-traits`, `bliss-paint`, `bliss-anyrender-vello` | yes (`bliss`) | `bliss`→`net` |
| | `bliss-shell` | yes (`bliss`) | `clipboard`, `file_dialog` |
| `super-surfer` | all of the above + `bliss-accesskit-xplat` | yes (`bliss`) | `bliss-shell`: `accessibility`; `bliss-accesskit-xplat`: `accesskit_unix`, `async-io` |
| `surfer-core` | — | — | `bliss` is an empty marker |

`bliss-dom` and `bliss-html` are **non-optional** in the two engine crates:
bliss-dom is surfer's *canonical DOM* (SURF-ENG-1 replaced "obscura-dom");
`bliss-html` supplies `HtmlDocument` + the parser provider.

Notable: the umbrella `bliss`/`arniko-bliss` crate and `bliss-accesskit-xplat`
are **never imported in source** — they exist only as Cargo feature glue.

### 1.2 API surface actually used

**`bliss-dom`** (deepest dependency):
- Types: `Document` (trait), `BaseDocument`, `HtmlDocument` (via `bliss-html`),
  `Node` / `NodeData` (`Text`, `Element`, `AnonymousBlock`, `Comment`,
  `Document`, `ShadowRoot`), `DocumentConfig`, `LocalName`, `Namespace`,
  `QualName`, `Attribute`, `ns!`, `local_name!`.
- `Document` methods: `resolve(0.0)`, `poll(None)`, `handle_ui_event`,
  `set_navigation_provider(Arc<dyn NavigationProvider>)`, `snapshot_node`,
  `query_selector` / `_all` / `_all_from`, `get_element_by_id`, `get_node[_mut]`,
  `create_text_node`, `root_element`, `find_title_node`, `tree`, `print_tree`,
  `downcast_mut::<HtmlDocument>()`.
- `Node`: `text_content`, `element_data`, `attr`, `attrs`, `is_element`,
  `outer_html`, `absolute_position`, `final_layout.{size,location}`,
  `children`, `parent`, `data`, `id`.
- `DocumentMutator` (via `doc.mutate()`): `set_attribute`, `clear_attribute`,
  `remove_and_drop_all_children`, `create_text_node`, `create_element`,
  `append_children`, `set_inner_html`, `child_ids`, `remove_node`,
  `deep_clone_node`, `set_node_text`, `insert_nodes_before`, `insert_nodes_after`.
- `BaseDocument::upsert_stylesheet_for_node`.

**`bliss-html`**: `HtmlDocument::from_html`, `HtmlProvider`.

**`bliss-net`**: `Provider::shared(Some(waker))`.

**`bliss-traits`**:
- `shell::{Viewport, ColorScheme}` — `Viewport::new`, `.window_size`, `.scale[_f64]`.
- `net::{NetWaker, NetProvider, Method, Body, FormData}`.
- `navigation::{NavigationOptions, NavigationProvider}`.
- `events::{UiEvent, BlissKeyEvent, BlissPointerEvent, BlissWheelEvent,
  BlissWheelDelta, KeyState, MouseEventButton(s), PointerCoords, PointerDetails}`.
- `dom_control::DomController` — `query_selector`, `set_style_property`,
  `set_attribute`.

**`bliss-paint`**: `paint_scene(painter, doc, scale, w, h, x, y)`.

**`bliss-shell`**: `BlissApplication`, `BlissShellEvent`, `BlissShellProxy`,
`WindowConfig`, `create_default_event_loop`, `View<Renderer>`
(`set_overlay_texture`, `request_redraw`).

**`bliss-anyrender-vello`**: `VelloWindowRenderer`, `VelloImageRenderer`,
`VelloScenePainter`, plus re-exported `vello`, `wgpu`, `peniko`, `kurbo`.

### 1.3 What surfer builds on top

- **`surfer-page`** — the page engine and the only crate that owns a DOM + render
  engine. `SurferRenderEngine` wraps one `Option<HtmlDocument>` and drives
  `resolve()` + `bliss_paint::paint_scene` (to an anyrender buffer or a vello
  scene); exposes a large read/mutate surface and a `RenderBackend` seam
  (headless vs visual). The Boa JS runtime bridges to the DOM through a
  thread-local `RefCell<Option<Dom>>` and a 35-arm `op_dom_inner` dispatcher;
  reads use `bliss_dom::Node`, writes go through `DocumentMutator`.
- **`surfer`** — engine/CLI/devtools/Crush runtime. `SurferPage` (headless page
  lifecycle), a devtools `NodeRegistry` that maps volatile bliss indices to
  stable CDP ids, a Crush `dom.*` host surface, and a headless `browse` path that
  runs without the `bliss` feature at all.
- **`super-surfer`** — the desktop GUI. A shell chrome document rendered from
  templates, each tab on its **own worker thread** rendering page content to an
  offscreen `wgpu::Texture`, blitted into the shell via
  `View::set_overlay_texture` (the *texture-overlay* design). Input is converted
  from winit into `bliss_traits::events::UiEvent` and routed with
  `Document::handle_ui_event`. Navigation is split into shell vs page providers
  with origin gating.

### 1.4 Friction & wants (the interesting part)

| # | Friction | Where | Vaji's answer |
|---|---|---|---|
| 1 | `DomController::set_inner_html` **appends**, doesn't replace → consumer wrote `replace_inner_html` | `super-surfer/src/app/shell/layout.rs:1173-1184` | replace semantics, guaranteed |
| 2 | `downcast_mut` always failed (resolved on the outer `Box`) | `shell/layout.rs:152-181` | no `Box<dyn Document>`-downcast dance; type-safe handles |
| 3 | `set_text_content` returns `InvalidMutation` for elements; `set_node_text` no-ops on elements | `browser/js.rs:512-527` | checked mutation (`vaji-dom::mutate`) returns errors, never silent no-ops |
| 4 | `set_attribute` snapshot on a never-styled element **panics Stylo** | `surfer-page/src/render/engine.rs:2019` (test `#[ignore]`d) | snapshot always valid; restyle-on-demand |
| 5 | `bliss-dom` `incremental` feature off → every edit rebuilds all layout | `engine.rs:909-914` | incremental restyle is the default |
| 6 | Node ids are recyclable slab indices → consumer mints stable handles | `surfer/src/devtools/mod.rs:13-24` | `NodeId` never reused |
| 7 | `Document` is `!Send` → page must live on UI thread; sub-docs blocked | `bliss-dom` `document.rs:132` | `Send` tree; worker-owned sub-documents |
| 8 | Sub-documents (`set_sub_document`) **unused**; page is a texture | `SURF-SUBDOC-1`, `SURF-ENGINE-2` | first-class sub-documents |
| 9 | `bliss-shell` leaves Linux a11y backend off → consumer re-enables via feature unification | `super-surfer/Cargo.toml:54-57` | a11y works by default |
| 10 | `BlissApplication::resumed()` is a no-op stub; `convert_events` private → consumer reimplements | `app/event_handling.rs:13-37` | real lifecycle hooks, public event conversion |
| 11 | `// TODO: needs bliss API` — no node-id-based attr/text mutation; `SetStyle`/`ReplaceInnerHtml` mutations unimplemented | `app/worker.rs:1092-1133` | mutation vocabulary is complete + id-based |
| 12 | JS↔DOM bridge 100–20,000× slower than native | `SURF-ENGINE-2.md:18` | typed DOM-operation vocabulary (a stable bridge seam) |

Planning docs worth reading: `docs/RECONCILE-browser-engine-consolidation.md`
(why bliss-dom became canonical), `docs/SURF-ENG-1-op-mapping.md` (the full
obscura→bliss-dom op map), and the `SURF-*` tickets in `.jagent/planning/`.

---

## 2. arniko — the UI/reactive substrate

Root: `/workspace/projects/surfer-workspace/arniko`. Members: `crates/arniko`,
`crates/arniko-crush`, `crates/exo-bliss-net`.

### 2.1 Dependency & feature map

All `bliss-*` (plus `stylo_taffy`, `accesskit_xplat`, `anyrender_vello`) come
from crates.io 0.3 with `default-features = false`; only the `bliss` facade keeps
defaults. arniko features:

```
launch   = ["bliss"]                                        # native window
reactive = ["bliss", "bliss-dom", "anyrender_vello", "winit", "keyboard-types", "mustang"]
networking = ["dep:exo-bliss-net"]
gpu      = ["mustang/gpu"]
```

`accesskit_xplat` is declared but **unconsumed** (`Cargo.lock` omits it) — a11y
was planned, never wired.

### 2.2 API surface actually used

- **`bliss` facade**: `bliss::dom::{Document, DocumentConfig, DocumentMutator}`,
  `bliss::html::{HtmlDocument, HtmlProvider}`, `bliss::shell::{BlissApplication,
  BlissShellEvent, BlissShellProxy, WindowConfig, create_default_event_loop}`,
  `bliss::traits::net::DummyNetProvider`,
  `bliss::traits::events::{EventSink, DomEvent, DomEventData, EventPhase,
  BlissKeyEvent}`, and the free fn `bliss::launch_static_html`.
- **`bliss-dom`** (direct): `qual_name!`, `local_name!`, `ns!`, `Attribute`,
  `QualName`, `BaseDocument`, `DocumentConfig`, `DocumentMutator`. Methods:
  `new`, `from_html`, `mutate`, `get_node`, `get_element_by_id`,
  `root_element`/`try_root_element`, `set_event_sink`, `set_events_enabled`,
  `resolve_layout`, `get_client_bounding_rect`, `hit`, `set_hover_to`,
  `scroll_viewport_by_has_changed`, `scroll_node_by_has_changed`, `clear_focus`,
  `clear_hover`, `is_animating`, `viewport_scroll`; mutator:
  `create_element`, `create_text_node`, `append_children`, `insert_nodes_before/after`,
  `remove_node`, `remove_and_drop_node`, `remove_and_drop_all_children`,
  `replace_node_with`, `reparent_children`, `child_ids`, `set_node_text`,
  `set_attribute`, `clear_attribute`, `set_inner_html`, `set_style_property`.
- **`bliss-traits`** (via `exo-bliss-net`): `net::{Request, NetHandler, NetPolicy,
  DefaultNetPolicy, NetProvider}`.
- **`bliss-shell`**: only through the facade (`BlissApplication`,
  `BlissShellEvent::Poll`, `BlissShellProxy`, `WindowConfig`,
  `create_default_event_loop`); no direct `bliss_shell::` import.
- **`bliss-anyrender-vello`** (direct): `VelloWindowRenderer`
  (`set_scene_effects`, `register_custom_paint_source`), `VelloScenePainter`,
  `CustomPaintSource`, `CustomPaintCtx`, `DeviceHandle`, `TextureHandle`,
  plus `vello`/`wgpu`.

### 2.3 What arniko builds on top

- **Components** — pure HTML-string generation (28 components; `Component::
  render() -> String`), no bliss.
- **Reactive layer** (`reactive` feature) — `Signal`/`Computed`/`Reactor`
  (version-counter flush), a `View` trait (`mount(&mut DocumentMutator, ...)`),
  `EventRouter` + `ArnikoEventSink` (`impl EventSink`), `DirectDomMutator`
  (notifies a `SceneScheduler` after each mutation), `batch`, `Show`/`Switch`,
  `KeyedFor`, `ErrorBoundary`, resources/effects/context.
- **Launch path** — `launch_reactive[_configured|_scheduled]` builds
  `create_default_event_loop()` → `BlissShellProxy` → `HtmlDocument::from_html`
  (with `HtmlProvider` + `DummyNetProvider`) → `VelloWindowRenderer` →
  `WindowConfig` → `BlissApplication`; a custom `ReactiveApplication` flushes the
  reactor after window events and redraws.
- **Mustang integration** (`arniko-mustang`, a separate crate) — the
  `SceneScheduler` "dirty region" hook: `Reactor::flush` calls `on_dom_changed()`;
  `DirectDomMutator` calls `on_region_changed(region)`; the renderer's
  `set_scene_effects` consumes `take_dirty()`.

The declared target architecture
(`mustang/docs/architecture/arniko-mustang-bliss-surfer-architecture.md`, s305)
said *"arniko does its own layout (Taffy), text (Parley), painting (Vello); **no
Stylo, no bliss-dom**"* and gives the boundary doctrine:

> don't add HTML/CSS parsing to arniko, don't add reactive signals to
> bliss-engine, don't add layout to mustang.

The shipped code contradicts the "no bliss-dom" half (BLISS-HOME-2 kept arniko a
bliss consumer). **The doctrine half is the part to keep**, and it maps cleanly
onto Vaji: parsing/style/layout/paint in Vaji; reactivity in arniko; compositing
in mustang.

### 2.4 Friction & wants

| # | Friction | Where | Vaji's answer |
|---|---|---|---|
| 1 | `set_inner_html` silently did nothing unless `HtmlProvider` was installed; content absent entirely in some paths | `reactive/app.rs:243-247`, `examples/texture_overlay_spike.rs:24-34` | parsing is unconditional (`vaji-html`) |
| 2 | Events **do not bubble** — sink called once with `current_target = target`; ancestor handlers never fire; two consumer workarounds | `.dejavue/plan.md:7` | `vaji-events::dispatch_path` |
| 3 | `DocumentMutator` is **infallible** and silently no-ops → arniko deleted its error enum as dead code | `reactive/direct_mut.rs:187-191` | checked mutation returns `Result` |
| 4 | a11y never wired (`accesskit_xplat` declared, unused) | `docs/PRODUCTION_READINESS_SPEC.md:142` | a11y tree built from `vaji-dom`, on by default |
| 5 | `networking` feature enables `exo-bliss-net` but the crate is never imported | `docs/REMAINING_TRACKS.md:66` | net is a narrow, honest crate |
| 6 | `root_element` widened to `Option` mid-flight (D-2c) | `REMAINING_TRACKS.md` | consistent total APIs |
| 7 | `register_custom_paint_source` ids rely on a process-global counter; `ImageData` not public → texture handle uncacheable | `texture_overlay_spike.rs:194-259` | scene-effects/overlay seams are first-class (`vaji-paint`) |

---

## 3. The union Vaji must serve

The consumer-visible surface Vaji must provide (or improve on), by crate:

- **Tree & mutation**: `Document` trait-ish facade, `DocumentMutator`,
  `DomController`, node inspection (`text_content`, `outer_html`,
  `absolute_position`, `final_layout`, `attr(s)`), id-based mutation, queries.
- **Events**: `UiEvent` + key/pointer/wheel variants, `EventSink`, event phases,
  bubbling, `handle_ui_event`.
- **Style/layout/text**: `resolve`/`resolve_layout`, `snapshot_node`,
  `upsert_stylesheet_for_node`, `get_client_bounding_rect`.
- **Net**: `NetProvider`/`NetHandler`/`Request`/`Method`/`Body`/`FormData`,
  `NavigationProvider`/`NavigationOptions`.
- **Paint**: `paint_scene`-equivalent, scene effects, dirty regions, overlay
  textures, custom paint sources.
- **Shell**: launch/application/proxy/window-config, viewport, redraw, clipboard,
  file dialog, accessibility.
- **Headless**: tree dump, virtual clock, synthetic input.

## 4. Out of scope — exosphere and others

Exosphere (`exo-workspace/exosphere`, via the `bliss-core` wrapper and the
`vendor/bliss-engine` copy), `exo-workspace/exosphere-apps`, `crush-gui`,
`khukuri-desktop`, `cece-code`, `bro-cli`, and the demo/archive trees are **not**
targets of this work and **get their own tickets**. Several of them are already
dangling on paths to engine copies that were removed (the arniko in-tree `bliss-*`
crates and `exosphere/crates/platform/rendering/`) — their cleanup is theirs to
schedule, not Vaji's to absorb.

The Vaji-side obligation toward them is only this: keep the surface
compatibility-friendly (see [`COMPATIBILITY.md`](COMPATIBILITY.md)) so a later
adopt-or-not decision stays cheap.
