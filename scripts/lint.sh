#!/usr/bin/env bash

# Run from the root
cd $(dirname ${BASH_SOURCE[0]})/..

cargo fmt --all --check --
cargo clippy --all-targets --all-features -- -D warnings
