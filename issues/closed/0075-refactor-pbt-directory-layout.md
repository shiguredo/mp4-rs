# `pbt/` の shiguredo-rust 規約違反 (common.rs 配置 / `prop_` 命名) を解消する

- Created: 2026-08-19
- Completed: 2026-08-20
- Branch: feature/refactor-pbt-directory-layout
- Polished: {YYYY-MM-DD}

## 目的

`pbt/tests/` 配下に pre-existing で存在する shiguredo-rust 規約違反を 2 件解消し、ディレクトリ配置を規約適合させる。

- `pbt/tests/common.rs`: 「テスト間で共有するヘルパーは `tests/helpers/` に置くこと」規約に反して `tests/` 直下に置かれている
- `pbt/tests/prop_boxes_moov_tree.rs` / `pbt/tests/prop_boxes_sample_entry.rs`: 「PBT のファイル名は `pbt/tests/prop_<module>.rs`」規約に対して、実体は `#[test]` 単体テストのみを持ち PBT を含まない

いずれも `0068` の pbt を proptest から noprop に移行する作業のスコープ外として明記された残懸念。

## 現状

### common.rs

- `pbt/tests/common.rs` (57 行): `arb_language_code` / `arb_track_name` / `arb_track_metadata` / `assert_track_metadata` の 4 ヘルパを提供
- `mod common;` 経由で `pbt/tests/prop_fmp4_segment_mux_demux.rs` と `pbt/tests/prop_mux_demux.rs` の 2 ファイルから参照されている
- shiguredo-rust 「テスト間で共有するヘルパーは `tests/helpers/` に置くこと」規約に不適合

### prop_ 命名違反ファイル

- `pbt/tests/prop_boxes_moov_tree.rs` (754 行): 全て `#[test] fn ...()` の単体テスト。noprop / proptest どちらも使用していない
- `pbt/tests/prop_boxes_sample_entry.rs` (1660 行): 同上
- shiguredo-rust 「PBT のファイル名は `pbt/tests/prop_<module>.rs`」規約と「pbt 以下に unittest を書かないこと」規約に不適合

## 設計方針

### common.rs の配置換え

- `pbt/tests/common.rs` を `pbt/tests/helpers.rs` + `pbt/tests/helpers/` (シンプルな 1 モジュール構成) または `pbt/tests/helpers/mod.rs` に配置換え
- shiguredo-rust の「`mod.rs` を使わないこと」規約に照らすと `pbt/tests/helpers.rs` の単一ファイルが本筋
- 参照側 2 ファイルの `mod common;` を `mod helpers;` に、`common::arb_*` を `helpers::arb_*` に書き換え

### prop_ 命名違反ファイルの扱い (選択肢 B で確定)

「単体テストのファイル名は `tests/test_<module>.rs`」および「pbt 以下に unittest を
書かないこと」の規約に厳密に従い、**選択肢 B** を採用する。

- `pbt/tests/prop_boxes_moov_tree.rs` → `tests/test_boxes_moov_tree.rs` (新規) に移動する
- `pbt/tests/prop_boxes_sample_entry.rs` → 既存 `tests/test_boxes_sample_entry.rs` に合流する
  - tests/ 内で同名のテストバイナリは 2 つ作れないため、既存の VpccBox 境界テスト
    (73 行) に追記合流する
  - 合流後は約 1730 行になるが、ファイル内の mod 分割が既にされているため
    「テストファイルが長くなった場合は mod で分割」規約に適合する

選択肢 A (PBT を追加して `prop_*` 命名を維持) は採用しない。

- 単体テストを pbt/ に残したままでは「pbt 以下に unittest を書かないこと」規約違反が解消しない
- 対象ボックスの roundtrip PBT は `prop_boxes.rs` / `prop_container_boxes.rs` /
  `prop_additional_boxes.rs` / `prop_codec_boxes.rs` で既にカバー済みのため、
  選択肢 A で追加する roundtrip PBT は重複になる

### 選択肢の判断基準 (確定済み)

- 対象 SampleEntry / MoovBox tree の PBT 追加コスト (A の場合): 不要 (既存 PBT でカバー済み)
- 本体 crate の integration test への影響 (B の場合、依存の増加や compile 時間): 移動対象は
  公開 API + std のみを使用するため、本体 crate への依存追加は不要
- shiguredo-rust 規約の厳密度と、既存の 2 ファイル数千行の重量: 規約厳守で B に決定

## 対象外

- 他の pbt/tests/ ファイルの規約適合 (対象 2 ファイルに限る)
- ワークスペース構成の変更 (`pbt/` メンバーの本体統合など)
- 既存 PBT の追加・強化 (issue `0072` / `0073` / `0074` で扱う)

## 完了条件

- `pbt/tests/common.rs` が `pbt/tests/helpers.rs` に配置換えされ、参照側 2 ファイルが更新されている
- `pbt/tests/prop_boxes_moov_tree.rs` が `tests/test_boxes_moov_tree.rs` に移動している
- `pbt/tests/prop_boxes_sample_entry.rs` の内容が `tests/test_boxes_sample_entry.rs` に合流し、元ファイルが削除されている
- `cargo test --workspace` が通る
- `grep -rn "mod common" pbt/tests/` が 0 件

## 解決方法

設計方針の「選択肢 B」に従って実装した。

1. `pbt/tests/common.rs` を `pbt/tests/helpers.rs` に配置換えした (git mv)
   - 参照側の `pbt/tests/prop_mux_demux.rs` / `pbt/tests/prop_fmp4_segment_mux_demux.rs` の
     `mod common;` を `mod helpers;` に、`common::` を `helpers::` に書き換えた
2. `pbt/tests/prop_boxes_moov_tree.rs` を `tests/test_boxes_moov_tree.rs` に移動した (git mv)
   - 先頭の doc コメントを単体テストの説明に修正した。コード本体は公開 API + std のみを
     使用するため、本体 crate への依存追加は不要だった
3. `pbt/tests/prop_boxes_sample_entry.rs` の内容を既存 `tests/test_boxes_sample_entry.rs`
   に合流し、元ファイルを削除した
   - tests/ 内で同名のテストバイナリは 2 つ作れないため、既存の VpccBox 境界テストに
     追記合流した (合流後 1732 行。ファイル内 mod 分割済み)
4. `cargo test --workspace` / `cargo fmt` / `cargo clippy --workspace --all-targets` が
   全て通ることを確認した
