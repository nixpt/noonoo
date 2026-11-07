# Noonoo — Compatibility & Migration

The goal: **switching a consumer from the `bliss-*` stack to the `noonoo-*` stack
is cheap, and the cost of doing so is measured rather than guessed.**

Two mechanisms make it cheap (package aliasing + compat shims), and one tool
makes it measured — **`api-drift`**, which snapshots a crate's public surface and
classifies the drift between two versions.

---

## 1. The switch mechanic: package aliasing

Cargo lets a dependency key differ from the package name:

```toml
# consumer's Cargo.toml — before
bliss-dom = { version = "0.3", optional = true }

# after — same import path in every source file
bliss-dom = { package = "noonoo-dom", version = "0.20", optional = true }
```

The dependency **key** (`bliss-dom`) is the extern-crate name, so
`use bliss_dom::Document;` keeps compiling unchanged. Consumers *already* use this
pattern — arniko/surfer write `bliss = { package = "arniko-bliss" }` and
`anyrender_vello = { package = "bliss-anyrender-vello" }`. Switching is therefore
a **manifest-only** change, provided the public API matches. That "provided the
API matches" is exactly what `api-drift` measures.

## 2. Compat shims (for API fidelity and clean measurement)

`api-drift` matches items by **path** (`bliss_dom::Document`). A crate whose lib
name is `noonoo_dom` would diff against `bliss_dom` as *every item removed + every
item added*. Two options:

- **Option A (recommended)** — ship a small `compat/` set of **shim crates** whose
  `[lib] name` matches the bliss crate it stands in for (`bliss_dom`,
  `bliss_html`, ...) and which simply re-export from the `noonoo-*` crates:

  ```toml
  # compat/bliss-dom/Cargo.toml
  [package]
  name = "noonoo-bliss-dom-compat"
  [lib]
  name = "bliss_dom"
  [dependencies]
  noonoo-dom = { path = "../../crates/noonoo-dom" }
  ```

  The shim is simultaneously (a) the drop-in target consumers alias to, and
  (b) the surface `api-drift` snapshots so the diff reads cleanly.

- **Option B** — no shims; require source edits and normalize path prefixes
  before diffing. Cheaper to build, more churn for consumers. Kept as a fallback.

During the port, a **transient** `[patch.crates-io]` override (uncommitted) is
the right way to test a consumer against local Noonoo without publishing —
the same pattern arniko already documents for `mustang`.

## 3. The measurement tool: api-drift

`api-drift` (`/workspace/projects/api-drift`) snapshots a Rust crate's public
surface from **rustdoc JSON**, diffs two snapshots, classifies each change
(`Compatible` / `Warning` / `Breaking`), and suggests fixes. Relevant facts:

- **Capture is rustdoc JSON**, so it needs a **nightly** toolchain:
  `cargo +nightly rustdoc -p <crate> -- -Z unstable-options --output-format json`.
  Use a *pinned* nightly for reproducibility (this box has `nightly-2026-08-28`).
- The committed artifact is `<surface>.snapshot.json`:
  `{ format, surface, version, items[], content_hash }`, with items as
  `{ path, kind, sig, attrs }` and a sha256 **content hash** verified on read
  (fail-closed).
- The CLI (`api-drift check|note`) does **not** capture. Capture is done through
  the library (`producer` + `serde` features) — hence our helper, below.
- `RustPublicSurface::name()` is hard-coded `"rust-public"`, so use **one
  directory per crate** (`api-drift/bliss/<crate>/`), or a custom `Surface`.
- Comparison is library-level (`diff_snapshots` → `classify_diff` →
  `suggest_for_breaks`); there is no CLI `diff`.
- Known limits: macros/type-aliases are invisible; re-exports of *external*
  items carry only `re-export of <path>`; `--all-features` may not compile (gate
  on a chosen feature set and use the same set on both sides).

## 4. The recipe (capture → gate)

Set up once, then read the scoreboard every slice.

```sh
# 0. build the helper (outside the workspace; see tools/api-snapshot)
cargo build --manifest-path tools/api-snapshot/Cargo.toml

# 1. produce rustdoc JSON for one crate (pinned nightly, fixed feature set)
cd /workspace/projects/libs/bliss-engine
cargo +nightly-2026-08-28 rustdoc -p bliss-dom \
    -- -Z unstable-options --output-format json      # -> target/doc/bliss_dom.json

# 2. turn it into a committed snapshot
tools/api-snapshot/target/debug/api-snapshot \
    target/doc/bliss_dom.json bliss-dom-rust-public "bliss-dom 0.3.2" \
    --out ../../libs/noonoo/api-drift/bliss/bliss-dom.snapshot.json

# repeat for each crate/feature set; the snapshot is committed
```

To **measure a port**, snapshot the same crate name on the Noonoo side (via the
compat shim, so the lib name matches) and diff:

```sh
api-diff api-drift/bliss/bliss-dom.snapshot.json \
         api-drift/noonoo/bliss-dom.snapshot.json
# -> [Compatible/Added] ... / [Breaking/Removed] ... / [Warning/SignatureChanged] ...
# exit 2 if anything is Breaking
```

**CI policy** (to be wired once shims exist):

- Any `Breaking` drift on the **consumer-visible subset** (see
  [`CONSUMERS.md`](CONSUMERS.md) §3) fails CI.
- `Compatible` additions never fail.
- Every intentional break must be recorded with `api-drift note` and carries a
  migration line in the changelog.
- Snapshots are committed under `api-drift/{bliss,noonoo}/`; the `bliss/` side is
  the frozen reference the port is measured against.

## 5. What we snapshot first

Snapshot the **consumer-visible subset**, not the world — a full 10-crate,
all-features capture of `bliss-dom` is a heavy rustdoc build and most of it does
not matter. Order:

1. `bliss-traits` (net/events/navigation/shell/dom_control) — small, and the
   whole seam consumers program against.
2. `bliss-dom` (the DOM, mutation, queries, resolve, dom_control) — the deep one.
3. `bliss-html`, `bliss-paint`, `bliss-shell`, `bliss-net` — the rest of the
   surface.
4. `arniko-bliss` (facade) — only to confirm the re-export paths.

The reference snapshots live in `api-drift/bliss/`. This is deliberately a
snapshot of the **current released 0.3.2**, not of `main`, so the reference is
what consumers actually build against.
