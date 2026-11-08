# noonoo-bench

Dependency-free benchmarks for the parsers and the render pipeline.

The harness uses only `std::time::Instant` and `std::hint::black_box`, so the
tool adds **no third-party packages** to the repository. It lives outside the
workspace (root `exclude`) because it is tooling, not engine surface.

## Run

```sh
# default: 1 KiB / 64 KiB / 1 MiB inputs
cargo run --release --manifest-path tools/bench/Cargo.toml

# one size, fewer samples
cargo run --release --manifest-path tools/bench/Cargo.toml -- \
    --bytes 1048576 --iters 3

# one family
cargo run --release --manifest-path tools/bench/Cargo.toml -- --only json
```

Covered:

| Name | What it measures |
| --- | --- |
| `html/parse` | tokenizer + tree builder on generated documents |
| `css/parse` | stylesheet tokenizer + rule/value parsing |
| `json/parse` | JSON parse into `noonoo_json::Json` |
| `toml/parse` | TOML subset → JSON projection |
| `pipeline/paint` | full compute-styles → layout → paint command stream |
| `a11y/build` | accessibility tree over a precomputed 64 KiB layout |
| `ui/style`, `ui/layout`, `ui/paint` | each stage alone for 320 arniko cards (~4,200 elements) under the real arniko stylesheet |
| `ui/restyle-one` | one element changes class, then the styles are brought up to date |
| `nested/<shape>-<depth>` | layout of one box nested `depth` deep as grid, flex, flex column, inline-block, float, table cell or absolute; `NESTED_DEPTHS=8,64` picks the depths |

`ui/*` and `nested/*` are the incremental-pipeline yardsticks (roadmap
Phase 4). Their starting point, on the zorro box (release build):

| Benchmark | Start | Now | What moved it |
| --- | --- | --- | --- |
| `ui/style` | 78.7 ms | 13.4 ms | rule index (25.5), shared custom properties |
| `ui/layout` | 6.1 ms | 5.5 ms | |
| `ui/paint` | 0.36 ms | 0.36 ms | |
| `ui/restyle-one` | 78.9 ms (a full restyle) | 13.5 ms | as `ui/style` |
| `nested/flex-256` | 88.8 ms (32: 0.67 ms) | | |
| `nested/grid-256` | 112.6 ms (32: 3.7 ms) | | |

Numbers are medians of wall-clock samples after one warm-up. Treat them as
relative signals, not a regression gate — CI only smoke-runs the binary so it
cannot rot.

## Notes

- Inputs are generated deterministically; no fixtures or network.
- Sizes are approximate: generators stop once the target length is exceeded.
- The pipeline case caps its input at 256 KiB so a 1 MiB parse run does not
  spend minutes in layout.
