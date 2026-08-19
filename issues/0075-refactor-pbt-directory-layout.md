# `pbt/` の shiguredo-rust 規約違反 (common.rs 配置 / `prop_` 命名) を解消する

- Created: 2026-08-19
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-pbt-directory-layout
- Polished: {YYYY-MM-DD}

## 目的

`pbt/tests/` 配下に pre-existing で存在する shiguredo-rust 規約違反を 2 件解消し、ディレクトリ配置を規約適合させる。

- `pbt/tests/common.rs`: 「テスト間で共有するヘルパーは `tests/helpers/` に置くこと」規約に反して `tests/` 直下に置かれている
- `pbt/tests/prop_boxes_moov_tree.rs` / `pbt/tests/prop_boxes_sample_entry.rs`: 「PBT のファイル名は `pbt/tests/prop_<module>.rs`」規約に対して、実体は `#[test]` 単体テストのみを持ち PBT を含まない

いずれも `0068` の pbt を proptest から noprop に移行する作業のスコープ外として明記された残懸念。

## 現状

### common.rs

- `pbt/tests/common.rs` (49 行): `arb_language_code` / `arb_track_name` / `arb_track_metadata` / `assert_track_metadata` の 4 ヘルパを提供
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

### prop_ 命名違反ファイルの扱い

2 択で issue 内で決定する:

- **選択肢 A**: PBT (encode/decode roundtrip 等) を追加して `prop_*` 命名を維持する
  - `prop_boxes_moov_tree.rs`: 現状のツリー構築 unit test に加え、`MoovBox` ツリーの roundtrip PBT を追加
  - `prop_boxes_sample_entry.rs`: 各 SampleEntry variant の追加 roundtrip PBT
- **選択肢 B**: `test_` プレフィックスに rename して、本体 crate 側の `tests/` に移動する
  - `pbt/` ワークスペースメンバーには置かない
  - 実質的に単体テストなので本体 crate の integration test が本筋

shiguredo-rust の「pbt 以下に unittest を書かないこと」規約に厳密に従うと B が本筋。ただし A の PBT 追加で `pbt/` に留める価値も評価対象。

### 選択肢の判断基準

- 対象 SampleEntry / MoovBox tree の PBT 追加コスト (A の場合)
- 本体 crate の integration test への影響 (B の場合、依存の増加や compile 時間)
- shiguredo-rust 規約の厳密度と、既存の 2 ファイル数千行の重量

## 対象外

- 他の pbt/tests/ ファイルの規約適合 (対象 2 ファイルに限る)
- ワークスペース構成の変更 (`pbt/` メンバーの本体統合など)
- 既存 PBT の追加・強化 (issue `0072` / `0073` / `0074` で扱う)

## 完了条件

- `pbt/tests/common.rs` が `pbt/tests/helpers.rs` に配置換えされ、参照側 2 ファイルが更新されている
- `pbt/tests/prop_boxes_moov_tree.rs` / `pbt/tests/prop_boxes_sample_entry.rs` の扱いが選択肢 A / B のいずれかで確定し、実装されている
- `cargo test -p pbt --workspace` が通る
- `grep -rn "mod common" pbt/tests/` が 0 件
