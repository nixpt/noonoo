# Noonoo — Surfer primitives

What **surfer-browser** needs from a rendering engine, and what Noonoo provides
today. Surfer is one of Noonoo's two target consumers (with arniko); this map is
grounded in its actual code, not in how we plan to use it later.

Surfer today (`/workspace/projects/surfer-browser`, locked on arniko's
`bliss-*` 0.3): `crates/surfer-page` owns the page engine (canonical
`bliss_html::HtmlDocument`, a Boa JS runtime, and a `SurferRenderEngine` over
`bliss_traits`/`bliss_paint` + `anyrender_vello`), `crates/super-surfer` owns the
GUI (`bliss-shell` + `winit` + overlay textures per tab). Its own engine
blueprints live in that repo (`docs/RECONCILE-browser-engine-consolidation.md`,
`docs/SURF-ENG-1-op-mapping.md`, `docs/ARCHITECTURE-MAP.md`); this file is
Noonoo's side of the same question.

## The union surfer exercises

| Layer | What surfer calls today |
| --- | --- |
| DOM | `HtmlDocument` (canonical, non-optional), `query_selector[_all][_from]`, `get_element_by_id`, `text_content`, `attr`, `children`, ancestors, `outer_html`, plus 22 mutation ops (`setAttribute`…`insertAdjacentHTML`, native `set_inner_html`, `deep_clone_node`, `insert_nodes_before/after`) |
| Events | DOM events with 3-phase propagation (capture/target/bubble), cancelables, JS-side listener registry |
| Layout/paint | `resolve()`, `layout_height/width`, `paint_scene` → Vello `Scene`, `render_to_buffer/png` (`anyrender::render_to_buffer`), decoded-image overlay on top of the scene |
| Net | reqwest client (cookies, SSRF guard, robots, encodings) + a `NetWaker` trait |
| JS | its own Boa runtime over the DOM bridge — **surfer owns JS; Noonoo must not** |
| Shell | `bliss_shell::View`/`BlissApplication`, `accesskit_xplat` re-enable, per-tab overlay textures, clipboard/file-dialog features |
| Devtools | engine-facing getters and setters (`devtools_set_attribute`, `devtools_set_text`, attribute maps, node dumps) |

## What Noonoo provides today

| Surfer need | Noonoo primitive | Where |
| --- | --- | --- |
| Canonical DOM, stable ids, checked mutation | `Document`, `NodeId` (never reused), `DocumentMutator` returning `Result` | `noonoo-dom` |
| Queries | `get_element_by_id`, `get_elements_by_tag_name` (`noonoo-dom`); full-selector `query_selector[_all]`, element-scoped `query_selector[_all]_in` (`querySelectorFrom`), and `matches`, with combinators, attributes, `:not`, `:is`, `:nth-child` | `noonoo-query` over `noonoo-selectors` |
| `text_content`, visible text | `Document::text_content` (surfer's skip-`<script>` wrapper stays host-side) | `noonoo-dom` |
| Events | `dispatch` with bubbling + phases, `EventListener` | `noonoo-events` |
| Style | [`noonoo-style`]: UA sheet + author sheets + inline `style`, vars/`calc()`, `:hover/:focus/:active` in the cascade | `noonoo-style` |
| Layout | block/inline/flex/grid/floats/positioned; scroll containers, hit-test, scroll chaining | `noonoo-layout` |
| Paint | backend-neutral command stream (rects, gradients, polygons, text runs, clips, opacity, radius) | `noonoo-paint` |
| Vello scene | `CommandList` → `vello::Scene`; winit window + input | `integrations/noonoo-vello`, `integrations/noonoo-winit` |
| Fonts | `@font-face` descriptors, TrueType metrics/outlines/kerning, `Shaper` seam | `noonoo-engine::font_faces`, `noonoo-ttf` |
| Headless | tree JSON, a11y JSON, virtual clock, synthetic input | `noonoo-headless` |
| Multi-document window | `Host`: layers (chrome/content), region routing, hit-test, focus/hover/active, composition, sub-documents sized by host nodes | `noonoo-shell` |
| Accessibility | owned a11y tree + AccessKit projection + Linux AT-SPI adapter (see `docs/A11Y.md`) | `noonoo-a11y`, integrations |
| Gating | subject/scope/action model + opt-in XIP adapter | `noonoo-caps`, `integrations/noonoo-xip` |
| Light data formats | JSON/TOML/INI renderings behind the value model | `noonoo-json`, `noonoo-toml`, `noonoo-ini` |
| Declarative fetch+swap | `Hx` (htmx-like) — the *page↔engine seam* exists; surfer's Boa path doesn't need it | `noonoo-hx` |

`Engine` (the assembled document) is `Send`, so a tab worker can own styles,
layout, and the paint stream off the main thread and hand the main thread
commands — no texture overlay needed in the *engine* sense. Surfer's
`render_to_buffer/png` maps onto `noonoo-vello` (`docs/BACKEND.md`).

## Tier 1 — the missing DOM-bridge primitives (small, mechanical)

These close surfer's 22-op JS bridge almost one-to-one, and replace import
helpers surfer hand-rolled. Each is a small, headless-testable add.

1. **Serialization**: `inner_html` / `outer_html` on a node, with an escaping
   writer in `noonoo-html` (the serializer, not the tree).
2. **Fragment ingest**: parse HTML *into* an existing document — a
   `set_inner_html(node, html)` (parse + clear children + append) and
   `insert_nodes_before/after`-shaped fragment parse, so `insertAdjacentHTML`
   has all four positions natively.
3. **Scoped queries** (done, `noonoo-query`): `query_selector_in(root, selector)`
   / `_all_in` with the exclude-self semantics surfer's `querySelectorFrom`
   defines, matching against the whole document as the DOM does.
4. **Mutation round-out**: `deep_clone`, `insert_after`/`replace_with`,
   `create_element` with attrs, attribute list iteration, `node_value`/comment
   access, and `remove_and_drop_children`.

## Tier 2 — rendering parity for real pages (bigger, sequenced)

5. **Images end to end**: a `DrawImage` paint command and an SRC-set/intrinsic
   sizing seam, with the *decode cache* staying host-side (surfer's `image`
   crate + LRU). Layout needs an image descriptor to size `<img>` correctly;
   paint to place it. No image codecs in the engine.
6. **Incremental restyle/relayout**: surfer scripts mutate the DOM on every
   callback and then call `resolve()`. Noonoo's pipeline is full recompute today;
   fine for small pages, quadratic-ish for big ones. The seam to own is
   invalidation: style scope → layout scope → paint scope, mirroring surfer's
   `InvalidationScope` experiment.
7. **A resource loader seam**: fonts already have `@font-face` (`src` goes
   through the host); images and `<style src>` need the same precedent, and
   `noonoo-net::load` should grow a `Loader` trait injection so surfer's hardened
   HTTP/cookie/SSRF client is the transport while the engine stays
   dependency-free. The dormant `http`/TLS feature stays borrowed regardless.
8. **Node positioning helpers**: `bounding_client_rect`/`absolute_position` —
   nearly free once `LayoutTree` boxes are exposed (`Rect` in page coordinates).

## Tier 3 — architectural notes

- **Two processes of one browser**: surfer runs one engine per tab on its own
  thread, then composites. Noonoo does not try to own that; it offers `Engine`
  (`Send`), a paint command stream, and — when a GUI wants a *single window*
  instead — `Host` layers/sub-documents, which is exactly the texture-overlay
  workaround surfer carries.
- **No JS runtime in Noonoo** (surfer owns Boa; see `CONSUMERS.md` §
  "What belongs to the consumer"), no cookie store, no privacy/proxy policy —
  those live behind the loader seam the host satisfies.
- **Backend gaps on the engine side** stand: media queries/transforms/
  animations, form-control painting, and the full HTML5 insertion modes are
  future slices (`docs/ROADMAP.md`); Linux is the only a11y adapter.
