check: fmt clippy wasm test doc

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

wasm:
    cargo clippy --workspace --target wasm32-unknown-unknown -- -D warnings

# Unit and mocked suites; the live suite is skipped here.
test:
    cargo test --workspace

# Against mainnet, read only: every relayer read, the account stream and the HyperEVM views,
# each held against its recording. Run by hand.
live:
    cargo test -p df-papertrade-core --test live -- --ignored --test-threads 1

# Line coverage of everything but the live suite.
cover:
    cargo llvm-cov -p df-papertrade-core --summary-only

doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
