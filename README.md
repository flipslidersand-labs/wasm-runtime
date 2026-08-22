# wasm-runtime

WebAssembly binary parser written in Rust — decodes every known section of the WebAssembly MVP binary format into typed structs.

## Features

- Validates the 8-byte wasm module header (magic + version)
- Iterates over all section headers via a lazy iterator
- Decodes **all known sections (ids 0–12)** into typed structs:
  Custom, Type, Import, Function, Table, Memory, Global, Export, Start, Element, Code, Data, DataCount
- LEB128 (unsigned/signed) decoder with overflow and boundary checks
- Cross-section validation (`validate()` checks type/export/element/start/datacount consistency)
- `wasm-dump` CLI: compact section list, verbose decoded output, and module validation

## Directory structure

```
src/
  lib.rs          — crate root (pub mod parser, sections, module)
  parser.rs       — header parser, LEB128 decoders, section_iter, ParseError
  sections.rs     — decoders and Display impls for all known sections (Type … DataCount)
  module.rs       — Module struct, parse_module(), validate(), ValidationError
  bin/
    wasm-dump.rs  — CLI entry point
tests/
  error_handling.rs  — ParseError/ValidationError coverage and Display message tests
```

## Requirements

- Rust ≥ 1.65.0 (edition 2021)

## Build

```bash
cargo build --release
```

## Usage

```bash
# Compact section list
wasm-dump path/to/file.wasm

# Decoded section contents
wasm-dump --verbose path/to/file.wasm
wasm-dump -v path/to/file.wasm

# Cross-section validation
wasm-dump --validate path/to/file.wasm

# Via cargo
cargo run --bin wasm-dump -- [--verbose|-v | --validate] path/to/file.wasm
```

### Example output

```
magic: 0x6D736100, version: 1
sections:
  [ 1] type     size=7  (1 types)
  [ 3] func     size=2  (1 funcs)
  [ 7] export   size=7  (1 exports)
  [10] code     size=9  (1 funcs)
```

## Test

```bash
cargo test
```

184 tests — header parsing, LEB128 decoding, section iteration, all section decoders, cross-section validation, error handling (ParseError/ValidationError Display), and section_iter boundary conditions.

## License

MIT
