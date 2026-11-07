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

Numbers are medians of wall-clock samples after one warm-up. Treat them as
relative signals, not a regression gate — CI only smoke-runs the binary so it
cannot rot.

## Notes

- Inputs are generated deterministically; no fixtures or network.
- Sizes are approximate: generators stop once the target length is exceeded.
- The pipeline case caps its input at 256 KiB so a 1 MiB parse run does not
  spend minutes in layout.
