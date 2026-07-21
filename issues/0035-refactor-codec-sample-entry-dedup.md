# boxes_sample_entry.rs の Hev1Box/Hvc1Box と Vp08Box/Vp09Box が完全に同一の構造・ロジックで重複している

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/refactor-codec-sample-entry-dedup
- Polished: 2026-07-20

## 目的

`Hev1Box` と `Hvc1Box`、`Vp08Box` と `Vp09Box` は、フィールド構成・`Encode`・`Decode`・`BaseBox` の実装がボックス種別定数（`b"hev1"` vs `b"hvc1"`、`b"vp08"` vs `b"vp09"`）を除いて完全に同一である。HEVC 対で約 150 行、VP 対で約 150 行のコピペ重複があり、将来的に HEVC / VP 関連のボックスに修正が必要になった場合、両方を同時に修正する必要があり修正漏れのリスクがある。

C API 層（`crates/c-api/src/boxes.rs`）にも `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の重複が伝播している。

## 優先度根拠

直ちにバグを引き起こすわけではないが、修正漏れリスクと可読性の観点から解消すべき技術的負債。

## 現状

- `src/boxes_sample_entry.rs:508-583`（Hev1Box）と `585-660`（Hvc1Box）: `visual: VisualSampleEntryFields` + `hvcc_box: HvccBox` + `unknown_boxes: Vec<UnknownBox>` で同一
- `src/boxes_sample_entry.rs:870-946`（Vp08Box）と `948-1020`（Vp09Box）: `visual` + `vpcc_box: VpccBox` + `unknown_boxes` で同一
- `crates/c-api/src/boxes.rs`: `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の `to_sample_entry()` と NALU 配列構築ロジックが重複。`Mp4SampleEntryVp08` / `Mp4SampleEntryVp09` も VpccBox 構築ロジックが類似するが、struct フィールドが異なる（Vp09 には `profile`/`level` がある）ため本 issue の対象外

## 設計方針

共通の内部構造体（`HevcSampleEntryInner` / `VpSampleEntryInner`）を抽出し、`Hev1Box` / `Hvc1Box` は `TYPE` 定数と内部構造体の薄いラッパーにする。マクロは使用しない（AGENTS.md / shiguredo-rust スキルの「マクロを作らないこと」規約に従う）。

C API 層も共通のヘルパー関数に抽出する。

### 後方互換性への影響

公開 API（`Hev1Box` / `Hvc1Box` / `Vp08Box` / `Vp09Box` の struct フィールド、`Encode` / `Decode` / `BaseBox` の trait impl）は不変。内部構造体の抽出は private な実装詳細であり、外部からの型参照・フィールドアクセスに影響しない。

## 完了条件

- 重複コードが解消されること
- 公開 API の後方互換性が保たれること
- 既存のテストが通ること
- 既存の PBT（ラウンドトリップテスト）が通ること
- `cargo clippy` が通ること

## 解決方法

コアライブラリで共通内部構造体を抽出し、C API 層で共通ヘルパー関数に抽出する。

## CHANGES.md

`[UPDATE]` で記載する（内部実装のリファクタリングであり、公開 API の変更はないため）。
