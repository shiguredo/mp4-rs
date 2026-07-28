# c-api の Avc1/Hev1/Hvc1 to_sample_entry で配列ベースポインタの null 検査が欠落しており UB になる

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-capi-sample-entry-null-check
- Polished: 2026-07-28

## 目的

C API の `Mp4SampleEntryAvc1::to_sample_entry` / `Mp4SampleEntryHev1::to_sample_entry` / `Mp4SampleEntryHvc1::to_sample_entry` が、`count > 0` のときに配列ベースポインタの null 検査を欠いており、C 呼び出し側が null を渡したときに UB になる問題を修正する。対象は以下 3 種。

- AVC1: `sps_count > 0` のとき `sps_sizes`、`pps_count > 0` のとき `pps_sizes` が未検査（`sps_data` / `pps_data` は既に無条件で検査済み）
- HEV1: `nalu_array_count > 0` のとき `nalu_types` / `nalu_counts` / `nalu_data` / `nalu_sizes` の 4 本すべてが未検査
- HVC1: HEV1 と同型

## 優先度根拠

FFI 境界での UB は segfault やセキュリティリスクに直結する。C 側の不正入力（`count > 0` で配列ポインタが null）で即座に未定義動作になる。closed の 0028（`issues/closed/0028-bug-capi-avc1-sps-pps-null-check.md`）で「本 issue が AVC1 の `sps_sizes` / `pps_sizes` null 検査を包含する」と決定済みであり、AVC1 側も本 issue で対応する。High。

## 現状

### HEV1（`crates/c-api/src/boxes.rs:920-947` `Mp4SampleEntryHev1::to_sample_entry`）

```rust
if self.nalu_array_count > 0 {
    unsafe {
        for i in 0..self.nalu_array_count as usize {
            let nalu_type = *self.nalu_types.add(i);
            let nalu_count = *self.nalu_counts.add(i);

            let mut nalus = Vec::new();
            for j in 0..nalu_count as usize {
                let nalu_index = self.nalu_data_index(i, j);
                let nalu_ptr = *self.nalu_data.add(nalu_index);
                let nalu_size = *self.nalu_sizes.add(nalu_index) as usize;

                if nalu_ptr.is_null() {
                    return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
                }
```

個々の `nalu_ptr` は null 検査するが、`nalu_types` / `nalu_counts` / `nalu_data` / `nalu_sizes` 自体が null のとき未検査で `add` / 間接参照する。加えて `Mp4SampleEntryHev1::nalu_data_index`（`crates/c-api/src/boxes.rs:982-994`）も内部で `*self.nalu_counts.add(i)` を実行しており、`nalu_counts` が null なら同様に UB になる。

### HVC1（`crates/c-api/src/boxes.rs:1061-1088` `Mp4SampleEntryHvc1::to_sample_entry`、`nalu_data_index` は `:1123-1135`）

HEV1 と同型のロジックであり、同じ 4 本のベースポインタが未検査で参照される。

### AVC1（`crates/c-api/src/boxes.rs:785-820` `Mp4SampleEntryAvc1::to_sample_entry`）

```rust
let mut sps_list = Vec::new();
if self.sps_data.is_null() {
    return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
}
if self.sps_count > 0 {
    unsafe {
        for i in 0..self.sps_count as usize {
            let sps_ptr = *self.sps_data.add(i);
            let sps_size = *self.sps_sizes.add(i) as usize;
```

`sps_data` / `pps_data` は `count` の値に関わらず無条件で `is_null()` 検査されているが、`sps_sizes` / `pps_sizes` は `sps_count > 0` / `pps_count > 0` のときでも未検査で `add` / 間接参照する。

## 設計方針

以下の 2 点を守り、既存挙動を壊さずに未検査のベースポインタだけを補うことを方針とする。

- **AVC1**: 既存の `sps_data.is_null()` / `pps_data.is_null()` の位置と条件（無条件検査）は据え置く。追加するのは `sps_count > 0` / `pps_count > 0` ブロック内での `sps_sizes` / `pps_sizes` の null 検査のみ。`count == 0` かつ `sps_data == NULL` を弾く既存の挙動は維持する。
- **HEV1 / HVC1**: ループ内で実際に参照される順に応じて null 検査を追加する。
  - `nalu_types` / `nalu_counts` は `nalu_array_count > 0` の外側ループの各周で必ず参照されるため、`nalu_array_count > 0` ブロックに入った直後（外側ループ開始前）で無条件に null 検査する。`nalu_data_index` 内の `nalu_counts.add(i)` もこの検査でカバーされる。
  - `nalu_data` / `nalu_sizes` は少なくとも 1 つの `nalu_counts[i] > 0` のときのみ内側ループで参照される。`nalu_array_count > 0` かつ全 `nalu_counts[i] == 0` の入力（HEVC 仕様上ありうる空 NALU 配列のみの `hvcC`）を過剰に弾かないため、内側ループに入る直前（`nalu_count > 0` を確認した直後）で null 検査する。
- いずれも null の場合は `Mp4Error::MP4_ERROR_NULL_POINTER` を返す（既存の `sps_data` / `pps_data` / `nalu_ptr` と同じエラー）。

## 完了条件

- AVC1: `sps_count > 0` かつ `sps_sizes` が null のとき、および `pps_count > 0` かつ `pps_sizes` が null のとき、`Mp4Error::MP4_ERROR_NULL_POINTER` が返ること
- HEV1 / HVC1: `nalu_array_count > 0` かつ `nalu_types` または `nalu_counts` が null のとき、および `nalu_count > 0` の内側ループに入る直前で `nalu_data` または `nalu_sizes` が null のとき、`Mp4Error::MP4_ERROR_NULL_POINTER` が返ること
- AVC1 の既存挙動（`sps_count == 0` かつ `sps_data == NULL` で `MP4_ERROR_NULL_POINTER` を返す等）が退行していないこと
- 上記を検証する新規テストが `crates/c-api/tests/test_boxes.rs`（新設）で通ること
- 既存の `crates/c-api/tests/e2e.rs` が通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `crates/c-api/src/boxes.rs` の `Mp4SampleEntryHev1::to_sample_entry`（`:916-995`）と `Mp4SampleEntryHvc1::to_sample_entry`（`:1057-1136`）で、`nalu_array_count > 0` ブロック直後に `nalu_types` / `nalu_counts` の null 検査を追加し、内側ループに入る直前（`nalu_count > 0` を確認後）に `nalu_data` / `nalu_sizes` の null 検査を追加する。
2. 同ファイルの `Mp4SampleEntryAvc1::to_sample_entry`（`:785-854`）で、`sps_count > 0` ブロック内の `sps_sizes`、`pps_count > 0` ブロック内の `pps_sizes` の null 検査を追加する。`sps_data.is_null()` / `pps_data.is_null()` の位置と条件は変更しない。
3. `crates/c-api/tests/test_boxes.rs` を新設し、`Mp4SampleEntry`（公開 API）経由で `to_sample_entry` を呼ぶ統合テストを追加する。null 渡しで `Mp4Error::MP4_ERROR_NULL_POINTER` が返ることを AVC1 / HEV1 / HVC1 の各対象ポインタで検証する。テスト関数名は英語、テスト内のメッセージは日本語で書く（`AGENTS.md` および `shiguredo-rust` の規約）。
