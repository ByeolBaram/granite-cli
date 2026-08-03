# Contributing to Granite CLI

Thank you for your interest in contributing. Please read this guide before opening a PR.

All participants are expected to follow our [Code of Conduct](CODE_OF_CONDUCT.md).

## Building

Requires Rust 1.88.0 or later (see `rust-version` in [`Cargo.toml`](Cargo.toml)).

```sh
cargo build
```

For a release build:

```sh
cargo build --release
```

## Running

```sh
cargo run -- --help
```

## Testing

```sh
cargo test
```

## Contribution flow

1. **Fork** the repository and create a branch for your change.
2. Make a focused, reviewable change. Keep it minimal and task-specific.
3. Run formatting, linting, and tests before pushing:
   ```sh
   cargo fmt --check
   cargo clippy
   cargo test
   ```
4. **Sign off** every commit with a `Signed-off-by` trailer to certify the [Developer Certificate of Origin](https://developercertificate.org/):
   ```sh
   git commit -s -m "your commit message"
   ```
5. Open a **pull request** against `main`. Describe what the change does and link any related issues.

Maintainers will review and may request changes before merging.
