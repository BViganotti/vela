# Release checklist

Vela has no CI. This checklist is the manual gate used for the 0.1.0 release.

## Build and tests

```sh
cargo build --release            # must print zero warnings
cargo test                       # must pass
cargo test --release             # must pass
cargo fmt --all -- --check       # formatting is stable
```

Expected: 37 test targets, 258 passing tests, zero warnings.

## Correctness gates

```sh
cargo test -p vela-core --test fuzz_modules   # 1000 mutations + 1000 random byte strings
cargo test -p vela-core --test fuzz_text      # 1000 WAT mutations + 500 wast mutations
cargo run -p vela-spec -- tests/fixtures      # 41/41 local assertions
cargo test -p vela-replay --test replay       # 100 replays, 500 rewind checkpoints
```

No test may be `#[ignore]`d.

## Studio gate

```sh
cargo build --release -p vela-studio
VELA_NO_OPEN=1 ./target/release/vela-studio   # manual smoke
python3 studio-tests/test_studio.py           # Playwright: run, breakpoint, step, rewind, spec, diff, SSE
```

## Benchmarks

```sh
cargo run --release -p vela-core --bin vela-bench
```

Record the output in `docs/benchmarks.md` if it changed materially. Do not
publish numbers from debug builds.

## Repository hygiene

- `Cargo.lock` contains only the four workspace crates (no external packages).
- No `.github/workflows` directory.
- `grep -r unsafe crates` returns nothing.
- `README.md`, `docs/`, `LIMITATIONS.md`, `CHANGELOG.md`, `LICENSE` and
  `FINAL_REPORT.md` exist and are current.
- The repository is committed and pushed.

## Honesty checks

- Do not claim official spec-suite coverage; only local fixtures are run.
- Do not hide excluded proposals; `LIMITATIONS.md` lists them.
- Do not publish fabricated benchmarks; quote the measured run.
