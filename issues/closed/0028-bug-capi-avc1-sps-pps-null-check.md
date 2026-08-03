# c-api の Avc1 to_sample_entry で sps_sizes / pps_sizes の null 検査が欠落しており UB になる

- Priority: High
- Created: 2026-07-20
- Completed: 2026-07-21
- Model: qwen3.8-max-preview
- Branch: feature/fix-capi-avc1-sps-pps-null-check
- Polished: YYYY-MM-DD

## 目的

C API の `Mp4SampleEntryAvc1::to_sample_entry()` において、`sps_data` の null チェックはあるが `sps_sizes` の null チェックが欠落している。`pps_data` / `pps_sizes` も同様。`sps_count > 0` かつ `sps_sizes` が null の場合、null ポインタ参照外し（UB / クラッシュ）が発生する。

既存の 0011-bug-capi-hev1-hvc1-null-check.md は Hev1/Hvc1 の問題であり、Avc1 は対象外。本 issue は Avc1 の null チェック欠落を修正する。

## 優先度根拠

C API は外部から呼び出される境界であり、不正な構造体が渡された場合に UB（未定義動作）が発生する。0011 と同じパターンであり、同様の優先度で対応すべき。

## 現状

`crates/c-api/src/boxes.rs` の `Mp4SampleEntryAvc1::to_sample_entry()` において:

```rust
if self.sps_data.is_null() {
    return Err(Mp4Error::MP4_ERROR_NULL_POINTER);
}
if self.sps_count > 0 {
    unsafe {
        for i in 0..self.sps_count as usize {
            let sps_ptr = *self.sps_data.add(i);
            let sps_size = *self.sps_sizes.add(i) as usize; // sps_sizes の null チェックなし
```

- `sps_data` は null チェックされているが、`sps_sizes` はされていない
- `pps_data` は null チェックされているが、`pps_sizes` はされていない
- 個別の `sps_ptr` / `pps_ptr` の null チェックはある

## 設計方針

`sps_count > 0` の場合に `sps_sizes.is_null()` を、`pps_count > 0` の場合に `pps_sizes.is_null()` をチェックし、null なら `Mp4Error::MP4_ERROR_NULL_POINTER` を返す。

## 完了条件

- `sps_sizes` / `pps_sizes` が null の場合に `MP4_ERROR_NULL_POINTER` エラーが返ること
- 既存のテストが通ること

## 解決方法

issue 0011（`0011-bug-capi-hev1-hvc1-null-check.md`）と重複するため closed にする。

0011 はタイトルこそ Hev1/Hvc1 だが、本文の完了条件に「AVC1 の `sps_sizes` / `pps_sizes` も同様に検査されること」、解決方法に「Avc1 `to_sample_entry` の `sps_sizes` / `pps_sizes` にも null 検査を追加する」が明記されており、本 issue の要求を完全に包含している。本 issue の「0011 は Avc1 対象外」という主張は 0011 の本文と矛盾する事実誤認であった。0011 の実装に集約する。
