# ADR-003: 定数式を `ConstExpr` という単一の型付き表現に統一する

**ステータス:** Draft  
**決定日:** 未確認（遡及記録: 2026-10-28）  
**review_by:** 2026-11-17

> 雛形: [ADR-000-public-template](https://github.com/flipslidersand-labs/repo-compass/blob/master/docs/adr/ADR-000-public-template.md)。遡及して書いた記録で、当時の比較経緯は推定していない。

---

## 課題

Global の初期値、Element・Data のオフセットなど、定数式を持つセクションが複数ある。
セクションごとに表現が違うと、扱いが分散する。

## 決定

定数式を `ConstExpr`（`src/sections/global.rs`）で表し、Global の `init`、Data の `offset` など、
定数式を持つ箇所で共通に使う。

## 理由

- 定数式の扱いを 1 か所にそろえると、表示（WAT 出力など）や検証の実装が重複しない。
- **却下した代替案:** 比較した記録はない（当時の検討は未確認）。

## 影響

- 新しい定数式の命令を足すときは、`ConstExpr` の 1 か所を直せばよい。
- 現在の `ConstExpr` が扱う命令の範囲は、MVP の定数式のうち実装済みのものに限る。

## 再検討条件

MVP より後の拡張（拡張定数式など）に対応するとき。

## 関連

- `src/sections/global.rs`、`src/wat.rs`
