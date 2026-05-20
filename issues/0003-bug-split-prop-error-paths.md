# prop_error_paths.rs が命名規則に違反し複数モジュールのエラーパスを混在させている

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`pbt/tests/prop_error_paths.rs` (2398 行) が、複数モジュールのエラーパステストを
1 つのファイルにまとめており、AGENTS.md の命名規則と責務分離に違反している。

## 根拠

AGENTS.md の規約:
- 「PBT のファイル名は `pbt/tests/prop_<module>.rs` とし、`src/<module>.rs` に対応させること」
- 「テストファイルが長くなった場合はファイル内で `mod` を使って分割すること」

`prop_error_paths.rs` に混在しているモジュール:

| mod 名 | 対応する src モジュール |
|---|---|
| `avcc_error_tests` | boxes_sample_entry |
| `hvcc_error_tests` | boxes_sample_entry |
| `dfla_error_tests` | boxes_sample_entry |
| `dops_error_tests` | boxes_sample_entry |
| `esds_error_tests` | descriptors |
| `sample_entry_inner_box_tests` | boxes_sample_entry |
| `moov_tree_error_tests` | boxes_moov_tree |
| `descriptor_error_tests` | descriptors |
| `mux_error_tests` | mux |
| `base_box_tests` | basic_types |

`prop_error_paths.rs` というファイル名に対応する `src/error_paths.rs` は存在しない。

## 修正方針

各 mod を対応するモジュール別 PBT ファイルに分散する:

- `avcc_error_tests`, `hvcc_error_tests`, `dfla_error_tests`, `dops_error_tests`, `sample_entry_inner_box_tests` → `prop_boxes_sample_entry.rs` (新規) または既存 `prop_codec_boxes.rs` / `prop_additional_boxes.rs` に統合
- `moov_tree_error_tests` → `prop_boxes.rs` (既存) または `prop_boxes_moov_tree.rs` (新規)
- `esds_error_tests`, `descriptor_error_tests` → `prop_descriptors.rs` (既存)
- `mux_error_tests` → `prop_mux_demux.rs` (既存)
- `base_box_tests` → `prop_basic_types.rs` (既存)

既存ファイルの行数を確認して、追加か新規ファイル切るかを判断する必要がある。
