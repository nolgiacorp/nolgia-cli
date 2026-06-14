.PHONY: e2e test build

build:
	cargo build --release

test:
	cargo test --workspace

e2e:
	cargo test --features e2e --test e2e_cli -p nolgia-cli -- --include-ignored
