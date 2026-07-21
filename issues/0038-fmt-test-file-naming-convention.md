# tests/decode_encode_test.rs が命名規則 test_<module>.rs に違反している

- Priority: Low
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fmt-test-file-naming-convention
- Polished: 2026-07-20

## 目的

`tests/` 配下のテストファイル命名規則 `test_<module>.rs` に対し、実際は `decode_encode_test.rs` となっている。`pbt/tests/` 配下は全て `prop_<module>.rs` 規則に準拠している。

## 優先度根拠

機能的な影響はないが、命名規則の一貫性の観点から修正すべき。

## 現状

`tests/` ディレクトリには `decode_encode_test.rs` のみ存在し、`test_decode_encode.rs` ではない。

## 完了条件

- `tests/test_decode_encode.rs` にリネームされること
- 既存のテストが通ること

## 解決方法

`git mv tests/decode_encode_test.rs tests/test_decode_encode.rs` でリネームする。
