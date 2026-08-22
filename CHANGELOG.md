# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `wasm-dump --diff` binary section diff comparison (#44, PR #55)
- `wasm-dump --wat` WAT text format output (#43, PR #54)
- `wasm-dump --stats` module statistics report (#45, PR #50)
- Integration tests with fixture wasm binaries (#46, #47, PR #51)
- Property-based tests via proptest for LEB128 and section_iter (#48, PR #52)
- Snapshot tests for all Display implementations via insta (#49, PR #53)
- Error handling tests: 42 tests covering all ParseError/ValidationError variants (#56–#59, PR #77)
- `cargo audit` job in CI for dependency vulnerability scanning (#67, PR #78)
- MSRV `rust-version = "1.65.0"` in `Cargo.toml` and CI MSRV check job (#69, PR #79)
- `keywords`, `categories`, `readme`, `exclude` fields in `Cargo.toml` for crates.io (#75, PR #81)

### Fixed
- `parse_header`: replaced `try_into().unwrap()` with direct byte indexing to eliminate hidden panics (#65, PR #76)
- `decode_const_expr`: same unwrap removal for f32/f64 byte array construction (#65, PR #76)

## [0.1.0] - 2026-08-18

### Added
- `src/module.rs`: `Module` struct, `parse_module()`, `validate()`, `ValidationError` (7 variants) (#25)
- Custom section (id=0) decoder with `name` / `producers` display (#34)
- Name section decoder — function name map (#39)
- Element section decoder — flag 0–7 support (#22, #31)
- DataCount section (id=12) decoder + data segment count validation (#33)
- Start section (id=8) decoder (#32)
- Data section (id=11) decoder (#23)
- Global section (id=6) decoder with `ConstExpr` init expression (#16, #24)
- Table section (id=4) decoder (#21)
- Memory section (id=5) decoder (#12)
- Import section (id=2) decoder (#11)
- Code section (id=10) function body decoder (#15)
- `wasm-dump --verbose` / `-v` flag: decoded content for every section (#17)
- `wasm-dump --validate` flag: cross-section consistency check output (#25)
- Type, Function, Export section decoders (#4)
- Section header iterator (`section_iter`) and LEB128 decoder (#3)
- Magic/version header parser and `ParseError` (16 variants) (#2)
- `wasm-dump` CLI binary (#5)
- CI: fmt + clippy + test jobs (#1)

[Unreleased]: https://github.com/flipslidersand/wasm-runtime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/flipslidersand/wasm-runtime/releases/tag/v0.1.0
