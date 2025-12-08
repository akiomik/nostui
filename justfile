default: build test 

build:
  cargo build --all-features --workspace

lint:
  cargo clippy --all-targets --all-features --workspace -- -D warnings

test:
  cargo test --all-features --workspace
  cargo doc --no-deps --document-private-items --all-features --workspace --examples

fmt:
  cargo fmt --all --check
