# Changelog

All notable changes to Vela are documented here. The format follows
Keep a Changelog and the project uses semantic versioning.

## [Unreleased]

### Added
- Cargo workspace with `vela-core`, `vela-replay`, `vela-studio` and
  `vela-spec`; no external dependencies.
- Bounded binary reader with LEB128 decoding, UTF-8 names and hard size limits.
- Text lexer and s-expression parser with nested comments and bounded depth.
- MIT license, workspace lints forbidding unsafe code.
