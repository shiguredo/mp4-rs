# `pbt/` に混在する単体テストを `tests/` へ移す

- Created: 2026-08-21
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-move-mixed-pbt-unit-tests
- Polished: {YYYY-MM-DD}

## 目的

`pbt/tests/prop_*.rs` に noprop を使わない単体テストが混在しており、shiguredo-rust の「pbt 以下に unittest を書かないこと」規約に違反している。0075 で「中身が単体テストのみの 2 ファイル」は `tests/` へ移したが、同 issue の対象外だった「同一ファイル内の混在」を解消する。

## 現状

`#[test]` 関数を noprop (`Runner` / `sample_*`) 利用の有無で分類すると、`pbt/tests/` 全体で PBT 約 211 件に対し単体テスト約 185 件が混在している。

### 単体テストを含むファイルと移動先

対応する `src/<module>.rs` と既存 `tests/test_<module>.rs` に合わせる。既存ファイルがある場合は合流し、無い場合は新設する。

| 元ファイル | 単体テスト概数 | 主な mod | 移動先 |
|---|---:|---|---|
| `prop_auxiliary.rs` | 25 | `error_cases` / `timestamp_tests` / `error_display_tests` 等 | 既存 `tests/test_auxiliary.rs` |
| `prop_basic_types.rs` | 18 | `boundary_tests`（固定入力分）/ `codec_box_boundary_tests` | `boundary_tests` 固定入力 → 既存 `tests/test_basic_types.rs`、`codec_box_boundary_tests` → 既存 `tests/test_boxes_sample_entry.rs` |
| `prop_boxes.rs` | 22 | `boundary_tests` | 既存 `tests/test_boxes_moov_tree.rs` |
| `prop_additional_boxes.rs` | 13 | `boundary_tests` / `root_box_tests` | `FreeBox` / `MdatBox` / `UnknownBox` / `Brand` / `RootBox` → 既存 `tests/test_boxes.rs`、SampleEntry 固定例（`opus_box_minimal` 等）→ 既存 `tests/test_boxes_sample_entry.rs` |
| `prop_codec_boxes.rs` | 24 | `boundary_tests` / `*_error_tests` | 既存 `tests/test_boxes_sample_entry.rs` |
| `prop_container_boxes.rs` | 29 | `boundary_tests` | 既存 `tests/test_boxes_moov_tree.rs` |
| `prop_descriptors.rs` | 16 | `boundary_tests` / `descriptor_error_tests` | 新設 `tests/test_descriptors.rs` |
| `prop_fmp4_boxes.rs` | 27 | `boundary_tests` | 既存 `tests/test_boxes_fmp4.rs` |
| `prop_mux_demux.rs` | 11 | `boundary_tests` / `estimate_moov_size_tests`（固定入力分）/ `mux_error_tests` | 新設 `tests/test_mux_mp4_file.rs` |

単体テストを含まないファイル（`prop_bitstream_vp8.rs` / `prop_bitstream_vp9.rs` / `prop_demux.rs` / `prop_fmp4_segment_mux_demux.rs` / `prop_mp4_file_kind_detector.rs`）は変更しない。

### 同一 mod 内で PBT と単体が混在している箇所

mod ごと移動できないため、単体テスト関数だけを切り出す。

- `prop_auxiliary.rs` の `composition_time_offset_tests`:
  - 残す PBT: `composition_time_offset_matches_ctts_entries`
  - 移す単体: `composition_time_offset_returns_none_without_ctts`
- `prop_basic_types.rs` の `boundary_tests`:
  - 残す PBT: `box_size_u64_boundary` / `box_size_large_payload`
  - 移す単体: 上記以外の固定入力テスト
- `prop_mux_demux.rs` の `estimate_moov_size_tests`:
  - 残す PBT: `estimate_returns_non_negative` 等 4 件
  - 移す単体: `estimate_empty_tracks` / `estimate_single_track_no_samples` / `estimate_large_sample_count`

### 0075 との関係

0075 は「単体テストのみの 2 ファイル」と `common.rs` 配置だけを対象とし、本文「対象外」で他ファイルの規約適合を明示的に外していた。本 issue はその残件である。

## 設計方針

- テストの断言・入力は変えず、配置と必要最小限の import / ヘルパ移動だけ行う
- 単体テストが参照するファイルローカルヘルパは、移動先に同伴させる（PBT 側でも使うものは双方に残すか、移動先へ複製する。挙動は変えない）
- `pbt/tests/helpers.rs` は PBT 用のまま残す。単体側が必要なら移動先ファイル内に同等ヘルパを置く（本体 crate の `tests/` から `pbt` を参照しない）
- 「任意入力でパニックしない」系の PBT（例: `prop_codec_boxes.rs` の `*_decode_no_panic`）は fuzz 候補だが本 issue の対象外（配置は `pbt/` のまま）
- テスト内容の強化・削除・PBT 化は行わない

## 完了条件

- `pbt/tests/prop_*.rs` 内の `#[test]` がすべて noprop を用いる PBT になっている（上記対象外の PBT はそのまま）
- 移した単体テストが対応する `tests/test_*.rs` で実行される
- `cargo test --workspace` / `cargo fmt` / `cargo clippy --workspace --all-targets` が通る

## 解決方法

（実装後に追記する）
