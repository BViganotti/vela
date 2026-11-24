# Roadmap and future work

This is a list of honest next steps, not promises. Vela v1 is intentionally
scoped as documented in `docs/scope.md` and `LIMITATIONS.md`.

## Specification and tests

- Vendor and run the official WebAssembly core test suite when it can be added
  without violating the no-vendored-code rule, or fetch it explicitly in a
  local-only harness. Publish a pass/fail matrix per proposal.
- Extend `.wast` support for `assert_exception` and annotated scripts if the
  relevant proposals are ever in scope.
- Add differential tests against another engine for a curated corpus.

## Runtime

- Reduce checkpoint cost with copy-on-write memory pages or delta snapshots.
- Add an instruction cache for decoded block signatures (currently resolved per
  entry).
- Improve numeric code with measured micro-optimizations, never at the cost of
  bounds checks.
- Consider a second memory or memory64 only if multi-memory becomes a target.
- Expose a host-side fuel function and per-instance resource accounting.

## Text format

- Full WAT grammar: inline data/element abbreviations, annotations, quoted
  identifiers, name section emission.
- Syntax highlighting and diagnostics in the Studio editor.
- A `.wat` formatter using the existing printer.

## Studio

- Virtualized trace list for multi-million-step runs.
- Snapshot diff view (memory/table maps between two steps).
- Per-instance inspection when multiple modules are linked.
- A proper CSP-safe worker for parsing large modules off the UI thread.
- Export trace bundles with module bytes, arguments, host seed and hashes.

## Product

- Stable embedding examples and a small API guide for third-party hosts.
- Optional headless recording API that writes trace bundles to disk for replay
  on another machine.
- More benchmarks, especially real-world modules, with reproducible scripts.

## Explicit non-goals

SIMD, threads/atomics, GC, exception handling, tail calls, multi-memory,
memory64, the component model, WASI, JIT and CI/CD workflows remain out of scope
for this project unless the scope changes deliberately.
