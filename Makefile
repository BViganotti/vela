.PHONY: build release test spec fuzz bench studio check clean

build:
	cargo build

release:
	cargo build --release

test:
	cargo test
	cargo test --release

check:
	cargo build --release
	cargo test

spec:
	cargo run -p vela-spec -- tests/fixtures

fuzz:
	cargo test -p vela-core --test fuzz_modules

bench:
	cargo run --release -p vela-core --bin vela-bench

studio:
	cargo run -p vela-studio

clean:
	cargo clean
