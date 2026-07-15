# c-api の Hev1/Hvc1 to_sample_entry で配列ベースポインタの null 検査が欠落しており UB になる

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-capi-hev1-hvc1-null-check
- Polished: YYYY-MM-DD

## 目的

C API の `Mp4SampleEntryHev1::to_sample_entry` / `Mp4SampleEntryHvc1::to_sample_entry` が、`nalu_array_count > 0` のとき配列ベースポインタ（`nalu_types` / `nalu_counts` / `nalu_data` / `nalu_sizes`）の null 検査をせずにデリファレンスし、C 呼び出し側が null を渡したときに即 UB になる問題を修正する。

## 優先度根拠

FFI 境界での UB は segfault・セキュリティリスクに直結する。C 側の不正入力（count > 0 で配列ポインタが null）で即座に未定義動作になる。AVC1 は `sps_data` / `pps_data` を検査しているのに HEV1 / HVC1 だけ抜けており、一貫性も欠いている。High。

## 現状

```rust
// crates/c-api/src/boxes.rs:773-783 (Hev1)
if self.nalu_array_count > 0 {
    unsafe {
        for i in 0..self.nalu_array_count as usize {
            let nalu_type = *self.nalu_types.add(i);
            let nalu_count = *self.nalu_counts.add(i);
            // ...
                let nalu_ptr = *self.nalu_data.add(nalu_index);
                let nalu_size = *self.nalu_sizes.add(nalu_index) as usize;

                if nalu_ptr.is_null() {
                    return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
                }
```

個々の `nalu_ptr` は null 検査するが、`nalu_types` / `nalu_counts` / `nalu_data` / `nalu_sizes` 自体が null のとき未検査で `add` / 間接参照する。`Hvc1`（`crates/c-api/src/boxes.rs:914-929`）も同型。

対照的に `Mp4SampleEntryAvc1` は `sps_data` / `pps_data` を先に `is_null()` 検査している（`crates/c-api/src/boxes.rs:642-661`）。ただし AVC1 も `sps_sizes` / `pps_sizes` は count > 0 で未検査であり、同型の問題がある。

## 設計方針

`nalu_array_count > 0`（または `sps_count > 0` / `pps_count > 0`）のとき、各ベースポインタを null 検査し、null の場合は `Mp4Error::MP4_ERROR_NULL_POINTER` を返す。AVC1 / HEV1 / HVC1 で一貫した検査パターンに揃える。

## 完了条件

- `nalu_array_count > 0` で配列ポインタが null のとき UB ではなく `MP4_ERROR_NULL_POINTER` が返ること
- AVC1 の `sps_sizes` / `pps_sizes` も同様に検査されること
- 既存の正常系テストが通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `crates/c-api/src/boxes.rs` の Hev1 / Hvc1 `to_sample_entry` で count > 0 時に `nalu_types` / `nalu_counts` / `nalu_data` / `nalu_sizes` の null 検査を追加する
2. Avc1 `to_sample_entry` の `sps_sizes` / `pps_sizes` にも null 検査を追加する
3. null 渡しで `MP4_ERROR_NULL_POINTER` が返るテストを追加する
