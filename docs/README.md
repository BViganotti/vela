# Vela documentation

Start here:

| Document | Contents |
| --- | --- |
| [architecture](architecture.md) | crates, data flow, soundness boundaries |
| [decoder](decoder.md) | binary format, limits, LEB128, encoding |
| [validation](validation.md) | module and function-body validation |
| [interpreter](interpreter.md) | frames, traps, numeric semantics, limits |
| [text](text.md) | WAT subset, printer, `.wast` commands |
| [WAT grammar](wat-grammar.md) | accepted grammar and instruction groups |
| [replay](replay.md) | traces, checkpoints, reverse stepping, breakpoints, diff |
| [determinism](determinism.md) | determinism contract and hashing |
| [imports](imports.md) | linking, type matching, instantiation order |
| [errors](errors.md) | error taxonomy and failure stages |
| [embedding](embedding.md) | using the libraries from Rust |
| [studio](studio.md) | security model, UI, HTTP/SSE API |
| [troubleshooting](troubleshooting.md) | common Studio and runtime issues |
| [security](security.md) | threat model and resource limits |
| [testing](testing.md) | test layers and commands |
| [verification](verification.md) | recorded build/test/fuzz/Studio transcript |
| [benchmarks](benchmarks.md) | measured performance |
| [examples](examples.md) | 20 example modules and expected results |
| [scope](scope.md) | supported features and non-goals |
| [roadmap](roadmap.md) | possible future work, not promises |

Project-level files: `README.md`, `LIMITATIONS.md`, `FINAL_REPORT.md`,
`CHANGELOG.md`, `LICENSE`.
