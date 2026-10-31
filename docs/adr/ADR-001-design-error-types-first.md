# ADR-001: エラー型（ParseError / ValidationError）を、実装より先に設計する

**ステータス:** Draft  
**決定日:** 未確認（遡及記録: 2026-10-28）  
**review_by:** 2026-11-17

> 雛形: [ADR-000-public-template](https://github.com/flipslidersand-labs/repo-compass/blob/master/docs/adr/ADR-000-public-template.md)。遡及して書いた記録で、当時の比較経緯は推定していない。

---

## 課題

WebAssembly のバイナリは、セクションごとに構造が違い、壊れ方も多い。パーサーの正しさを保証するために、
「どこが壊れたら、どう失敗させるか」を先に決める必要があった。

## 決定

デコードの失敗（`ParseError`）と、セクションをまたぐ整合性の違反（`ValidationError`）を、別の型として先に設計する。
任意のバイト列に対して、`parse_module` はパニックせず、`Ok` か `Err` だけを返す。

## 理由

- 失敗の種類を型で表すと、テストが「どの失敗か」を検証でき、利用者も失敗を区別して扱える。
- 単一のセクションでは見つからない不整合（例: Function 宣言と Code 本体の数の不一致）は、
  デコードの失敗とは別に、`validate()` が検知する。
- **却下した代替案:** 比較した記録はない（作者の記述による。当時の検討は未確認）。

## 影響

- エラーの種類が増えるたびに、型とテストを足す必要がある。
- 「パニックしない」性質は、`tests/proptest_suite.rs` の任意バイト列の検証で確認している。

## 再検討条件

エラー型の細分化が使われないと分かったとき、または別の失敗の扱い方（例: 部分的なデコードを許す）が必要になったとき。

## 関連

- `src/parser.rs`（`ParseError`）、`src/module.rs`（`ValidationError`、`validate()`）
