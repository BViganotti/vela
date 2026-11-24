#!/bin/sh
# Deterministic fuzz sweep. No network, no external tools.
set -eu
cd "$(dirname "$0")/.."
cargo test -p vela-core --test fuzz_modules
cargo test -p vela-core --test fuzz_text
