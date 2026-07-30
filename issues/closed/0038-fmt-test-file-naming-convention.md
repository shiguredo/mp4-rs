# tests/decode_encode_test.rs が命名規則 test_<module>.rs に違反している

- Priority: Low
- Created: 2026-07-20
- Completed: 2026-07-30
- Model: qwen3.8-max-preview
- Branch: develop
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

1. `git mv tests/decode_encode_test.rs tests/test_decode_encode.rs` でリネームした
2. `cargo test --test test_decode_encode` で既存 9 件のテストが通ることを確認した
3. ユーザー指示により作業ブランチではなく `develop` 上で直接対応した
