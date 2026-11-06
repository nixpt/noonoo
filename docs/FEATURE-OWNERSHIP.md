# Vaji — Feature Ownership

When you reimplement an engine whose consumers have grown features of their own,
the first question is *who owns what*. This is Vaji's answer, derived from the
[consumer analysis](CONSUMERS.md). The rule of thumb: **Vaji owns the substrate
(parse → style → layout → paint → events → shell); consumers own their
application logic.** If a consumer built something because the engine *lacked*
it, that missing thing moves *into* Vaji — not into the consumer's port.

## 1. Owned by Vaji (engine substrate)

| Capability | Today | Reasoning |
|---|---|---|
| Document tree + stable node ids | `bliss-dom` (recyclable indices) | Ids consumers must be able to hold across mutations |
| Checked mutation (`DocumentMutator`) | `bliss-dom` (infallible, silent no-op) | Silent no-ops forced consumers to delete their error types |
| Queries + `query_selector*` | `bliss-dom` | |
| `DomController` | `bliss-traits::dom_control` (separate grab-bag) | It is a thin DOM writer; it belongs with the DOM |
| Style / cascade / incremental restyle | `bliss-dom` + `stylo_taffy` | Incremental must be the default, not a feature |
| Layout | `bliss-dom` + `stylo_taffy` | One boundary (`vaji-layout`) |
| Text shaping / line breaking | `bliss-dom` (mixed in) | Split out (`vaji-text`) — different engine, different failures |
| HTML parsing | `bliss-html` | Parsing must never be a silent no-op |
| Event model + **bubbling** | `bliss-traits::events` + `bliss-dom` driver | Bubbling is a correctness fix consumers asked for |
| Net traits + schemes | `bliss-traits::net` + `bliss-net` | One narrow crate |
| Navigation seam | `bliss-traits::navigation` | Consumed by surfer; part of the net surface |
| Paint command stream | `bliss-paint` | Owned translation |
| **Scene effects / dirty regions** | forked `anyrender_vello` | A compositor seam belongs in the paint crate, not a fork |
| **Overlay textures / custom paint sources** | forked `anyrender_vello` | First-class, not a spike |
| Shell launch + lifecycle | `bliss-shell` | `resumed`/suspend must be real, not stubs |
| Viewport / colour scheme | `bliss-traits::shell` | Belongs with the shell |
| Accessibility | `bliss-shell` (Linux no-op) | Works by default (`vaji-shell` `a11y`) |
| **Sub-documents** | `bliss-dom` (half-built, unused) | The single biggest consumer want |
| **Motion / spring timing** | planned (`BLISS-MOTION-1`) | Owned (`vaji-motion`) |
| **Headless tooling** (dump/clock/input) | absent; consumers hand-rolled | First-class (`vaji-headless`) |

Bolded rows are capabilities consumers **lack today** or carry workarounds for;
they are Vaji's reason to exist beyond a straight port.

## 2. Kept out of Vaji (consumer territory)

| Capability | Owner | Why not Vaji |
|---|---|---|
| Component library / HTML-string builder | arniko components | Application UI |
| Reactive signals / `Reactor` / `View` | arniko `reactive` | The architecture doctrine: no reactive signals in the engine |
| JS engine (Boa) + DOM/network bridges | surfer | A scripting host's job; Vaji provides the *typed DOM op vocabulary* it binds to |
| Compositor / `SceneScheduler` | mustang | The doctrine: layout/effect scheduling above the engine |
| Browser chrome, tabs, navigation policy | surfer | Application |
| DevTools UI panels | surfer | Application (built on Vaji's dump/queries) |
| Capsules / Crush host caps | arniko-crush | Application |
| Storage / cookies / SSRF policy | surfer | Application policy; Vaji exposes the net seam |
| `exo://` scheme policy | exo-bliss-net | Host-specific; Vaji exposes the `NetProvider` seam |

## 3. Borderline calls (and their resolution)

| Thing | Resolution |
|---|---|
| `bliss-traits` grab-bag (events + net + shell + dom_control together) | **Split** into the crates that own each concern. Nothing is called "traits". |
| `stylo_taffy` as a separate published crate | **Dissolved**: the bridge is the style→layout seam, now expressed as the `vaji-style` → `vaji-layout` dependency. |
| `bliss-html` ↔ `bliss-dom` | `vaji-html` produces `vaji-dom` trees; one direction. |
| Umbrella crate (`arniko-bliss`) | **Kept** as `vaji`, but documented as convenience-only; consumers are steered to specific crates. |
| `accesskit_xplat` / `anyrender_vello` forks | **Not ported as forks.** The *capabilities* (Linux a11y backend; scene effects + custom paint) move into `vaji-shell` / `vaji-paint` behind features. |
| A `Document` trait object (`Box<dyn Document>`) | **Avoid**: it is what made `downcast_mut` fail for consumers. Prefer a concrete `Document` + generics. |

## 4. The doctrine (inherited, and it holds)

> Parsing, style, layout, text, paint, events, shell → **Vaji**.
> Reactivity, components, scripting hosts → **the consumer** (arniko, surfer).
> Compositing / effect scheduling → **mustang**.

Vaji keeps that split, and adds the missing engine capabilities that the split
previously assumed existed but did not.
