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
- `--stats`: module summary statistics / モジュールのサマリー統計
- `--wat`: WAT (WebAssembly Text format) output / WAT（WebAssembly テキスト形式）出力
- `--timing`: parse time measurement / パース時間の計測
- `--explain`: byte-level annotated explanation of the module / モジュールのバイト単位の注釈付き解説
- `--diff`: structural diff between two wasm modules / 2つの wasm モジュール間の構造的差分

## Directory structure / ディレクトリ構成

```
src/
  lib.rs          — crate root
  prelude.rs      — common re-exports
  parser.rs       — header parser, LEB128 decoders, section_iter, ParseError
  module.rs       — Module struct, parse_module(), validate(), ValidationError
  sections/       — decoders and Display impls for each known section
    mod.rs        — section dispatch, apply_section()
    custom.rs, data.rs, element.rs, export.rs, func.rs,
    global.rs, import.rs, misc.rs, types.rs
  diff.rs         — module diffing
  explain.rs      — annotated/explained decoding output
  stats.rs        — section/size statistics
  wat.rs          — WAT (WebAssembly Text) rendering
  bin/
    wasm-dump.rs  — CLI entry point
tests/
  cli.rs, integration.rs, error_handling.rs, error_context.rs,
  section_events.rs, snapshot_tests.rs, proptest_suite.rs
```

## Design decisions / 設計判断（ADR）

- [Design error types first](docs/adr/ADR-001-design-error-types-first.md) — エラー型を先に設計
- [Property-test LEB128](docs/adr/ADR-002-property-test-leb128.md) — LEB128 をプロパティテストで検証
- [Unify const expressions](docs/adr/ADR-003-unify-const-expr.md) — 定数式を `ConstExpr` に統一

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
wasm-dump --stats path/to/file.wasm    # summary statistics / サマリー統計
wasm-dump --wat path/to/file.wasm      # WAT text output / WAT テキスト出力
wasm-dump --timing path/to/file.wasm   # parse time measurement / パース時間計測
wasm-dump --explain path/to/file.wasm  # byte-level annotated explanation / バイト単位の注釈付き解説
wasm-dump --diff a.wasm b.wasm         # structural diff between two modules / 2モジュール間の構造的差分
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

Tests covering header parsing, LEB128 decoding, section iteration, all section decoders, cross-section validation, and error handling.

ヘッダーパース・LEB128 デコード・全セクションデコーダー・セクション横断バリデーション・エラーハンドリングを網羅するテスト。

## Conformance / 仕様適合性

Measured against the official [WebAssembly testsuite](https://github.com/WebAssembly/testsuite) (commit `44f1503`, 11 files: binary, custom, data, elem, func, global, imports, memory, start, table, type). Needs `wasm-tools`; skipped otherwise.
公式 testsuite（コミット `44f1503`、11 ファイル）で測定。`wasm-tools` が必要で、無ければ skip。

```bash
git clone --depth 1 https://github.com/WebAssembly/testsuite.git /tmp/testsuite
SPEC_TESTSUITE_DIR=/tmp/testsuite cargo test --test spec_conformance -- --nocapture
```

| | Result / 結果 | Note / 備考 |
| --- | --- | --- |
| Valid modules accepted / 正しいモジュールの受理 | 210 / 247 | All 37 failures use post-MVP features (reference types, extended const, tags) / 失敗 37 件は全て MVP 後の拡張 |
| Malformed binaries rejected / 壊れたバイナリの拒否 | 99 / 120 | 5 unknown section ids are accepted on purpose (shown as `unknown` by the dump tool); 16 remain MVP-strictness candidates (#150) / 不明なセクション ID 5 件は dump ツールが表示するため意図的に受理、残り 16 件は MVP の厳格さの穴の候補（#150） |

The failure set is pinned in `tests/spec_expected_failures.txt`; the test fails when it changes (regression or an unrecorded improvement).
失敗の集合は `tests/spec_expected_failures.txt` に固定し、変わるとテストが落ちる。

## License

MIT
