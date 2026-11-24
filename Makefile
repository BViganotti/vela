.PHONY: build release test check spec fuzz bench studio studio-test examples fmt docs clean

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

examples:
	cargo test -p vela-core --test examples

fuzz:
	cargo test -p vela-core --test fuzz_modules
	cargo test -p vela-core --test fuzz_text

bench:
	cargo run --release -p vela-core --bin vela-bench

studio:
	cargo run -p vela-studio

studio-test:
	cargo build --release -p vela-studio
	python3 studio-tests/test_studio.py

fmt:
	cargo fmt --all

docs:
	@echo "docs/architecture.md docs/decoder.md docs/validation.md docs/interpreter.md"
	@echo "docs/text.md docs/replay.md docs/studio.md docs/security.md docs/testing.md"
	@echo "docs/benchmarks.md docs/scope.md docs/examples.md LIMITATIONS.md FINAL_REPORT.md"

clean:
	cargo clean
