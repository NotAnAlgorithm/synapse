#!/bin/bash

set -e

(cd ../../cargo/format && cargo fmt --all --manifest-path ../../android/backend/Cargo.toml)
