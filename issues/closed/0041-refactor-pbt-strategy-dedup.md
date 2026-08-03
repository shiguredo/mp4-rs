# pbt/tests の prop_additional_boxes.rs と prop_codec_boxes.rs で Strategy 定義が重複している

- Priority: Low
- Created: 2026-07-20
- Completed: 2026-07-28
- Model: qwen3.8-max-preview
- Branch: feature/refactor-pbt-strategy-dedup
- Polished: 2026-07-20

## 目的

`arb_hvcc_box`、`arb_vpcc_box`、`arb_av1c_box`、`arb_dops_box`、`arb_esds_box` の Strategy 関数が `prop_additional_boxes.rs` と `prop_codec_boxes.rs` の両方でほぼ同一の内容で重複定義されている。`prop_additional_boxes.rs` 側は一部フィールドを固定値にしている簡略版だが、構造は同一。`arb_avcc_box` 系は `prop_codec_boxes.rs` 側では `arb_avcc_box_baseline` / `arb_avcc_box_high` と名前が異なり、完全な重複ではなく類似。

## 優先度根拠

機能的な影響はないが、Strategy の修正時に両方を更新する必要があり修正漏れのリスクがある。

## 現状

- `pbt/tests/prop_additional_boxes.rs:81-237`: 簡略版 Strategy 群（`arb_dops_box` 81 行目、`arb_esds_box` 93 行目、`arb_avcc_box` 143 行目、`arb_hvcc_box` 以下 237 行目付近まで）
- `pbt/tests/prop_codec_boxes.rs:13-326`: 完全版 Strategy 群

## 設計方針

共通の Strategy を `pbt/tests/common/` モジュールに集約し、両ファイルから参照する。`pbt` クレートはテスト専用クレートであり lib ターゲットが存在しないため、`pbt/src/lib.rs` ではなく `pbt/tests/common/` に配置する。

`prop_additional_boxes.rs` 側の簡略版は、完全版の Strategy に `.prop_map()` で固定値を上書きする形に変換する。`arb_dops_box` は両ファイルでほぼ同一（固定値なし）であり、単純に削除して共通版を参照するだけで済む。

## 完了条件

- 共通の Strategy が 1 箇所に集約されること
- 既存のテストが通ること（`cargo test -p pbt`）
- `cargo clippy` が通ること

## 解決方法

非対応として closed する。

### 非対応の理由

- 削減できる重複は約 130 行、共通化後もファイル総量 2004 行に対して差し引き 50〜80 行程度の圧縮にとどまり、効果が小さい
- 「Strategy 修正時の修正漏れリスク」は、フィールド追加や型変更ならコンパイラが両方の Strategy でエラーを検出するため、実質的には値域変更程度に限られる
- 完全版に `.prop_map()` で固定値を上書きする方式は、直接構造体を構築している現状より、なぜその値が固定なのか（狭い探索空間で速く回す等の意図）が読み取りにくくなる懸念がある
- Rust の integration test はファイルごとに別クレートとしてコンパイルされるため、`arb_dops_box` だけ移すような部分対応でも `pbt/tests/common/mod.rs` の新設と `mod common;` の追加という同じセットアップコストがかかり、部分対応の費用対効果も悪い
- 以上より、Priority Low のまま放置するより明示的に non-対応で closed する方が管理コストが低いと判断した
