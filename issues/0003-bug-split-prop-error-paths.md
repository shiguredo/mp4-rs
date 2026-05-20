# prop_error_paths.rs をモジュール別 PBT ファイルに分割する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`pbt/tests/prop_error_paths.rs` (2398 行, 115 テスト) が、複数モジュールのテストを
1 つのファイルにまとめており、AGENTS.md の命名規則と責務分離に違反している。

## 根拠

AGENTS.md の規約:
- 「PBT のファイル名は `pbt/tests/prop_<module>.rs` とし、`src/<module>.rs` に対応させること」
- 「テストファイルが長くなった場合はファイル内で `mod` を使って分割すること」

`prop_error_paths.rs` というファイル名に対応する `src/error_paths.rs` は存在しない。

## 分割計画

### 各 mod の内訳

| mod 名 | 行範囲 | 行数 | テスト数 | 分割先 |
|---|---|---|---|---|
| `avcc_error_tests` | 13-166 | 154 | 8 | `prop_codec_boxes.rs` |
| `hvcc_error_tests` | 170-273 | 104 | 4 | `prop_codec_boxes.rs` |
| `dfla_error_tests` | 277-293 | 17 | 1 | `prop_codec_boxes.rs` |
| `dops_error_tests` | 297-317 | 21 | 1 | `prop_codec_boxes.rs` |
| `esds_error_tests` | 321-337 | 17 | 1 | `prop_descriptors.rs` |
| `proptest!` ブロック | 341-374 | 34 | 5 | 各ボックス対応のファイルに分散 |
| `sample_entry_inner_box_tests` | 378-639 | 262 | 10 | `prop_additional_boxes.rs` |
| `moov_tree_error_tests` | 643-1079 | 437 | 23 | `prop_boxes.rs` |
| `descriptor_error_tests` | 1083-1237 | 155 | 10 | `prop_descriptors.rs` |
| `mux_error_tests` | 1241-1476 | 236 | 6 | `prop_mux_demux.rs` |
| `base_box_tests` | 1480-2398 | 919 | 46 | `prop_boxes.rs` |

### 分割先の推定行数

| 分割先 | 現在行数 | 追加行数 | 分割後推定行数 |
|---|---|---|---|
| `prop_codec_boxes.rs` | 638 | ~330 | ~968 |
| `prop_descriptors.rs` | 256 | ~172 | ~428 |
| `prop_additional_boxes.rs` | 1045 | ~262 | ~1307 |
| `prop_boxes.rs` | 1097 | ~1356 | ~2453 |
| `prop_mux_demux.rs` | 1122 | ~236 | ~1358 |

`prop_boxes.rs` が 2453 行に達するため、分割後にファイル内で `mod` を使って分割する。
具体的には `moov_tree_error_tests` と `base_box_tests` を `mod` ブロックとして分離し、
将来的な `prop_boxes_moov_tree.rs` / `prop_boxes_base_box.rs` への分離を見据える。

## 注意事項

### `sample_entry_inner_box_tests` と `base_box_tests` はエラーパステストではない

- `sample_entry_inner_box_tests`: SampleEntry の `box_type()`, `is_unknown_box()`, `children()` のテスト
- `base_box_tests`: BaseBox トレイトの実装テスト (MoovBox, TrakBox 等のコンテナボックス)

エラーパスではないため、エラーパステストと混在させず、既存の正常系テストに統合する。
`sample_entry_inner_box_tests` を `prop_additional_boxes.rs` に統合する理由:
`prop_additional_boxes.rs` は既に `sample_entry_tests` モジュールを含んでおり、SampleEntry 関連のテストが集約されている。

### `proptest!` ブロックの扱い

行 341-374 の `proptest!` ブロックは `AvccBox`, `HvccBox`, `DflaBox`, `DopsBox`, `EsdsBox` の
ランダムバイト列デコードのパニック安全性テスト。各ボックスに対応する分割先ファイルに移動する。

`prop_error_paths.rs` の `ProptestConfig` は `with_cases(50)` だが、
分割先の `prop_codec_boxes.rs` は `with_cases(200)`、`prop_descriptors.rs` は `with_cases(200)` を使用している。
分割時に既存の `proptest!` ブロックに統合すると cases 数が変化するため、個別ブロックとして分離するか、
既存ブロックに統合して cases 数を 200 に合わせるかを判断する。

### ヘルパー関数の重複

`create_avc1_box()`, `create_hvcc_box()`, `create_opus_box()` 等が複数 mod で定義されている。
分割後は各ファイルに同じヘルパーが散在するが、AGENTS.md の「モックやスタブを使わない」規約に従い、
テストユーティリティモジュールへの抽出は行わず、各ファイルに複製する。

### import 文の移動

各 mod は `use super::*;` で親モジュールの import を使用している。
分割後は各ファイルで必要な import を追加する。既存ファイルの import との重複は `use` 文を統合して解消する。

## 修正手順

1. 各 mod を対応する既存 PBT ファイルに移動する
2. 各ファイルでテストを実行し、パスすることを確認する
3. `prop_error_paths.rs` を削除する
4. 全テストを再実行して確認する

## テスト実行コマンド

```bash
cargo test -p pbt --test prop_codec_boxes
cargo test -p pbt --test prop_descriptors
cargo test -p pbt --test prop_additional_boxes
cargo test -p pbt --test prop_boxes
cargo test -p pbt --test prop_mux_demux
```

## CHANGES.md

テストファイルの分割は機能に影響しないリファクタリングのため、`### misc` に `[UPDATE]` で記載する。
