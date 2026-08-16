# wasm-runtime

WebAssembly binary parser written in Rust — MVP implementation.

## Features

- Validates the 8-byte wasm module header (magic + version)
- Iterates over all section headers via a lazy iterator
- Decodes Type, Function, and Export sections into typed structs
- `wasm-dump` CLI: prints sections, exports, and (with `--verbose`) type signatures

## Directory structure

```
src/
  lib.rs          — crate root
  parser.rs       — header parser, LEB128 decoder, section iterator, ParseError
  sections.rs     — Type / Function / Export section decoders
  bin/
    wasm-dump.rs  — CLI entry point
```

## Requirements

- Rust stable (edition 2021)

## Build

```bash
cargo build --release
```

## Usage

```bash
# Basic
cargo run --bin wasm-dump -- path/to/file.wasm

# With type signatures
cargo run --bin wasm-dump -- --verbose path/to/file.wasm
```

### Example output

```
magic: 0x6D736100, version: 1
sections:
  [ 1] type     size=7  (1 types)
  [ 3] func     size=2  (1 funcs)
  [ 7] export   size=7  (1 exports)
  [10] code     size=9

exports:
  add              Func  [0]
```

## Test

```bash
cargo test
```

25 tests — header parsing, LEB128 decoding, section iteration, Type/Function/Export decoding.

## License

MIT
