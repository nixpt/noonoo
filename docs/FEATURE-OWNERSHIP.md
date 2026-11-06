# Noonoo — Feature Ownership

When you reimplement an engine whose consumers have grown features of their own,
the first question is *who owns what*. This is Noonoo's answer, derived from the
[consumer analysis](CONSUMERS.md). The rule of thumb: **Noonoo owns the substrate
(parse → style → layout → paint → events → shell); consumers own their
application logic.** If a consumer built something because the engine *lacked*
it, that missing thing moves *into* Noonoo — not into the consumer's port.

## 1. Owned by Noonoo (engine substrate)

| Capability | Today | Reasoning |
|---|---|---|
| Document tree + stable node ids | `bliss-dom` (recyclable indices) | Ids consumers must be able to hold across mutations |
| Checked mutation (`DocumentMutator`) | `bliss-dom` (infallible, silent no-op) | Silent no-ops forced consumers to delete their error types |
| Queries + `query_selector*` | `bliss-dom` | |
| `DomController` | `bliss-traits::dom_control` (separate grab-bag) | It is a thin DOM writer; it belongs with the DOM |
| Style / cascade / incremental restyle | `bliss-dom` + `stylo_taffy` | Incremental must be the default, not a feature |
| Layout | `bliss-dom` + `stylo_taffy` | One boundary (`noonoo-layout`) |
| Text shaping / line breaking | `bliss-dom` (mixed in) | Split out (`noonoo-text`) — different engine, different failures |
| HTML parsing | `bliss-html` | Parsing must never be a silent no-op |
| Event model + **bubbling** | `bliss-traits::events` + `bliss-dom` driver | Bubbling is a correctness fix consumers asked for |
| Net traits + schemes | `bliss-traits::net` + `bliss-net` | One narrow crate |
| Navigation seam | `bliss-traits::navigation` | Consumed by surfer; part of the net surface |
| Paint command stream | `bliss-paint` | Owned translation |
| **Scene effects / dirty regions** | forked `anyrender_vello` | A compositor seam belongs in the paint crate, not a fork |
| **Overlay textures / custom paint sources** | forked `anyrender_vello` | First-class, not a spike |
| Shell launch + lifecycle | `bliss-shell` | `resumed`/suspend must be real, not stubs |
| Viewport / colour scheme | `bliss-traits::shell` | Belongs with the shell |
| Accessibility | `bliss-shell` (Linux no-op) | Works by default (`noonoo-shell` `a11y`) |
| **Sub-documents** | `bliss-dom` (half-built, unused) | The single biggest consumer want |
| **Motion / spring timing** | planned (`BLISS-MOTION-1`) | Owned (`noonoo-motion`) |
| **Headless tooling** (dump/clock/input) | absent; consumers hand-rolled | First-class (`noonoo-headless`) |

Bolded rows are capabilities consumers **lack today** or carry workarounds for;
they are Noonoo's reason to exist beyond a straight port.

## 2. Kept out of Noonoo (consumer territory)

| Capability | Owner | Why not Noonoo |
|---|---|---|
| Component library / HTML-string builder | arniko components | Application UI |
| Reactive signals / `Reactor` / `View` | arniko `reactive` | The architecture doctrine: no reactive signals in the engine |
| JS engine (Boa) + DOM/network bridges | surfer | A scripting host's job; Noonoo provides the *typed DOM op vocabulary* it binds to |
| Compositor / `SceneScheduler` | mustang | The doctrine: layout/effect scheduling above the engine |
| Browser chrome, tabs, navigation policy | surfer | Application |
| DevTools UI panels | surfer | Application (built on Noonoo's dump/queries) |
| Capsules / Crush host caps | arniko-crush | Application |
| Storage / cookies / SSRF policy | surfer | Application policy; Noonoo exposes the net seam |
| `exo://` scheme policy | exo-bliss-net | Host-specific; Noonoo exposes the `NetProvider` seam |

## 3. Borderline calls (and their resolution)

| Thing | Resolution |
|---|---|
| `bliss-traits` grab-bag (events + net + shell + dom_control together) | **Split** into the crates that own each concern. Nothing is called "traits". |
| `stylo_taffy` as a separate published crate | **Dissolved**: the bridge is the style→layout seam, now expressed as the `noonoo-style` → `noonoo-layout` dependency. |
| `bliss-html` ↔ `bliss-dom` | `noonoo-html` produces `noonoo-dom` trees; one direction. |
| Umbrella crate (`arniko-bliss`) | **Kept** as `noonoo`, but documented as convenience-only; consumers are steered to specific crates. |
| `accesskit_xplat` / `anyrender_vello` forks | **Not ported as forks.** The *capabilities* (Linux a11y backend; scene effects + custom paint) move into `noonoo-shell` / `noonoo-paint` behind features. |
| A `Document` trait object (`Box<dyn Document>`) | **Avoid**: it is what made `downcast_mut` fail for consumers. Prefer a concrete `Document` + generics. |

## 4. The doctrine (inherited, and it holds)

> Parsing, style, layout, text, paint, events, shell → **Noonoo**.
> Reactivity, components, scripting hosts → **the consumer** (arniko, surfer).
> Compositing / effect scheduling → **mustang**.

Noonoo keeps that split, and adds the missing engine capabilities that the split
previously assumed existed but did not.
