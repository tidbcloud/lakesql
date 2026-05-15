.PHONY: check build run test integration bump-major bump-minor bump-patch

default: build

lint: check

check:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo deny check
	# cargo install cargo-machete
	cargo machete
	# cargo install hawkeye
	hawkeye check
	# cargo install typos-cli
	typos

run:
	cargo run

build:
	cargo build --release

test:
	cargo test --all --all-features --lib -- --nocapture

integration:
	make -C tests

integration-down:
	make -C tests down

integration-core:
	make -C tests test-core

integration-driver:
	make -C tests test-driver

integration-lakesql:
	make -C tests test-lakesql

integration-bindings-python:
	make -C tests test-bindings-python

integration-bindings-nodejs:
	make -C tests test-bindings-nodejs

bump-major:
	./scripts/bump_version.py major

bump-minor:
	./scripts/bump_version.py minor

bump-patch:
	./scripts/bump_version.py patch
