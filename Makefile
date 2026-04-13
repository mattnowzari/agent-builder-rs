clean:
	cargo clean

check:
	cargo check

clippy:
	cargo clippy

lint:
	rustfmt src/main.rs --style-edition 2024 --check

autoformat:
	rustfmt src/main.rs --style-edition 2024

test:
	cargo test

build:
	cargo build

clean-build:
	cargo clean
	cargo build

build-release:
	cargo clean
	cargo build --release
