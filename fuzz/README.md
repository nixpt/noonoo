# Noonoo fuzzing

Four cargo-fuzz targets for the hand-written parsers:

| Target | Crate | Invariant exercised |
| --- | --- | --- |
| `html` | `noonoo-html` | parse any UTF-8 input, then walk every element |
| `css` | `noonoo-css` | parse a stylesheet, then read its rules and font faces |
| `json` | `noonoo-json` | parse JSON, then re-serialize an accepted value |
| `toml` | `noonoo-toml` | parse TOML, then read the JSON projection |

The project lives outside the workspace (root `exclude`) because the fuzzing
runtime is not engine surface. Seeds under `corpus/<target>/` give the fuzzer a
head start; `artifacts/` and `coverage/` are ignored.

## Run

Fuzzing needs a **nightly** toolchain and [`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz):

```sh
cargo install cargo-fuzz
cargo +nightly fuzz run html
cargo +nightly fuzz run css
cargo +nightly fuzz run json
cargo +nightly fuzz run toml
```

`cargo +nightly fuzz list` should print all four targets. Reproduce a crash
from `fuzz/artifacts/<target>/` with:

```sh
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash-file>
```

## Stable-CI counterpart

Fuzzing is nightly-only, so the workspace also carries deterministic
`tests/robustness.rs` suites in each parser crate. They feed an adversarial
corpus plus pseudo-random UTF-8 through the same entry points and run on stable
in the normal `cargo test --workspace` job. The two layers are complementary:
the stable suite catches deterministically; the fuzzer explores.
