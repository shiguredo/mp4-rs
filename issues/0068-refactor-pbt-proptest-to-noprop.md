# pbt を proptest から noprop に移行する

- Created: 2026-08-18
- Completed: {YYYY-MM-DD}
- Branch: feature/refactor-pbt-proptest-to-noprop
- Polished: 2026-08-18

## 目的

`pbt/` 配下の Property-Based Testing フレームワークを proptest から noprop に統一する。`shiguredo-rust` スキル (「PBT は noprop を使うこと」) と現状の実装が乖離しており、この乖離を残したままだと新規追加 PBT (`0062`〜`0066`、`0069` の bitstream 系) と既存 PBT で採用フレームワークが二重化して保守負荷が上がる。

## 現状

- `pbt/Cargo.toml` の `[dev-dependencies]` は `proptest = "1.11"` のみを保持しており、noprop は未導入
- `pbt/tests/` 配下の `prop_*.rs` は 14 ファイル存在するが、実際に proptest を使用しているのは以下の 12 ファイル (grep で `proptest` の import・マクロ・型参照を確認):
  - `prop_additional_boxes.rs` / `prop_auxiliary.rs` / `prop_basic_types.rs` / `prop_boxes.rs` / `prop_codec_boxes.rs` / `prop_container_boxes.rs` / `prop_demux.rs` / `prop_descriptors.rs` / `prop_fmp4_boxes.rs` / `prop_fmp4_segment_mux_demux.rs` / `prop_mp4_file_kind_detector.rs` / `prop_mux_demux.rs`
- 共通ヘルパ `pbt/tests/common.rs` は proptest 前提で書かれており、`arb_language_code` / `arb_track_name` / `arb_track_metadata` は `impl Strategy<Value = T>` を返し、`assert_track_metadata` は `prop_assert_eq!` を使い `Result<(), TestCaseError>` を返す
- `pbt/tests/` の中で `prop_` プレフィックスなのに proptest を使わない (`#[test]` のみの単体テスト) ファイルが 2 つある: `prop_boxes_moov_tree.rs` と `prop_boxes_sample_entry.rs`。これらは shiguredo-rust の「PBT のファイル名は `pbt/tests/prop_<module>.rs`」および「pbt 以下に unittest を書かないこと」との pre-existing な整合違反で、**本 issue のスコープ外** (別 issue で扱う。残懸念に記載)
- crate 本体 (`shiguredo_mp4`) の `[dependencies]` は空で、`src/` / `crates/` / `fuzz/` にも proptest 依存はない (`grep -rln proptest src/ crates/ fuzz/` は 0 件)
- `shiguredo-rust` スキルの「ライブラリ」節は「PBT は noprop を使うこと」と規定しており、規約と実装が乖離している
- `0062`〜`0066`、`0069` の open issue は新規 PBT を noprop で書く方針を明記しており、本 issue で既存分の移行を完了しないと proptest と noprop の両方が pbt に共存する期間が発生する

## 設計方針

`pbt/tests/` 配下の proptest 使用 12 ファイルと共通ヘルパ `common.rs` を noprop に書き換え、`pbt/Cargo.toml` から `proptest` 依存を削除し、`noprop = "0.2"` を追加する。noprop の使い方は `noprop` skill (Runner / TestCaseContext / Ratio / `sample_*` / rejection / failure reproduction) を参照する。

### Cargo.toml の変更

`pbt/Cargo.toml` の `[dev-dependencies]`:

- 削除: `proptest = "1.11"`
- 追加: `noprop = "0.2"` (用途コメント「Property-Based Testing フレームワーク」を維持)

shiguredo-rust の「バージョン番号はマイナーバージョンまで指定すること」規約と「依存ライブラリには用途をコメントで明記すること」規約に従う。

### コミット分割と `common.rs` の扱い

`common.rs` は `mod common;` 経由で 2 ファイル (`prop_fmp4_segment_mux_demux.rs` / `prop_mux_demux.rs`) から参照されており、`impl Strategy<Value = T>` を返すシグネチャに呼び出し側が依存している。`common.rs` を先に noprop 化すると呼び出し側がコンパイル不能になり、後回しにすると呼び出し側の noprop 化後にヘルパを呼べなくなる。

そのため以下の順序でコミットを分割する。

1. 最初のコミットで `pbt/Cargo.toml` の `[dev-dependencies]` に `noprop = "0.2"` (用途コメント付き) を追加する。`proptest` は残したままにする (以降のコミットで proptest と noprop の両方が dev-dep に存在する状態を許容する)
2. `common.rs` と、それを利用する 2 ファイル (`prop_fmp4_segment_mux_demux.rs` / `prop_mux_demux.rs`) を **同一コミット** で noprop へ移行する。このコミットの検証では対象テストバイナリが 2 つある (`cargo test -p pbt --test prop_fmp4_segment_mux_demux` と `cargo test -p pbt --test prop_mux_demux`) ため両方が通ることを確認する
3. `common.rs` に依存しない残り 10 ファイル (`prop_additional_boxes.rs` / `prop_auxiliary.rs` / `prop_basic_types.rs` / `prop_boxes.rs` / `prop_codec_boxes.rs` / `prop_container_boxes.rs` / `prop_demux.rs` / `prop_descriptors.rs` / `prop_fmp4_boxes.rs` / `prop_mp4_file_kind_detector.rs`) は 1 ファイル 1 コミットで移行し、各コミット直後に `cargo test -p pbt --test prop_<module>` が通ることを確認する
4. すべての移行が完了した最後のコミットで `pbt/Cargo.toml` から `proptest` 依存を削除する

`assert_track_metadata` のシグネチャは noprop 移行後に `Result<(), TestCaseError>` から通常の `assert!` / `assert_eq!` (パニック) に変わる (noprop は property 本体を `Fn(&mut TestCaseContext) -> Result<(), Box<dyn Error>>` で書き、失敗は `panic!` またはエラー返しで表現する)。呼び出し側の `?` 演算子も外す必要がある。

### proptest API から noprop API への写像

用法別に置換対応を決める。

- `use proptest::prelude::*;` の import → 削除。`use noprop::...;` に差し替える
- `proptest! { fn foo(v in strat) { ... } }` → `Runner::new(seed).run(cases, |ctx| { let v = strat.sample(ctx); ... Ok(()) })` の形へ書き換える
- `Strategy<Value = T>` を返す `arb_*` ヘルパ → `fn arb_*(ctx: &mut TestCaseContext) -> T` の形の noprop サンプラー関数に書き換える。命名は既存の `arb_*` を維持する
- `prop_oneof![Just(a), Just(b), Just(c)]` (固定値スライスから 1 つ選ぶ) → `sample_choice(ctx, &[a, b, c])`
- `prop_oneof![s1(), s2()]` (複数の Strategy を混在させる) → `sample_weighted_index(ctx, &[1, 1])` で分岐 index を得てから対応する `arb_*(ctx)` を呼ぶ
- `prop_oneof![w1 => s1(), w2 => s2()]` (重み付き) → `sample_weighted_index(ctx, &[w1, w2])`
- `Just(v)` を境界値として混ぜているケース (例: `arb_bit_position` の `Just(0) / Just(31) / Just(32) / Just(33) / Just(usize::MAX) / any::<usize>()`) → `sample_with_boundaries(ctx, &[0, 31, 32, 33, usize::MAX], ratio, |ctx| sample_usize(ctx))` の形にする。`ratio` には既存の `prop_oneof!` の等確率選択と同等の境界値ヒット率を指定する。上記例では 6 分岐等確率で境界値 5 + ランダム 1 なので `Ratio::new(5, 6)` を使う (5/6 の確率で境界値、1/6 の確率でランダム)。境界値ヒット率が `1/N` の場合のみ `Ratio::one_nth(N)` を使う
- `prop_assert!` / `prop_assert_eq!` → `assert!` / `assert_eq!` (noprop の property 本体では panic による失敗検出が既定)
- `#![proptest_config(ProptestConfig::with_cases(N))]` の `N` → `Runner::new(seed).run(N, |ctx| ...)` の第 1 引数として同じ数値を渡す。既存の各箇所で使われている N (grep で 20 / 50 / 64 / 100 / 200 / 256 / 500 / 1000 の 8 種を確認) をそのまま保持する

### seed の供給

各 PBT ファイルは `noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")` で seed を取得する。環境変数名は `MP4_RS_PBT_SEED` に統一する (プロジェクト prefix + `PBT_SEED`)。失敗ケースを再現するときは同じ環境変数を設定して再実行する。

### 「proptest」文字列の 0 件化

移行後、`pbt/` 配下で `proptest` という文字列が残らないようにする。対象は import / マクロ / 型参照 / module パス / **コメント内の文字列** も含む。

現状で確認されているコメント内の `proptest` 残存: `prop_additional_boxes.rs` 冒頭の「`proptest_boxes.rs と proptest_codec_boxes.rs でカバーされていない Box のテスト`」(存在しないファイル名への言及であり既存の誤記)。このコメントも本 issue で書き換える (`prop_boxes.rs と prop_codec_boxes.rs で〜` に修正)。

### CHANGES.md エントリ

`shiguredo-changelog` は「機能に直接影響しない変更 (ドキュメント追加、リファクタリング等) は `### misc` サブセクションに記載すること」と規定する。本作業は crate 利用者への公開 API 影響がない内部テスト基盤の refactor であり、`### misc` サブセクションに `- [UPDATE] pbt を proptest から noprop に移行する` の形式 (種別 + 1 行) で記載する。担当者の行はエントリの最後に書く。

### 意味論の保存

- 検証対象のプロパティ (ラウンドトリップ、境界条件、不変条件) の意味論は変えない
- 各 PBT の cases 数 (`with_cases(N)`) は同じ N を保持する
- 境界値サンプリング (旧 `Just(0)` / `Just(u32::MAX)` などで強制していたケース) を維持するため、`sample_with_boundaries` の boundaries 配列に該当値を含める
- カバレッジ (`cargo llvm-cov -p pbt`) が移行前後で有意に低下しないことを目視で確認する。実装上の疑義がある箇所は noprop skill の `docs::recipes` (coverage gate パターン) に従って `Cell<usize>` カウンタを入れる

### 対象外

- テスト意味論の強化 (境界値の追加、プロパティの追加) は別 issue で扱う
- noprop 側のバグ修正・機能追加は別 issue で扱う (`shiguredo/noprop` リポジトリ側の作業)
- `0062`〜`0066`、`0069` の bitstream 系および AAC 系新規 PBT は各 issue で noprop で書く (本 issue の対象外)
- `pbt/tests/prop_boxes_moov_tree.rs` および `pbt/tests/prop_boxes_sample_entry.rs` の命名規則違反 (`prop_` プレフィックスなのに単体テスト) は本 issue のスコープ外。別 issue で `tests/` への移動または命名の見直しを扱う (残懸念に記載)
- `pbt/tests/common.rs` を `pbt/tests/helpers/` へ配置換えする shiguredo-rust 規約適合作業は本 issue のスコープ外 (別 issue で扱う候補)

## 完了条件

- `pbt/Cargo.toml` の `[dev-dependencies]` から `proptest` が消え、`noprop = "0.2"` (用途コメント付き) が追加されていること
- `pbt/tests/` 配下の proptest 使用 12 ファイル (現状セクションに列挙) と `pbt/tests/common.rs` から `proptest::` の import・API 呼び出しが 0 件になること (import / マクロ / 型参照 / モジュールパス / コメント内文字列すべてを対象。`grep -rn "proptest" pbt/` で 0 件を確認する)
- `common.rs` の `arb_*` ヘルパが `fn arb_*(ctx: &mut TestCaseContext) -> T` の形の noprop サンプラーになっていること
- `assert_track_metadata` が noprop 版に書き換わり、`Result<(), TestCaseError>` から `assert!` / `assert_eq!` を使う形になっていること (呼び出し側の `?` も外れていること)
- 各 PBT ファイルが検証していたプロパティの意味論 (ラウンドトリップ / 境界条件 / 不変条件) が移行前後で変わらないこと。境界値サンプリング (旧 `Just(0)` / `Just(u32::MAX)` などで強制していたケース) が `sample_with_boundaries` の boundaries に含まれること。cases 数 (`with_cases(N)`) が同じ N で `runner.run(N, ...)` に渡されていること
- 失敗時の再現手段が `noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")` 経由で確保されていること
- `cargo test --workspace` が通り、pbt の全 PBT が noprop 経由で成功すること
- `CHANGES.md` の `develop` の `### misc` サブセクションに 1 行のエントリが追加され、エントリ末尾に担当者行が付いていること
- `cargo fmt --all -- --check`、`cargo clippy --workspace -- -D warnings`、`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` が通ること
