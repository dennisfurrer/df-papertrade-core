check: fmt clippy wasm test doc

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

wasm:
    cargo clippy --workspace --target wasm32-unknown-unknown -- -D warnings

test:
    cargo test --workspace

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
