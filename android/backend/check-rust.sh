#!/bin/bash

set -e

# Non-zero exit on warnings
export RUSTFLAGS="-Dwarnings"

rustup component add clippy && cargo clippy
(cd ../../cargo/format && cargo fmt --check --all --manifest-path ../../android/backend/Cargo.toml)