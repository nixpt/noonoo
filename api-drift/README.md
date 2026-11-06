# api-drift baseline

The reference API snapshots Noonoo's compatibility is measured against: the public
surface of the engine Noonoo replaces, `nixpt/bliss-engine` **0.3.2** (the version
consumers resolve to), captured from **rustdoc JSON**.

Produced with [`tools/api-snapshot`](../tools/api-snapshot) on
`nightly-2026-08-28`:

```sh
# rustdoc JSON (per crate), then convert
cargo +nightly-2026-08-28 rustdoc -p <crate> -- -Z unstable-options --output-format json
tools/api-snapshot/target/debug/api-snapshot \
    $CARGO_TARGET_DIR/doc/<lib>.json <surface> "<crate> <ver>" --out <file>
```

## The baseline

| Crate | Surface | Items | `content_hash` (sha256) |
|---|---|---:|---|
| `bliss-traits` | `bliss-traits-rust-public` | 73 | `a728a9ff87f674dca4ea2a5e6af034c36570e08474a144fad290cc2a56f8f587` |
| `bliss-dom` | `bliss-dom-rust-public` | 119 | `a108a3c604189b075daf4a0aa742f48f3024948129469caf89788ae13d098efc` |
| `bliss-html` | `bliss-html-rust-public` | 6 | `f4fd86035e0b1925e38923f9fff3398588983a4eff4bd8df96c92bc6fcd98efd` |
| `bliss-net` | `bliss-net-rust-public` | 2 | `94dd73afc29dc7cf5c61114f9a61625906fcc08776f7ce82f1ee73be885a3b93` |
| `bliss-paint` | `bliss-paint-rust-public` | 1 | `e408d0d80805ebf98636d63527113002adf5abf36e6b5b342fd947019c84e973` |
| `bliss-shell` | `bliss-shell-rust-public` | 17 | `b72e428ca25ea6f1a4934c10aeccc3832dda038b5c9143b27084bdb5c76e33f2` |
| `arniko-bliss` (facade) | `arniko-bliss-rust-public` | 84 | `d51fa3a03c8572ca6cea8143408cb2eeeffcc5562114f50d34620486bf0e1737` |

`bliss-stylo-taffy`, `bliss-accesskit-xplat`, and `bliss-anyrender-vello` are
**deliberately not snapshotted**: Noonoo does not re-export them (the forks'
capabilities move into `noonoo-shell`/`noonoo-paint` behind features; the bridge
dissolves into the `noonoo-style` → `noonoo-layout` seam).

## Caveats (why these numbers are smaller than the source looks)

- **Macros are invisible.** `local_name!`, `ns!`, `qual_name!` carry a lot of the
  consumer's DOM vocabulary and do not appear in a rustdoc-JSON surface. They
  will be hand-tracked, not diffed.
- **Re-exports of external items** record only `re-export of <path>`, not the
  target's signature.
- These figures are **default-feature** captures. A feature-gated item (e.g.
  `bliss-dom`'s `svg`, `bliss-shell`'s `accessibility`) appears only if the
  feature is on; re-capture with the same feature set on both sides when diffing.

## Using it

```sh
# measure a Noonoo shim against the baseline (same surface name, via a compat crate)
tools/api-snapshot/target/debug/api-diff \
    api-drift/bliss/bliss-dom.snapshot.json \
    api-drift/noonoo/bliss-dom.snapshot.json
# exit 2 when anything is Breaking
```

See [`docs/COMPATIBILITY.md`](../docs/COMPATIBILITY.md) for the full recipe and
the CI gate.
