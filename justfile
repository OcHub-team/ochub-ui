default: check

check:
    cargo check --all-targets --locked

test:
    cargo test --all-targets --locked

gallery:
    cargo run --example gallery

fmt:
    cargo fmt --all

ci:
    cargo fmt --all -- --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --all-targets --locked
