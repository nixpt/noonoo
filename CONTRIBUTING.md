# Contributing to Noonoo

## Before you write code

Read [`ARCHITECTURE.md`](ARCHITECTURE.md) and
[`docs/DEPENDENCIES.md`](docs/DEPENDENCIES.md). The two things that will get a
change sent back are (1) leaking a borrowed engine's types past its module and
(2) adding a dependency that the standard library or an existing crate already
covers.

## Clean room

This project is a clean-room reimplementation. Do not copy code from
`bliss-engine`, Blitz, or any other rendering engine. Work from behaviour,
specification, and ticket text. If you are modelling something that exists
elsewhere, describe what it must *do*, then write it fresh.

## Checks

Every change must pass:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

CI also checks the declared MSRV (**1.89**) on its own toolchain. If a change
raises the floor, say so in the PR and bump `rust-version` in the workspace
manifest — the declared MSRV must stay true.

## Features

New backend work lands behind a feature and does not become a default. The
default build stays dependency-light: no GPU, no window, no screen reader.

## Commits

Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`, ...), scoped by crate
where it helps (`feat(noonoo-core): ...`).
