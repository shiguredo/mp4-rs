# crates/wasm の hev1 / hvc1 系モジュールの重複コードを共通化する

- Priority: Medium
- Created: 2026-07-28
- Completed: 2026-07-30
- Model: Opus 4.7
- Branch: feature/refactor-wasm-hev1-hvc1-dedup
- Polished: 2026-07-30

## 目的

`crates/wasm/src/boxes_hev1.rs` と `crates/wasm/src/boxes_hvc1.rs` は、識別子（`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1`、関数名の `_hev1_` / `_hvc1_`、JSON の `"kind"`）と一部コメント・テスト JSON の整形を除いて、実装の 95% 以上が同一の重複コードである。closed issue 0010 で「片方だけ触って片方を忘れる」タイプのバグが実際に起きた場所であり、再発防止のため共通化する。

## 優先度根拠

Medium。closed issue 0010 で「両方に同型のバグ（`mp4_free(_, 0)` の no-op リーク、`free_array_list` の count 不一致）が同時に埋め込まれた」実例があり、放置は再発リスク。とはいえ緊急ではない。

## 現状

hev1 / hvc1 の 2 ファイルは合計 1147 行（`boxes_hev1.rs` 566 + `boxes_hvc1.rs` 581）あり、実質同一の 4 経路で重複している。

### 経路 1: 解放関数

- `crates/wasm/src/boxes_hev1.rs` の `mp4_sample_entry_hev1_free`
- `crates/wasm/src/boxes_hvc1.rs` の `mp4_sample_entry_hvc1_free`

差はコーデック名と `entry` の型（`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1`）のみ。

### 経路 2: JSON パース関数

- `crates/wasm/src/boxes_hev1.rs` の `parse_json_mp4_sample_entry_hev1`
- `crates/wasm/src/boxes_hvc1.rs` の `parse_json_mp4_sample_entry_hvc1`

差はコーデック名と戻り値型のみ。closed issue 0047（`Completed: 2026-07-29`）により、両関数とも「フェーズ 1: JSON を Rust 型に落とす → フェーズ 2: 末尾で `allocate_and_copy_*`」の deferred allocate に既に揃っている。0047 は allocate 順序の統一のみで、hev1 / hvc1 間の共通化は行っていない。したがって経路 2 の共通化は本 issue の対象に含める。

### 経路 3: JSON 出力（NaluArrays::fmt）

- `crates/wasm/src/boxes_hev1.rs` の `struct NaluArrays` と `impl nojson::DisplayJson for NaluArrays`
- `crates/wasm/src/boxes_hvc1.rs` の同名定義

本体は完全同一。差は doc コメントの接頭辞（`HEVC NALU` / `NALU`）のみ。

### 経路 4: テストの JSON リテラル

- 各ファイルに 6 個のテスト（`test_*_to_json` / `test_json_to_*` / `test_json_to_*_rejects_missing_width_after_nalu_arrays` / `test_json_to_*_free_{more,fewer,empty}_nalu_arrays`）
- テスト内 JSON リテラルは共通スカラーフィールド（`width` 〜 `lengthSizeMinusOne` の 18 個）を繰り返しており、差は `"kind": "hev1"` vs `"kind": "hvc1"` と `naluArrays` の中身、および一部テストでの JSON 整形差だけ

## 設計方針

共通の内部 helper に寄せる。マクロは使わない（`shiguredo-rust` の「マクロを作らないこと」規約）。

`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` は `crates/c-api/src/boxes.rs` の `#[repr(C)]` 構造体でフィールドレイアウトが完全に同一（closed issue 0035 で導入した非公開中間型 `HevcSampleEntryRaw` とも同型）。共通化は次の方針に固定する:

- **案 B（ヘルパー関数の引数化）**: `fn free_hevc_sample_entry_fields(nalu_array_count: &mut u32, nalu_types: &mut *const u8, ...)` として生ポインタを渡す。`*_free` 関数はフィールドを取り出して helper に渡すだけの薄いラッパになる。トレイト不使用
- **案 A（ジェネリクス + トレイト）は不採用**: `shiguredo-rust` は「トレイトを作らないこと」を規約としており、closed issue 0035（コア + c-api の Hev1 / Hvc1 共通化）もトレイトを使わずヘルパー抽出に寄せている。本 issue でも同様に案 A は採らない

### 経路 1 の共通化

`crates/wasm/src/boxes.rs` に `free_hevc_sample_entry_fields` を置き、`mp4_sample_entry_hev1_free` / `_hvc1_free` を薄いラッパにする。

### 経路 2 の共通化

同様にヘルパーへ寄せる。公開関数 `parse_json_mp4_sample_entry_hev1` / `_hvc1` のシグネチャと戻り値型は変えず、JSON からのスカラー・NALU 配列の取り込みと `allocate_and_copy_*` 呼び出しを共通化する。戻り値の `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` 構築だけを各公開関数側に残す。

### 経路 3 の共通化

`struct NaluArrays` は完全同一のため、`crates/wasm/src/boxes.rs` などの共通モジュールに 1 つ定義して両ファイルから `use` する。

### 経路 4 の共通化

テストヘルパー `fn build_hevc_test_json(kind: &str, nalu_arrays_json: &str) -> String` を各テストモジュール（あるいは `#[cfg(test)]` 共通モジュール）に置き、6 個のテストの JSON リテラルからヘッダ部分を排除する。

### 他 issue との関係

- **issue 0035**（コアの `src/boxes_sample_entry.rs` と `crates/c-api/src/boxes.rs` の Hev1Box / Hvc1Box 重複を共通化）: 完了済み。レイヤーが異なる（コア + c-api）。本 issue は wasm 側に閉じる
- **issue 0047**（`parse_json_mp4_sample_entry_*` 系の allocate 順序 defer 化）: 完了済み。経路 2 の allocate 順序は既に揃っている。共通化は 0047 のスコープ外だったため、本 issue で行う
- **issue 0048**（`mp4_alloc` / `allocate_and_copy_*` のアラインメント）: 別目的。本 issue では触らない

## 完了条件

- 経路 1・2・3・4 の重複が解消される
- 公開 API（`pub fn mp4_sample_entry_hev1_free` / `_hvc1_free`、`pub fn parse_json_mp4_sample_entry_hev1` / `_hvc1`、`pub fn fmt_json_mp4_sample_entry_hev1` / `_hvc1`）のシグネチャは変えない
- `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が pass する
- `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --all-targets -- -D warnings` が warning なしで通る

## 解決方法

`feature/refactor-wasm-hev1-hvc1-dedup` ブランチで案 B（トレイト不使用のヘルパー関数化）で対応した。

### 実施内容

- 経路 1（解放関数）: `crates/wasm/src/boxes.rs` に `free_hevc_sample_entry_fields` を追加し、`mp4_sample_entry_hev1_free` / `_hvc1_free` はフィールド 5 本を渡すだけの薄いラッパにした
- 経路 2（JSON パース）: `parse_json_hevc_sample_entry_fields` を共通ヘルパーとして追加。フェーズ 1（JSON → Rust 型）とフェーズ 2（`allocate_and_copy_*` × 3）の deferred allocate 順序は維持したまま共通化した。公開関数 `parse_json_mp4_sample_entry_hev1` / `_hvc1` は共通ヘルパーの結果を各 `#[repr(C)]` 型へ写し替えるだけの薄いラッパ（`hevc_fields_to_hev1` / `_hvc1`）になる
- 経路 3（`NaluArrays::fmt`）: `struct NaluArrays` を `boxes.rs` に `HevcNaluArrays` として集約し、両ファイルから `use` する
- 経路 4（テスト JSON リテラル）: `HEVC_TEST_JSON_SCALAR_FIELDS` 定数に既定値を集約し、`build_hevc_test_json` / `build_hevc_test_json_omitting` の 2 関数から参照する
- 共通の中間表現は `HevcSampleEntryFields` として `boxes.rs` に集約した（`c-api::boxes::Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` と同型のフィールドを持つ）
- 公開 API（`pub fn mp4_sample_entry_hev1_free` / `_hvc1_free` / `parse_json_mp4_sample_entry_hev1` / `_hvc1` / `fmt_json_mp4_sample_entry_hev1` / `_hvc1`）のシグネチャは変更していない
- `CHANGES.md` の `## develop` の `### misc` に `[UPDATE]` を追記した

### レビュー指摘への追加対応

`/review-diff-code` の指摘を受け、以下も本 PR に含めた:

- doc の英語混在「allocate 済み」を「割り当て済み」に置き換え、`HevcSampleEntryFields` の docstring に「一回限りの受け渡し用途」と「フェーズ 2 が panic しない前提」を明記した
- `hevc_fields_to_hev1` / `_hvc1` にトレイト・マクロ禁止規約下での判断根拠を追記した
- 中間型を `HevcSampleEntryAllocated` から `HevcSampleEntryFields` へ改名した（`Allocated` サフィックスの含意が曖昧だったため）
- `test_json_to_hev1` / `_hvc1` のアサーションを 6 個から 19 個のスカラーフィールド全数に拡張し、`hevc_fields_to_*` 内のフィールド取り違えを検出できる網を張った
- `free_hevc_sample_entry_fields` の部分 alloc 失敗経路（`nalu_counts == null && nalu_data != null` の非常態）に回帰テストを追加した
- テスト JSON ヘルパーを構造化し、`.replace("            \"width\": 1920,\n", "")` の脆さを解消した（`build_hevc_test_json_omitting(kind, arrays, Some(field))`）
- `parse_json_hevc_sample_entry_fields` のエラーパスを表駆動で網羅した（18 個のスカラー欠落 + `naluArrays` / `naluType` / `units` 欠落）

### 検証

- `cargo fmt --all -- --check` / `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --all-targets -- -D warnings` / `cargo test -p wasm --lib` / `cargo doc --workspace --exclude dump_wasm --exclude transcode_wasm --no-deps` がすべて通ることを確認した
- 元の 12 個のテスト（hev1 / hvc1 × 6）に加え、共通ヘルパーの直接テスト 5 個（改善 10 + 11 + `build_hevc_test_json_omitting` セルフテスト）を追加し、49 個の pass を確認した
