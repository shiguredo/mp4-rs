# c-api の Avc1/Hev1/Hvc1 to_sample_entry で配列ベースポインタの null 検査が欠落しており UB になる

- Priority: High
- Created: 2026-07-15
- Completed: 2026-07-28
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

### 本体の null 検査追加

1. `crates/c-api/src/boxes.rs` の `Mp4SampleEntryHev1::to_sample_entry` と `Mp4SampleEntryHvc1::to_sample_entry` で、`nalu_array_count > 0` ブロック直後に `nalu_types` / `nalu_counts` の null 検査を追加し、`nalu_count > 0` を確認した内側ループ直前で `nalu_data` / `nalu_sizes` の null 検査を追加した。内側検査は `nalu_count > 0 && (nalu_data.is_null() || nalu_sizes.is_null())` の結合形にして clippy::collapsible_if にも整合させた。
2. 同ファイルの `Mp4SampleEntryAvc1::to_sample_entry` で、`sps_count > 0` ブロック内に `sps_sizes` の null 検査を、`pps_count > 0` ブロック内に `pps_sizes` の null 検査を追加した。`sps_data.is_null()` / `pps_data.is_null()` の位置と条件は既存挙動維持のため据え置き、そのコメントも 1 行残した。
3. `crates/c-api/tests/test_boxes.rs` を新設し、null 経路 9 件、非退行 4 件、正常系 3 件（`nalu_counts = [2, 1, 3]` の非自明な累積で `nalu_data_index` のプレフィックス和を間接検証）、個別要素 null 3 件、早期スキップ 2 件、後続イテレーション境界 2 件、逆パターン 2 件、AVC1 pps 側非退行 1 件など、計 26 テストを追加した。

### レビュー指摘への対応

4. `Mp4Error` に `Debug` / `PartialEq` / `Eq` を derive し、テストを `assert_eq!` 化して失敗時に実測エラー種別を出力できるようにした。
5. `Mp4SampleEntryAvc1` / `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` の型 doc に「ポインタフィールドの null 契約」セクションを追加した。AVC1 と HEV1 / HVC1 の非対称な契約（AVC1 の `sps_data` / `pps_data` は `count == 0` でも非 null 必須、HEV1 / HVC1 の `nalu_data` / `nalu_sizes` は全 `nalu_counts[i] == 0` なら null 許容）を明記した。cbindgen 経由で C ヘッダ `crates/c-api/include/mp4.h` にも自動反映される。
6. `crates/c-api/src/fmp4_segment_mux.rs` の `convert_samples` の戻り値型を `Result<_, &'static str>` から `Result<_, Mp4Error>` に変更した。これまで `.map_err(|_| "sample_entry is invalid")?` で捨てていた `Mp4Error` を `?` で伝播するように直し、`fmp4_segment_muxer_write_media_segment*` 経路でも `MP4_ERROR_NULL_POINTER` 等の種別が呼び出し側で観測できるようにした（従来は一律 `MP4_ERROR_INVALID_INPUT` に丸められていた）。
7. `mp4_file_muxer_append_sample` の `set_last_error` に `{e:?}` を追記し、6 種の新規 null 経路のどれで落ちたかを last_error 文字列から判別できるようにした。
8. HVC1 側の null 検査コメントは HEV1 と一字一句同一だったため、HVC1 の impl 冒頭に「HEV1 を参照」の 1 行コメントを置いて残余は削除した。AVC1 の SPS / PPS null 検査の冗長コメントも情報量が無いため削除した。
9. CHANGES.md のサブ箇条書きの表記を既存慣行に合わせて `` `avc1` `` / `` `hev1` `` / `` `hvc1` `` の小文字バッククォート形に統一した。

### 変更履歴

`CHANGES.md` に 2 つの `[FIX]` エントリを追加した。
- `to_sample_entry` の配列ベースポインタ null 検査追加
- `fmp4_segment_muxer_write_media_segment*` のサンプル変換エラー種別の保持
