# prop_error_paths.rs を各 PBT ファイルへ再配置する

- Priority: Low
- Created: 2026-05-20
- Completed: 2026-07-23
- Model: opencode mimo-v2.5-pro
- Branch: feature/refactor-split-prop-error-paths
- Polished: 2026-07-23

## 目的

`pbt/tests/prop_error_paths.rs` は複数モジュールのエラーパステストと、一部の正常系テスト（`sample_entry_inner_box_tests` / `base_box_tests`）を 1 ファイルに集約している。`shiguredo-rust` スキルの命名規則「PBT のファイル名は `pbt/tests/prop_<module>.rs` とし、`src/<module>.rs` に対応させること」に違反しているため、対応する PBT ファイル（`src/boxes_moov_tree.rs` / `src/boxes_sample_entry.rs` 対応の 2 ファイルは新設する）へ再配置し、`prop_error_paths.rs` を削除する。

## 優先度根拠

以下 open issue が本 issue の完了を（明示的または間接的に）前提としているが、いずれの下流 issue も本 issue 未完了でも独立に進められる（0029 は解決方法内に分岐が織り込み済み、他は分割前ファイルに書き足せば済む。本 issue 完了後に refresh のコストのみ発生する）。詳細は「## 他 issue との依存関係」を参照。

## 現状

- `pbt/tests/prop_error_paths.rs`：2417 行、116 個の `#[test]`
- 対応する `src/error_paths.rs` は存在しない
- 内部は 10 個の `mod` と 1 つのトップレベル `proptest!` ブロックで構成
- `sample_entry_inner_box_tests` と `base_box_tests` はエラーパスではない（BaseBox / `inner_box()` メソッドの正常系テスト）

`EsdsBox` は `src/descriptors.rs` ではなく `src/boxes_moov_tree.rs` で定義されている点に注意（`impl BaseBox for EsdsBox`）。

### 内部構成と移動先

| 元の mod / ブロック | テスト数 | 対応する src | 移動先 PBT ファイル |
|---|---|---|---|
| `avcc_error_tests` | 8 | `src/boxes_sample_entry.rs` | `prop_codec_boxes.rs` |
| `hvcc_error_tests` | 4 | `src/boxes_sample_entry.rs` | `prop_codec_boxes.rs` |
| `dfla_error_tests` | 1 | `src/boxes_sample_entry.rs` | `prop_codec_boxes.rs` |
| `dops_error_tests` | 1 | `src/boxes_sample_entry.rs` | `prop_codec_boxes.rs` |
| `esds_error_tests` | 1 | `src/boxes_moov_tree.rs` | `prop_codec_boxes.rs`（既存 `esds_box_roundtrip` と同一ファイル） |
| トップレベル `proptest!`（`*_decode_no_panic` 5 テスト）| 5 | 各ボックス | `prop_codec_boxes.rs`（新規 `with_cases(50)` ブロック） |
| `sample_entry_inner_box_tests` | 11 | `src/boxes_sample_entry.rs` | **新設**：`prop_boxes_sample_entry.rs` |
| `moov_tree_error_tests` | 23 | `src/boxes_moov_tree.rs` | **新設**：`prop_boxes_moov_tree.rs` |
| `descriptor_error_tests` | 10 | `src/descriptors.rs` | `prop_descriptors.rs` |
| `mux_error_tests` | 6 | `src/mux.rs` | `prop_mux_demux.rs` |
| `base_box_tests` | 46 | 対象が跨るため分解 | 分解して 2 ファイルへ（moov 22 + SampleEntry 24） |

### `base_box_tests` の 46 テストの内訳と分解先

- **`boxes_moov_tree.rs` 系 22 テスト**（→ `prop_boxes_moov_tree.rs` の `moov_tree_base_box_tests` mod、ヘルパー 6 個同伴）：MoovBox / MvhdBox / TrakBox / TkhdBox / MdiaBox / MdhdBox / HdlrBox / MinfBox / SmhdBox / VmhdBox / DinfBox / DrefBox / UrlBox / EdtsBox / ElstBox / StblBox / StsdBox / SttsBox / StscBox / StszBox / StcoBox / Co64Box の各 `*_base_box`
- **`boxes_sample_entry.rs` 系 24 テスト**（→ `prop_boxes_sample_entry.rs` の `sample_entry_base_box_tests` mod、ヘルパー 12 個同伴）：Avc1Box / AvccBox / Hev1Box / HvccBox / OpusBox / DopsBox / Hvc1Box / Vp08Box / Vp09Box / VpccBox / Av01Box / Av1cBox / Mp4aBox / FlacBox / DflaBox の各 `*_base_box`（17 個）、`SampleEntry` enum の `sample_entry_box_type` / `sample_entry_children`、`SampleEntry::decode` 分岐系 `sample_entry_decode_{hvc1,vp08,vp09,av01,mp4a,flac,hev1}`（7 個）

分類基準：テストが直接 assert する Box 型（または `SampleEntry` enum）が定義される src ファイルで判定する。ヘルパー関数も同基準。

## 設計方針

### 命名規則遵守のための新設ファイル

`shiguredo-rust` の命名規則に沿い、以下 2 ファイルを新設する。

- `pbt/tests/prop_boxes_moov_tree.rs`（`src/boxes_moov_tree.rs` に対応）
- `pbt/tests/prop_boxes_sample_entry.rs`（`src/boxes_sample_entry.rs` に対応）

### 既存 PBT ファイルの命名規則違反は本 issue のスコープ外

`prop_additional_boxes.rs` / `prop_codec_boxes.rs` / `prop_container_boxes.rs` / `prop_fmp4_boxes.rs` / `prop_fmp4_segment_mux_demux.rs` / `prop_boxes.rs` は `src/` に厳密対応していないが、本 issue のスコープ外（後続 issue で扱う）。特に `prop_container_boxes.rs` は MoovBox / TrakBox / MdiaBox / MinfBox / StblBox のコンテナ組合せテスト（`minimal_*_box` ヘルパー群を含む）を集約しており、本 issue で新設する `prop_boxes_moov_tree.rs`（単体 Box テスト）とは責務が異なる。

### `sample_entry_inner_box_tests` の統合方針

- `sample_entry_inner_box_tests` mod を新設 `prop_boxes_sample_entry.rs` に移す
- 併せて `prop_additional_boxes.rs` の `sample_entry_tests` mod を丸ごと `prop_boxes_sample_entry.rs` に転出させる
  - この mod には Stpp 関連 9 テスト（`sample_entry_stpp_methods` / `sample_entry_stpp_encode_decode_roundtrip` / `stpp_box_decode_valid_bytes` / `stpp_box_missing_namespace_null_terminator` / `stpp_box_missing_schema_location_null_terminator` / `stpp_box_missing_auxiliary_mime_types_null_terminator` / `stpp_box_invalid_utf8_in_namespace` / `stpp_box_decode_wrong_box_type` / `sample_entry_decode_stpp_dispatches_to_stpp_variant`）とヘルパー 4 個（`create_stpp_box` / `create_audio_fields` / `create_visual_fields` / `build_valid_stpp_bytes`）が含まれる。mod 全体の移動により自動的に運ばれる
- Stpp 系 3 テスト（`sample_entry_stpp_inner_box` → `sample_entry_inner_box_tests` mod / `sample_entry_stpp_methods` および `sample_entry_stpp_encode_decode_roundtrip` → `sample_entry_tests` mod）は検証観点が異なるため、いずれも保持する
- 統合後の `prop_boxes_sample_entry.rs` の mod 構成:
  - `sample_entry_tests`（元 `prop_additional_boxes.rs` から転入、20 テスト）
  - `sample_entry_inner_box_tests`（元 `prop_error_paths.rs` から転入、11 テスト）
  - `sample_entry_base_box_tests`（新規、24 テスト + ヘルパー 12 個）
- 同名関数の統合:
  - `create_audio_fields()` / `create_visual_fields()`：`prop_error_paths.rs` と `prop_additional_boxes.rs` の 2 実装が完全一致することを実測確認済み。統合先のファイルスコープに 1 個集約する
  - `create_vpcc_box()` / `create_av1c_box()`：`sample_entry_inner_box_tests` 版と `base_box_tests` 版が実測で完全一致（`Uint::new()` のパス表記だけ異なる）。統合先のファイルスコープに 1 個集約する
  - `sample_entry_children`：`sample_entry_base_box_tests`（SampleEntry::Avc1 対象）と `sample_entry_tests`（SampleEntry::Stpp 対象）に別実装で存在。テスト対象バリアントが異なるため両方保持する（mod スコープで自然に分離）

### `*_decode_no_panic` 5 テストの扱い

`avcc_box_decode_no_panic` / `hvcc_box_decode_no_panic` / `dfla_box_decode_no_panic` / `dops_box_decode_no_panic` / `esds_box_decode_no_panic` は「任意入力でパニックしないことだけを検証する PBT」で `shiguredo-rust` 規約に反するが、本 issue では規約違反是正まで踏み込まず、`prop_codec_boxes.rs` に新規 `with_cases(50)` `proptest!` ブロックとして単純移送する（既存 `with_cases(200)` ブロックとは統合しない）。削除・fuzz 化は「### 本 issue 完了後の後続 issue」参照。

### ヘルパー関数の衝突対処

- mod スコープに閉じたヘルパー（`create_avcc_box()` / `create_hvcc_box()` 等）は移動先の mod と一緒に移す
- 移動先ファイルに同名関数がある場合の対処:
  - シグネチャが同一 → 移動元を捨てて既存を再利用
  - シグネチャが異なる → mod 内に閉じたまま置き、意図が伝わる名前にリネーム
    - 例：`mux_error_tests::create_avc1_sample_entry()`（引数なし、1920x1080 固定）は、`prop_mux_demux.rs::create_avc1_sample_entry(width: u16, height: u16)`（引数あり）と衝突するため、mod 内側を `create_fixed_avc1_sample_entry()` にリネーム
    - `mux_error_tests::create_opus_sample_entry()`（引数なし、48000Hz/2ch 固定）も同様に `create_fixed_opus_sample_entry()` にリネーム
- `sample_entry_base_box_tests` mod の残り 8 個のヘルパー（`create_avc1_box` / `create_hev1_box` / `create_opus_box` / `create_hvc1_box` / `create_vp08_box` / `create_vp09_box` / `create_av01_box` / `create_mp4a_box` / `create_flac_box` / `create_dfla_box`）は mod スコープに閉じたまま移送する
- テストユーティリティの共通モジュール化（`pbt/tests/common/` への抽出）は本 issue では行わない。0041（Strategy 定義の共通化）に委ねる

## 完了条件

- `pbt/tests/prop_error_paths.rs` が削除されている
- 新設ファイル `pbt/tests/prop_boxes_moov_tree.rs` / `pbt/tests/prop_boxes_sample_entry.rs` が作成されている
- 移送前後で `cargo test -p pbt` のテスト総数が変化していない
- `cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` / `prek run --all-files` が pass する
- `CHANGES.md` の `## develop` セクションの既存 `### misc` サブセクションに `[UPDATE]` エントリと担当者行が追記されている
- 本 issue ファイルが `bug-` から `refactor-` にリネームされ、`issues/closed/0003-refactor-split-prop-error-paths.md` に移動されている

## 解決方法

以下の順で実装した。分割先ファイルごとに 1 コミットとし、各コミット時点で `cargo test -p pbt` と `cargo clippy --workspace --all-targets -- -D warnings` が pass する状態を保った。事前計測で 464 テスト pass を記録し、移送後も 464 テストを維持した。

1. `avcc_error_tests` / `hvcc_error_tests` / `dfla_error_tests` / `dops_error_tests` / `esds_error_tests` の 5 mod を `prop_codec_boxes.rs` に独立 mod として移送し、合わせて `*_decode_no_panic` 5 テストを新規 `with_cases(50)` `proptest!` ブロックとして追加した。移送に伴い `prop_error_paths.rs` のトップレベル `use` を全削除し、`prop_codec_boxes.rs` の `use` に `DflaBox` を追加した
2. `descriptor_error_tests` を `prop_descriptors.rs` に独立 mod として移送した
3. `mux_error_tests` を `prop_mux_demux.rs` に移送した。`prop_mux_demux.rs` の既存 `create_avc1_sample_entry(width, height)` / `create_opus_sample_entry(channel_count)` と衝突するため、mod 内側を `create_fixed_avc1_sample_entry()` / `create_fixed_opus_sample_entry()` にリネームした
4. `pbt/tests/prop_boxes_moov_tree.rs` を新設し、`moov_tree_error_tests` と `base_box_tests` の moov tree 系 22 テスト + 関連ヘルパー 6 個を移送した（`moov_tree_base_box_tests` mod として配置）。`base_box_tests` mod からは moov 系の `use` と関数のみを削除し、SampleEntry 系はそのまま残した
5. `pbt/tests/prop_boxes_sample_entry.rs` を新設し、以下を集約した:
   - `sample_entry_inner_box_tests`（元 `prop_error_paths.rs`）
   - `base_box_tests` の SampleEntry 系 24 テスト + ヘルパー 12 個（`sample_entry_base_box_tests` mod として配置）
   - `prop_additional_boxes.rs` の `sample_entry_tests` mod 全体（`create_stpp_box` / `create_wvtt_box` / `build_valid_stpp_bytes` ヘルパー含む）
6. 続くコミットで、`prop_boxes_sample_entry.rs` 内の重複ヘルパー（`create_audio_fields` / `create_visual_fields` / `create_vpcc_box` / `create_av1c_box`）をファイルスコープに 1 個ずつ集約し、各 mod から `use super::{...};` で参照する形に整理した。残りのヘルパーは mod スコープに閉じたまま維持した
7. `git rm pbt/tests/prop_error_paths.rs` でファイルを削除した
8. `CHANGES.md` の `## develop` セクションの既存 `### misc` サブセクション末尾に `[UPDATE]` エントリを追記した
9. `cargo test --workspace`（535 テスト pass） / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo fmt --all -- --check` を pass させた。`prek run --all-files` は `check-symlinks` が既存の broken symlink（`examples/dump_wasm/dump_wasm.wasm` / `examples/transcode_wasm/transcode_wasm.wasm`、wasm 未ビルドによる本 issue と無関係の問題）を検出したが、それ以外の hook は全て pass した
10. 本コミットで `Completed:` を作業完了日に更新し、`## 解決方法` を実施結果に書き換え、同一コミットで `git mv issues/0003-bug-split-prop-error-paths.md issues/0003-refactor-split-prop-error-paths.md` により bug → refactor にリネームした
11. 続くコミットで `git mv issues/0003-refactor-split-prop-error-paths.md issues/closed/0003-refactor-split-prop-error-paths.md` により closed に移動した

## CHANGES.md

```markdown
- [UPDATE] `pbt/tests/prop_error_paths.rs` を対応する各 PBT ファイル（`prop_boxes_moov_tree.rs` / `prop_boxes_sample_entry.rs` を新設）に再配置する
  - `shiguredo-rust` の PBT ファイル命名規則違反を解消する
  - @<担当者>
```

## 他 issue との依存関係

- `issues/0027-test-fmp4-error-path-tests.md`：追加先候補として `prop_error_paths.rs` を挙げる。本 issue 完了後は 0027 の担当者が `refresh-issue` で対応
- `issues/0029-bug-mdhd-language-code-5bit-validation.md`：0029 側の解決方法に「issue 0003 が先に実施された場合は、分割後の対応ファイルに追加する」の分岐が織り込み済み。追随不要（本 issue 完了時に自動的に「分割後の対応ファイル = `prop_boxes_moov_tree.rs` の `moov_tree_error_tests` mod」となる）
- `issues/0041-refactor-pbt-strategy-dedup.md`：本 issue で `sample_entry_tests` を `prop_boxes_sample_entry.rs` に集約するため、0041 の対象範囲（`prop_additional_boxes.rs` / `prop_codec_boxes.rs` の Strategy 定義の共通化）が本 issue 完了後に確定する。0041 側の本文に本 issue で新設するファイルへの参照はないため 0041 の refresh は不要
- `issues/0044-add-subtitle-wvtt.md`：`### PBT 追加` / `### 単体テスト追加` / `## 解決方法` に `prop_error_paths.rs` および `prop_additional_boxes.rs::sample_entry_tests` mod への参照が複数箇所ある。本 issue 完了後、これらは `prop_boxes_sample_entry.rs` への読み替えが必要で、0044 の担当者が `refresh-issue` で対応
- `issues/0045-add-subtitle-tx3g.md`：SampleEntry 系ボックス追加。本 issue 完了後、テスト追加先は `prop_boxes_sample_entry.rs` になる。0045 側の本文に本 issue で新設するファイルへの参照はないため 0045 の refresh は不要

### 本 issue 完了後の後続 issue

本 issue の作業から派生し、以下 4 項目は後続 issue として起票されるべきである。起票の実施可否・タイミング・優先度は担当者判断（本 issue の完了条件には含めない）:

- `*_decode_no_panic` 5 テストの削除（fuzz と役割重複、`shiguredo-rust` 規約違反。fuzz ターゲット `fuzz_avcc_box.rs` 等は既存で、decode 後さらに encode まで走らせるためパニックカバレッジは PBT 版を厳密に含む）
- `pbt/tests/prop_*.rs` の `ProptestConfig::with_cases` 値の整理（現状 20 / 50 / 64 / 100 / 200 / 256 / 500 / 1000 が混在）
- `prop_additional_boxes.rs` の SampleEntry 系 top-level roundtrip 11 テスト + `boundary_tests` 内 3 テスト（`opus_box_minimal` / `mp4a_box_aac_lc` / `avc1_box_1080p`）の `prop_boxes_sample_entry.rs` への完全集約
- 命名規則違反 PBT ファイル群（`### 既存 PBT ファイルの命名規則違反は本 issue のスコープ外` 節で列挙した 6 ファイル）の全面解消
