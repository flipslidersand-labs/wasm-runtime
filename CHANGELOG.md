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
- Error handling tests: 42 tests covering the ParseError/ValidationError variants that existed at the time (#56–#59, PR #77)
- `cargo audit` job in CI for dependency vulnerability scanning (#67, PR #78)
- MSRV `rust-version = "1.65.0"` in `Cargo.toml` and CI MSRV check job (#69, PR #79)
- `keywords`, `categories`, `readme`, `exclude` fields in `Cargo.toml` for crates.io (#75, PR #81)
- Memory limits validation: `min`/`max` above 65536 pages is rejected with `ValidationError::MemoryLimitExceeded` (#185, PR #189)
- `wasm-dump --explain` byte-level annotation of the binary (#61, PR #89)
- `wasm-dump --max-size <bytes>` input size limit, default 256 MiB (268435456 bytes); oversized input exits 1, also applies to both `--diff` inputs (PR #182)
- `wasm-dump --explain-max-lines <N>` overrides the `--explain` output line cap (default 100000 unchanged, `0` = unlimited, which can use a lot of memory on large inputs) (#212)
- `wasm_runtime::prelude` module re-exporting the common types and parse functions (#73, PR #83)
- `wasm-dump` flags can be combined and given in any order (`--verbose --validate --stats`, etc.); unknown flags now print `error: unknown flag: --foo` (#72, PR #87)
- `ParseErrorContext` (byte offset and section id) and `parse_module_with_context()` (#60, PR #85)
- `parse_module_with_events()` with `SectionEvent` callback (#62, PR #95)
- Function bodies are scanned as instruction sequences; malformed bodies are rejected with `ParseError::{UnknownOpcode, MissingEnd, TooManyLocals}`, and `memory.init` / `data.drop` without a datacount section is rejected by `Module::validate()` with `ValidationError::DataCountRequired` (PR #160)
- `ParseError::SectionOutOfOrder`: non-custom sections that are out of order or duplicated are rejected; unknown section ids are still accepted (PR #155)
- `ValidationError::DataMemoryIndexOutOfRange`: active data segment with a non-existent memory index (PR #173)
- `ValidationError::LimitsMinGreaterThanMax`: memory/table limits with `min > max` (PR #174)
- Conformance tests against the official WebAssembly testsuite, with a tracked baseline of expected failures; skipped unless `SPEC_TESTSUITE_DIR` and `wasm-tools` are available (PR #151)
- Architecture Decision Records for three design decisions, listed in the README (PR #149)
- `parse_module_with_options` and `ParseOptions`: a single entry point that combines the section-event hook with `ParseErrorContext` errors; `parse_module`, `parse_module_with_events` and `parse_module_with_context` are unchanged thin wrappers (#196, PR #213)

### Changed
- `ValidationError` is now `#[non_exhaustive]`: downstream `match` needs a wildcard arm (#191)
- **Breaking:** `ParseError` is not `#[non_exhaustive]`, so the variants added in this release (`SectionOutOfOrder` in PR #155, `UnknownOpcode` / `MissingEnd` / `TooManyLocals` in PR #160) break exhaustive downstream `match` expressions
- **Breaking:** function bodies containing opcodes outside the supported set (MVP, sign-extension, basic reference types, `0xFC` prefix; e.g. SIMD, threads, exception handling, tail call) are now rejected at parse time with `UnknownOpcode` instead of being accepted unread (PR #160)
- **Breaking:** a section whose declared size is larger than its decoded content is now rejected with `SizeMismatch`, whose message is generalized to "declared size does not match payload" (PR #156)
- **Breaking:** `DiffStatus::Equal` is now `Equal { size: u32 }` instead of a unit variant; `Display` prints the actual byte count instead of `(0 bytes)` (#64, PR #84)
- **Breaking:** modules with out-of-order or duplicated non-custom sections, previously accepted, are now rejected with `SectionOutOfOrder` (PR #155)
- **Breaking:** init expressions are unified to the typed `ConstExpr`: `Global.init_expr: Vec<u8>` becomes `Global.init`, and `offset_expr` of active data/element segments becomes `offset` (#70, PR #96)
- Names printed by `wasm-dump` and `Display` impls now escape control characters, `\`, `"`, C1 controls, and bidi/line-separator characters as `\u{..}` (PR #181, #188, #197)
- `wasm-dump` exits 1 and reports `(decode error: ..)` when a section fails to decode; `--explain` keeps exit 0 (PR #175)

### Fixed
- `parse_header`: replaced `try_into().unwrap()` with direct byte indexing to eliminate hidden panics (#65, PR #76)
- `decode_const_expr`: same unwrap removal for f32/f64 byte array construction (#65, PR #76)
- Use `checked_add` for length/offset arithmetic in `read_name`, `read_byte_vec`, export/code section decoders and `explain` so oversized LEB128 lengths no longer overflow `usize` on 32-bit targets (#202)
- `--explain` output is capped at 100,000 lines (a final `output truncated` line marks the cut) and hex previews no longer format whole payloads; section decoders pre-allocate at most 1 MiB, accounting for element size, so crafted counts cannot inflate memory (#200, PR #211)
- `section_iter` validates the declared section size against the remaining bytes, returning `UnexpectedEof` instead of panicking on truncated input (#111, PR #126)
- Section decoders clamp LEB128 `count` allocation by payload length, so a huge count with a tiny payload no longer over-allocates (#119, PR #136)
- `decode_leb128_signed` rejects non-sign-extended unused bits in the final byte with `Leb128Overflow` (PR #171)
- `diff_modules` validates the magic/version header (PR #170)
- `--explain` no longer panics when a locals declaration overruns the function body (PR #163); it now also annotates the overrun and resumes at the body end instead of misaligning the following bodies (#201, PR #215)
- **Behavior change:** `ParseErrorContext` returned by `parse_module_with_context` / `parse_module_with_options` now carries `offset` and `section_id` for errors raised while iterating sections (header, order, size); they were `None` before. `offset` is the section id byte position for these errors and the payload start for body-decode errors (#201, PR #215)
- WAT output: import/export names are escaped as WAT strings (PR #172); table `max` no longer fuses with the reftype, e.g. `10funcref` (#138, PR #158)

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

[Unreleased]: https://github.com/flipslidersand-labs/wasm-runtime/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/flipslidersand-labs/wasm-runtime/releases/tag/v0.1.0
