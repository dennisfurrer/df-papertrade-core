# The gate before any commit.
check: fmt clippy wasm test features

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Both crates build for the browser and Workers.
wasm:
    cargo clippy --workspace --target wasm32-unknown-unknown -- -D warnings

test:
    cargo test --workspace

# Signing is a feature: the protocol builds without the curve, and only asks for it with `sign`.
features:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo clippy -p hl-papertrade -- -D warnings
    if cargo tree -p hl-papertrade -e no-dev --target all | grep -q ' k256 v'; then
        echo "hl-papertrade pulls in k256 without asking to sign" >&2
        exit 1
    fi
