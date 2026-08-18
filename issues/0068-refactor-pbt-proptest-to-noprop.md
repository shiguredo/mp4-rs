# pbt を proptest から noprop に移行する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-pbt-proptest-to-noprop
- Polished: {YYYY-MM-DD}

## 目的

`pbt/` 配下の Property-Based Testing フレームワークを proptest から noprop に統一する。`shiguredo-rust` スキル (「PBT は noprop を使うこと」) と現状の実装が乖離しており、この乖離を残したままだと新規追加 PBT (`0062`〜`0066` の bitstream 系) と既存 PBT で採用フレームワークが二重化して保守負荷が上がる。

## 現状

- `pbt/Cargo.toml` の `[dev-dependencies]` は `proptest = "1.11"` のみを保持しており、noprop は未導入
- `pbt/tests/` 配下の 13 個の PBT ファイルすべてが `use proptest::prelude::*;` を先頭に持ち、`proptest!` マクロ / `Strategy` / `prop_oneof!` / `any::<T>()` / `Just` などの proptest API に依存している
  - `prop_additional_boxes.rs` / `prop_auxiliary.rs` / `prop_basic_types.rs` / `prop_boxes.rs` / `prop_boxes_moov_tree.rs` / `prop_codec_boxes.rs` / `prop_container_boxes.rs` / `prop_demux.rs` / `prop_descriptors.rs` / `prop_fmp4_boxes.rs` / `prop_fmp4_segment_mux_demux.rs` / `prop_mp4_file_kind_detector.rs` / `prop_mux_demux.rs`
- 共通ヘルパ `pbt/tests/common.rs` も proptest 前提で書かれており、`arb_*` ヘルパは `impl Strategy<Value = T>` を返す
- `shiguredo-rust` スキルの「ライブラリ」節は「PBT は noprop を使うこと」と規定しており、規約と実装が乖離している
- `0062`〜`0066` の bitstream 系 open issue は新規 PBT を noprop で書く方針を明記しており、本 issue で既存分の移行を完了しないと proptest と noprop の両方が pbt に共存する期間が発生する

## 設計方針

`pbt/tests/` 配下の全 PBT ファイルと共通ヘルパ (`pbt/tests/common.rs`) を noprop に書き換え、`pbt/Cargo.toml` から proptest 依存を削除する。noprop の使い方は `noprop` スキル (Runner / TestCaseContext / Ratio / `sample_*` / rejection / failure reproduction) を参照する。

移行にあたっての方針:

- 1 ファイル 1 コミットを基本とし、各コミットで移行完了後に `cargo test -p pbt --test prop_{module}` が通ることを確認する (`shiguredo-git` の粒度規約に従う)
- proptest の `Strategy` を返す `arb_*` ヘルパは、noprop の `sample_*` を呼ぶ関数に置き換える。命名は既存の `arb_*` を維持してよい
- 検証対象のプロパティ (ラウンドトリップ、境界条件、不変条件) の意味論は変えない。noprop の `sample_with_boundaries` を使い、既存の proptest で `Just` によって強制していた境界値を維持する
- 既存の `prop_oneof!` は noprop の `sample_choice` (等価 API) または重み付きサンプリングに置き換える。旧テストのカバレッジ (どの分岐をどれくらい踏むか) を落とさない
- `proptest!` マクロが提供していた自動 shrink・再現用シードは、noprop の Runner とテストケースコンテキストの機能で置き換える。移行前後で「失敗時にどう再現するか」の手段が確保されていること
- 移行の途中でテストの意味論を強化・弱体化させない (別 issue で扱う)

pbt 以外への影響:

- crate 本体 (`shiguredo_mp4`) の `[dependencies]` には影響しない (proptest は元々 dev-dep)
- `crates/c-api` / `crates/wasm` / `fuzz/` は proptest を使っていないため影響しない (`grep -rln proptest src/ crates/ fuzz/` で 0 件を確認済み)
- `CHANGES.md` には内部テスト基盤の変更として `[UPDATE]` エントリを追加する (公開 API 変更なしのため `[CHANGE]` ではない)

### 対象外

- テスト意味論の強化 (境界値の追加、プロパティの追加) は別 issue で扱う
- noprop 側のバグ修正・機能追加は別 issue で扱う (`shiguredo/noprop` リポジトリ側の作業)
- `0062`〜`0066` の bitstream 系新規 PBT は各 issue で noprop で書く (本 issue の対象外)

## 完了条件

- `pbt/Cargo.toml` の `[dev-dependencies]` から `proptest` が消え、`noprop` が追加されていること
- `pbt/tests/` 配下の 13 個の PBT ファイル (`prop_additional_boxes.rs` / `prop_auxiliary.rs` / `prop_basic_types.rs` / `prop_boxes.rs` / `prop_boxes_moov_tree.rs` / `prop_codec_boxes.rs` / `prop_container_boxes.rs` / `prop_demux.rs` / `prop_descriptors.rs` / `prop_fmp4_boxes.rs` / `prop_fmp4_segment_mux_demux.rs` / `prop_mp4_file_kind_detector.rs` / `prop_mux_demux.rs`) と `pbt/tests/common.rs` から `proptest::` の import・API 呼び出しが 0 件になること (`grep -rn "proptest" pbt/` で確認)
- 各 PBT ファイルが検証していたプロパティの意味論 (ラウンドトリップ / 境界条件 / 不変条件) が移行前後で変わらないこと。境界値サンプリング (旧 `Just(0)` / `Just(u32::MAX)` などで強制していたケース) が noprop 側でも同等以上の頻度で踏まれること
- 失敗時の再現手段 (シード指定など) が noprop の Runner 機能で確保されていること
- `cargo test --workspace` が通り、pbt の全 PBT が noprop 経由で成功すること
- `CHANGES.md` の `develop` に `[UPDATE]` として記載されていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
