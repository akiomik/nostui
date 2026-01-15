default: build test 

build:
  cargo build --all-features --workspace

lint:
  cargo clippy --all-targets --all-features --workspace -- -D warnings

test:
  cargo test --all-features --workspace

test-cov:
  cargo llvm-cov --all-features --workspace

doc:
  cargo doc --no-deps --document-private-items --all-features --workspace --examples

fmt:
  cargo fmt --all

run:
  cargo run

[macos]
log-tailf:
  tail -f ~/Library/Application\ Support/io.0m1.nostui/nostui.log
