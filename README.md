# wasm-runtime

WebAssembly binary parser written in Rust — decodes every known section of the WebAssembly MVP binary format into typed structs.

Rust 製 WebAssembly バイナリパーサー。WebAssembly MVP バイナリフォーマットの全既知セクションを型付き構造体にデコードします。

## Features / 機能

- Validates the 8-byte wasm module header (magic + version) / 8 バイトの wasm モジュールヘッダー検証
- Iterates over all section headers via a lazy iterator / 遅延イテレーターによる全セクションヘッダーの走査
- Decodes **all known sections (ids 0–12)** into typed structs: Custom, Type, Import, Function, Table, Memory, Global, Export, Start, Element, Code, Data, DataCount / **全既知セクション（id 0–12）**を型付き構造体にデコード
- LEB128 (unsigned/signed) decoder with overflow and boundary checks / オーバーフロー・境界チェック付き LEB128 デコーダー
- Cross-section validation (`validate()`) / セクション横断バリデーション
- `wasm-dump` CLI: compact section list, verbose decoded output, validation / `wasm-dump` CLI

## Directory structure / ディレクトリ構成

```
src/
  lib.rs          — crate root
  parser.rs       — header parser, LEB128 decoders, section_iter, ParseError
  sections.rs     — decoders and Display impls for all known sections
  module.rs       — Module struct, parse_module(), validate(), ValidationError
  bin/
    wasm-dump.rs  — CLI entry point
tests/
  error_handling.rs  — ParseError/ValidationError coverage
```

## Requirements / 必要環境

- Rust ≥ 1.65.0 (edition 2021)

## Build / ビルド

```bash
cargo build --release
```

## Usage / 使い方

```bash
wasm-dump path/to/file.wasm            # compact section list / セクション一覧
wasm-dump --verbose path/to/file.wasm  # decoded contents / デコード出力
wasm-dump --validate path/to/file.wasm # cross-section validation / バリデーション
```

### Example output / 出力例

```
magic: 0x6D736100, version: 1
sections:
  [ 1] type     size=7  (1 types)
  [ 3] func     size=2  (1 funcs)
  [ 7] export   size=7  (1 exports)
  [10] code     size=9  (1 funcs)
```

## Test / テスト

```bash
cargo test
```

184 tests covering header parsing, LEB128 decoding, section iteration, all section decoders, cross-section validation, and error handling.

ヘッダーパース・LEB128 デコード・全セクションデコーダー・セクション横断バリデーション・エラーハンドリングを網羅する 184 件のテスト。

## License

MIT
