# api-snapshot (Noonoo tooling)

Two tiny binaries that fill the gaps in [`api-drift`](../../../api-drift):

- **`api-snapshot`** — rustdoc JSON → an api-drift `.snapshot.json` (api-drift's
  CLI has no capture command).
- **`api-diff`** — diff two snapshots, print classified breaks, exit `2` when
  anything is `Breaking` (api-drift's CLI has no `diff` command).

This crate is **excluded from the Noonoo workspace** on purpose: it pulls
`serde_json` and a path dependency on `api-drift`, and the engine build must not
pay for either.

## Build

```sh
cargo build --manifest-path tools/api-snapshot/Cargo.toml
# binaries: tools/api-snapshot/target/debug/{api-snapshot,api-diff}
```

## Capture a snapshot

```sh
# 1. rustdoc JSON (pinned nightly — reproducibility matters)
cd /workspace/projects/libs/bliss-engine
cargo +nightly-2026-08-28 rustdoc -p bliss-dom -- \
    -Z unstable-options --output-format json          # -> target/doc/bliss_dom.json

# 2. snapshot it
/workspace/projects/libs/noonoo/tools/api-snapshot/target/debug/api-snapshot \
    target/doc/bliss_dom.json \
    bliss-dom-rust-public \
    "bliss-dom 0.3.2" \
    --out /workspace/projects/libs/noonoo/api-drift/bliss/bliss-dom.snapshot.json
```

Notes:

- One `surface` name per crate (`bliss-dom-rust-public`, ...) — `api-drift`'s
  `RustPublicSurface` hard-codes `rust-public`, so distinct names avoid clashes.
- Feature flags change the surface: pass the same `--features`/`--all-features`
  selection on every capture of a given crate, and diff like against like.
- Macros and type aliases are invisible to the producer; treat the snapshot as
  the consumer-visible subset, which is what we care about.
