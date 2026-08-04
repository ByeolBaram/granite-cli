#!/usr/bin/env bash

# Run from the root
cd $(dirname ${BASH_SOURCE[0]})/..

# Get the minimum version of rust from Cargo.toml
rust_version=$(grep "rust-version" Cargo.toml | cut -d'=' -f2 | sed 's,[ "],,g')

cargo +$rust_version fmt --all --check --
cargo +$rust_version clippy --all-targets --all-features -- -D warnings
