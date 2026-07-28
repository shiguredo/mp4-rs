# crates/wasm の hev1 / hvc1 系モジュールの重複コードを共通化する

- Priority: Medium
- Created: 2026-07-28
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/refactor-wasm-hev1-hvc1-dedup
- Polished: YYYY-MM-DD

## 目的

`crates/wasm/src/boxes_hev1.rs` と `crates/wasm/src/boxes_hvc1.rs` は、識別子（`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1`、関数名の `_hev1_` / `_hvc1_`）と一部のポインタキャスト書式を除いて、実装の 95% 以上が同一の重複コードである。closed issue 0010 で「片方だけ触って片方を忘れる」タイプのバグが実際に起きた場所であり、再発防止のため共通化する。

## 優先度根拠

Medium。closed issue 0010 で「両方に同型のバグ（`mp4_free(_, 0)` の no-op リーク、`free_array_list` の count 不一致）が同時に埋め込まれた」実例があり、放置は再発リスク。とはいえ緊急ではない。

## 現状

hev1 / hvc1 の 2 ファイルは合計 1031 行あり、実質同一の 4 経路で重複している。

### 経路 1: 解放関数

- `crates/wasm/src/boxes_hev1.rs:202` `mp4_sample_entry_hev1_free`（約 45 行）
- `crates/wasm/src/boxes_hvc1.rs:202` `mp4_sample_entry_hvc1_free`（同 45 行）

差はコーデック名と `entry` の型（`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1`）のみ。

### 経路 2: JSON パース関数

- `crates/wasm/src/boxes_hev1.rs:96` `parse_json_mp4_sample_entry_hev1`（約 100 行）
- `crates/wasm/src/boxes_hvc1.rs:96` `parse_json_mp4_sample_entry_hvc1`（同 100 行）

差はコーデック名のみ。**issue 0047 が別途「allocate 順序の deferred 化」で書き換えを予定しているため、共通化タイミングは 0047 との調整**（実装時に判断）。

### 経路 3: JSON 出力（NaluArrays::fmt）

- `crates/wasm/src/boxes_hev1.rs:53-93` `struct NaluArrays` と `impl nojson::DisplayJson for NaluArrays`
- `crates/wasm/src/boxes_hvc1.rs:53-93` 完全同一（コメントの「HEVC」の 1 文字だけ差）

### 経路 4: テストの JSON リテラル

- 各ファイルに 6 個のテスト（`test_*_to_json` / `test_json_to_*` / `test_json_to_*_free_{more,fewer,empty}_nalu_arrays`）
- テスト内 JSON リテラルは 16 個の共通フィールド（`width` 〜 `lengthSizeMinusOne`）を繰り返しており、差は `"kind": "hev1"` vs `"kind": "hvc1"` と `naluArrays` の中身だけ

## 設計方針

共通の内部 helper に寄せる。マクロは使わない（`shiguredo-rust` の「マクロを作らないこと」規約）。

`Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` は `crates/c-api/src/boxes.rs` の `#[repr(C)]` 構造体でフィールドレイアウトが完全に同一。共通化は次の 2 通りが考えられ、実装時に判断する:

- **案 A（ジェネリクス + トレイト）**: `fn free_hevc_sample_entry<T: HevcSampleEntry>(entry: &mut T)` + trait `HevcSampleEntry` でフィールドアクセスを抽象化する。ただし `shiguredo-rust` は「トレイトを作らないこと」を規約とするため、許可を取る必要がある
- **案 B（ヘルパー関数の引数化）**: `fn free_hevc_sample_entry_fields(nalu_array_count: &mut u32, nalu_types: &mut *const u8, ...)` として生ポインタを渡す。`*_free` 関数はフィールドを取り出して helper に渡すだけの薄いラッパになる。トレイト不使用

推奨は案 B（トレイト不使用）。

### 経路 3 の共通化

`struct NaluArrays` は完全同一のため、`crates/wasm/src/boxes.rs` などの共通モジュールに 1 つ定義して両ファイルから `use` する。

### 経路 4 の共通化

テストヘルパー `fn build_hevc_test_json(kind: &str, nalu_arrays_json: &str) -> String` を各テストモジュール（あるいは `#[cfg(test)]` 共通モジュール）に置き、6 個のテストの JSON リテラルからヘッダ部分を排除する。

### 他 issue との関係

- **issue 0035**（コアの `src/boxes_sample_entry.rs` と `crates/c-api/src/boxes.rs` の Hev1Box / Hvc1Box 重複を共通化）: レイヤーが異なる（コア + c-api）。本 issue は wasm 側に閉じる
- **issue 0047**（`parse_json_mp4_sample_entry_*` 系 9 関数の allocate 順序 defer 化）: 経路 2 と対象が重なる。0047 の実装時に共通化まで含めるか、完了後に本 issue で共通化するかは実装時に判断

## 完了条件

- 経路 1・3・4 の重複が解消される（経路 2 は 0047 との調整による）
- 公開 API（`pub fn mp4_sample_entry_hev1_free` / `_hvc1_free`、`pub fn parse_json_mp4_sample_entry_hev1` / `_hvc1`、`pub fn fmt_json_mp4_sample_entry_hev1` / `_hvc1`）のシグネチャは変えない
- `cargo test --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz` が pass する
- `cargo clippy --workspace --exclude dump_wasm --exclude transcode_wasm --exclude fuzz --all-targets -- -D warnings` が warning なしで通る

## 解決方法

案 B（トレイト不使用のヘルパー関数化）で対応する場合の手順:

1. `crates/wasm/src/boxes.rs` に共通ヘルパー `free_hevc_sample_entry_fields` を追加する
2. `mp4_sample_entry_hev1_free` / `_hvc1_free` を薄いラッパに置き換える
3. `struct NaluArrays` と `impl nojson::DisplayJson for NaluArrays` を `crates/wasm/src/boxes.rs`（または新規モジュール）に集約し、両ファイルから `use` する
4. テストヘルパー `build_hevc_test_json` を導入し、6 個の JSON リテラルからヘッダ部分を差し替え可能にする
5. `parse_json_mp4_sample_entry_*` の共通化は 0047 完了後、あるいは 0047 に含めて対応
