# Local `.wast` fixtures

These scripts are the local spec-style oracle. They are compiled into the Studio
dashboard and run by `cargo run -p vela-spec`.

| File | Assertions | Focus |
| --- | ---: | --- |
| `basic.wast` | 10 | arithmetic, factorial loop, globals, memory/data |
| `floats.wast` | 8 | NaN classes, nearest, signed zero, exhaustion |
| `invalid.wast` | 5 | validation vs malformed distinction |
| `linking.wast` | 5 | register/import across modules, unlinkable module |
| `tables.wast` | 6 | indirect calls, table size/grow, undefined element |
| `traps.wast` | 7 | memory, divide, overflow, unreachable, truncation |

Total: 41 assertions.

## What is checked

- `assert_return` compares exact values (floats by bit pattern; NaNs by class).
- `assert_trap` requires an actual trap of the expected stage.
- `assert_exhaustion` requires stack exhaustion.
- `assert_invalid` requires a validation failure; parse failures do not count.
- `assert_malformed` requires a parse/decode failure.
- `assert_unlinkable` requires an import/link failure.
- `register` exposes an instance's exports under a module name.

## What is not here

The official WebAssembly specification suite is not vendored. These fixtures are
a curated subset and should not be described as official coverage. Excluded
proposals (SIMD, threads, GC, exceptions, tail calls, multi-memory, WASI) are
documented in `LIMITATIONS.md`.

## Adding a fixture

1. Write the `.wast` script.
2. Add a test in `crates/vela-core/tests/spec.rs` calling `run_file("name.wast")`.
3. Add it to `FIXTURES` in `crates/vela-studio/src/assets.rs` for the dashboard.
4. Rebuild the Studio so `include_str!` embeds the new file.
