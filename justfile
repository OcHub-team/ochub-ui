default: check

check:
    cargo check --all-targets --locked

test:
    cargo test --all-targets --locked

gallery:
    cargo run --example gallery

wasm:
    cargo +nightly -Zbuild-std=std,panic_abort check --target wasm32-unknown-unknown --lib --example gallery --locked

fmt:
    cargo fmt --all

ci:
    cargo fmt --all -- --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --all-targets --locked
