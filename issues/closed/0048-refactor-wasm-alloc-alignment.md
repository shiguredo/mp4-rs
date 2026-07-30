# WASM の mp4_alloc / allocate_and_copy_* を各要素型のアラインメントに合わせて確保するように直す

- Priority: Medium
- Created: 2026-07-23
- Completed: 2026-07-30
- Model: Opus 4.7
- Branch: feature/refactor-wasm-alloc-alignment
- Polished: 2026-07-30

## 目的

WASM 側の `mp4_alloc`（`crates/wasm/src/lib.rs`）は `Layout::from_size_align(size, 1)` で確保しており、返り値のアドレスは 1 バイト境界しか保証しない。その領域を `u16` / `u32` / ポインタ配列として読み出す経路があり、Rust のアロケーション契約上 UB である（`std::alloc::alloc` は `layout.align()` 以上の整列のみを保証する）。

`mp4_alloc` の C ABI は変えず、typed 配列の確保・解放を要素型の `align_of::<T>()` に合わせて直し、契約違反を解消する。

## 優先度根拠

Medium。実運用（wasm32 の `std::alloc::alloc` は多くのケースで 8 バイト境界を返す）では顕在化しにくく、既存の `boxes_avc1` / `boxes_hev1` / `boxes_hvc1` などは動作している。ただし Rust の UB 契約違反として残り、`miri` やアラインメント関連の静的検査で問題化しうる。同種の UB 修正である `issues/0034-bug-wasm-from-raw-parts-zero-size.md` と同水準とする。

`issues/closed/0010-bug-wasm-hev1-hvc1-free.md` は、`nalu_counts` を `u32` として読む経路が増える一方で確保は align 1 のまま残るため、アラインメント UB の解消を本 issue のスコープに含める前提を明記している。

## 現状

`mp4_alloc` / `mp4_free`（`crates/wasm/src/lib.rs`）はいずれも `Layout::from_size_align(..., 1)` を使う。

`allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs`）は `mp4_alloc` の返り値を `*const u8` として露出するだけなので、u8 用途では alignment 問題はない。

一方、次の経路は align 1 の領域をより大きな型として露出・読み出す。

1. `allocate_and_copy_u16_array`（`crates/wasm/src/boxes.rs`）: `mp4_alloc` の返り値を `*const u16` にキャストして返す。対応する `free_u16_array` も `mp4_free`（align 1）で解放する
2. `allocate_and_copy_array_list`（同）: `data_ptrs`（`Vec<*const u8>`）と `sizes`（`Vec<u32>`）をそれぞれ `allocate_and_copy_bytes` 経由で確保し、`*const *const u8` / `*const u32` にキャストする。対応する `free_array_list` も `mp4_free`（align 1）で解放する
3. `parse_json_mp4_sample_entry_hev1` / `parse_json_mp4_sample_entry_hvc1`（`crates/wasm/src/boxes_hev1.rs` / `boxes_hvc1.rs`）: `nalu_counts_vec: Vec<u32>` を `allocate_and_copy_bytes` で確保し、返り値を `*const u32` にキャストして `Mp4SampleEntryHev1` / `Hvc1` に格納する。解放は `mp4_sample_entry_hev1_free` / `mp4_sample_entry_hvc1_free` が `mp4_free`（align 1）を呼ぶ

上記領域を後段で読む例:

- WASM: `FtabList` の `DisplayJson::fmt`（`crates/wasm/src/boxes_tx3g.rs`）が `*self.font_ids.add(i)` で `u16` を読む
- WASM: `NaluArrays` の `DisplayJson::fmt`（`boxes_hev1.rs` / `boxes_hvc1.rs`）が `*self.nalu_counts.add(i)` で `u32` を読む
- WASM: `mp4_sample_entry_hev1_free` / `hvc1_free` が `from_raw_parts(entry.nalu_counts, ...)` で `u32` 列を読む
- C API（wasm と型共有）: `Mp4SampleEntryTx3g::to_sample_entry` が `ftab_font_ids: *const u16` を `from_raw_parts` する。`Mp4SampleEntryAvc1` / `Hev1` / `Hvc1` の `to_sample_entry` 系が `*_sizes: *const u32` と `*_data: *const *const u8` を slice 化する（ポインタが wasm の上記確保経路由来のとき、同じアラインメント契約違反が後段に伝播する）

`Av01` / `Mp4a` / `Flac` のサンプルエントリーは `*const u8` バッファのみを読むため、本 issue の「より大きな型として読む」経路には含めない。

## 設計方針

**案 B を採用する**（案 A は採用しない）。

### 採用: 案 B（`mp4_alloc` の ABI は維持）

typed 配列の確保・解放は `allocate_and_copy_*` / `free_*`、および `hev1` / `hvc1` の `nalu_counts` 経路で、`Layout::from_size_align(size, mem::align_of::<T>())` と `std::alloc::alloc` / `dealloc` を直接使う。

- `mp4_alloc` / `mp4_free` の C ABI シグネチャは変えない（u8 用途のまま）
- 確保と解放の align は必ず対にする（align 4 で確保した領域を align 1 の `mp4_free` で解放しない）
- `nalu_counts` は専用ヘルパ（例: `allocate_and_copy_u32_array` / `free_u32_array`）を `boxes.rs` に足して `hev1` / `hvc1` から使うか、同等の layout 処理を両 free / parse に直接書く。重複を避けるならヘルパ追加を優先する

### 不採用: 案 A（`mp4_alloc` / `mp4_free` に align 引数）

C ABI 破壊（breaking change）と cbindgen / 既存 C consumer 追従が必要で、変更範囲が広い。本 issue では採らない。

## 完了条件

- `allocate_and_copy_u16_array` / `free_u16_array` / `allocate_and_copy_array_list` / `free_array_list` が、露出する要素型のアライメント要件を満たす境界で確保・解放する
- `parse_json_mp4_sample_entry_hev1` / `hvc1` の `nalu_counts` 確保と、`mp4_sample_entry_hev1_free` / `hvc1_free` の `nalu_counts` 解放が、`u32` のアライメントで対になっている
- `Layout::from_size_align(_, 1)`（または同等の align 1 確保）を `*const u16` / `*const u32` / `*const *const u8` として露出する経路が、上記対象に残らない
- 既存の WASM テスト（`cargo test -p wasm`）がすべて pass する
- `cargo test --workspace` がすべて pass する
- `cargo clippy --all-targets --all-features -- -D warnings` が warning なしで通る

## 解決方法

案 B で次の順に対応する。

1. `crates/wasm/src/boxes.rs` の `allocate_and_copy_u16_array` を `Layout::from_size_align(byte_size, align_of::<u16>())` + `std::alloc::alloc` に書き換える
2. 同じく `free_u16_array` を同じ align の `std::alloc::dealloc` に置き換える（`mp4_free` を使わない）
3. `allocate_and_copy_array_list` の `data_ptrs` / `sizes` をそれぞれ `align_of::<*const u8>()` / `align_of::<u32>()` で確保する
4. `free_array_list` を同じ align で `dealloc` する（要素バイト列そのものは従来どおり `mp4_free` でよい。こちらは `*const u8` 用途）
5. `nalu_counts` 用に `allocate_and_copy_u32_array` / `free_u32_array`（名称は実装時に揃えてよい）を `boxes.rs` に追加するか、同等処理を入れる
6. `parse_json_mp4_sample_entry_hev1` / `hvc1` の `nalu_counts` 確保を手順 5 の経路に切り替え、`mp4_sample_entry_hev1_free` / `hvc1_free` の解放も同じ align の `dealloc`（または `free_u32_array`）に切り替える
7. `cargo test` と `cargo clippy` で回帰を確認する

## CHANGES.md

`[FIX]` として `## develop` 直下の wasm FIX 群の隣に記載する。Rust semantic 上の UB 契約違反を解消する潜在バグ修正であり、同じ hev1 / hvc1 領域の `[FIX] wasm の mp4_sample_entry_hev1_free() / mp4_sample_entry_hvc1_free() のメモリ解放の不一致を修正する` と同水準の bug fix として扱う。C ABI（`mp4_alloc` / `mp4_free`）のシグネチャは維持する。

当初 `[UPDATE]` / `### misc` として記載したが、`/review-diff-code` の重要指摘（同 CHANGES.md の分類・配置規約に反する）により `[FIX]` / `## develop` 直下へ移動した。

## 実装補足

merge 前提の追記。issue 0056（`crates/wasm` の `hev1` / `hvc1` 共通化）が本 issue の作業中に develop へ merge され、`nalu_counts` の確保・解放が個別ファイルから `crates/wasm/src/boxes.rs` の共通ヘルパ `parse_json_hevc_sample_entry_fields` / `free_hevc_sample_entry_fields` へ移動した。共通ヘルパは合流時点で旧経路（`allocate_and_copy_bytes` + `mp4_free`）に戻っていたため、本 issue の align 契約修正を共通ヘルパ側にも再適用している。個別ファイル（`boxes_hev1.rs` / `boxes_hvc1.rs`）は共通ヘルパへの委譲だけを持つ形になった。
