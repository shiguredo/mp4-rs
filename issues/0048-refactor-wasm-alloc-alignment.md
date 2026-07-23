# WASM の mp4_alloc / allocate_and_copy_* を各要素型のアラインメントに合わせて確保するように直す

- Priority: Medium
- Created: 2026-07-23
- Completed: YYYY-MM-DD
- Model: Opus 4.7
- Branch: feature/refactor-wasm-alloc-alignment
- Polished: YYYY-MM-DD

## 目的

WASM 側の `mp4_alloc` (`crates/wasm/src/lib.rs:37-45`) は `Layout::from_size_align(size, 1)` で確保しており、返り値のアドレスは 1 バイト境界しか保証しない。その領域を `allocate_and_copy_u16_array` / `allocate_and_copy_array_list` 経由で `*const u16` / `*const u32` / `*const *const u8` にキャストして露出し、後段で aligned な `u16` / `u32` / `*const u8` として `*ptr.add(i)` や `std::slice::from_raw_parts(ptr, n)` する現在の設計は Rust semantic 上 UB（`std::alloc::alloc` の契約は `layout.align()` 以上に整列することのみで、align 1 は最低保証しかない）。

`mp4_alloc` に `align` 引数を追加するか、`allocate_and_copy_*` 内部で `Layout::from_size_align(size, mem::align_of::<T>())` を通す実装に refactor して契約違反を解消する。

## 優先度根拠

Medium。実運用（wasm32 の `std::alloc::alloc` はほとんどのケースで 8 バイト境界を返す）では顕在化せず、既存 `boxes_avc1.rs` / `boxes_hev1.rs` / `boxes_hvc1.rs` などが問題なく動作している。ただし Rust の UB 契約違反として残っており、`clippy::mem_aligned_reads` や `miri` で問題化しうる。他 UB 修正 issue（`issues/0034-bug-wasm-from-raw-parts-zero-size.md`）と同水準の優先度。

## 現状

`crates/wasm/src/lib.rs:37-45`:

```rust
pub extern "C" fn mp4_alloc(size: u32) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align(size as usize, 1)
        .expect("layout creation with alignment 1 should never fail");
    unsafe { std::alloc::alloc(layout) }
}
```

`crates/wasm/src/lib.rs:56-64` の `mp4_free` も同じく `align = 1` で `dealloc` する。

`crates/wasm/src/boxes.rs:219-234` の `allocate_and_copy_bytes` は `mp4_alloc` の返り値をそのまま `*const u8` として露出しており、u8 用途では alignment 問題なし。

一方、以下は 1 バイト境界の領域をより大きな型として露出する。

- `crates/wasm/src/boxes.rs:291-305` `allocate_and_copy_u16_array`: 内部で `mp4_alloc(byte_size)` を呼び返り値を `*const u16` にキャストして返す。1 バイト境界の領域を `u16` として dereference する経路を作る
- `crates/wasm/src/boxes.rs:239-267` `allocate_and_copy_array_list`: `data_ptrs` (`Vec<*const u8>`) を `allocate_and_copy_bytes` で確保し `*const *const u8` にキャスト、`sizes` (`Vec<u32>`) を同様に確保し `*const u32` にキャストする。ポインタサイズ (wasm32 で 4 バイト、align 4) と `u32` (align 4) の要件を 1 バイト境界の領域が満たさない

上記領域を後段で読む箇所（差分・実測）:

- `crates/c-api/src/boxes.rs:1774-1780` `Mp4SampleEntryTx3g::to_sample_entry`: `std::slice::from_raw_parts(self.ftab_font_ids, ...)` で `*const u16` を slice にする
- `crates/wasm/src/boxes_tx3g.rs:152` `FtabList::fmt`: `unsafe { *self.font_ids.add(i) }` で `u16` を read
- `crates/c-api/src/boxes.rs` の他 `to_sample_entry`（Avc1 / Hev1 / Hvc1 / Av01 / Mp4a / Flac 等）: `sizes: *const u32` と `data_ptrs: *const *const u8` の slice 化と要素 read

`std::alloc::alloc` の docs は「The allocated block of memory may or may not be initialized... `layout.align()` must always be a power of 2 greater than zero. The returned pointer must be non-null and aligned to `layout.align()`.」と定めており、align 1 で確保した領域を align 2/4 として dereference するのは UB。wasm32-unknown-unknown の `std::alloc::alloc` 実装（dlmalloc）は事実上 8 バイト境界返却で顕在化しないが、契約違反は残る。

## 設計方針

以下の 2 案から実装時に選ぶ:

### 案 A: `mp4_alloc` / `mp4_free` に align 引数を追加

```rust
pub extern "C" fn mp4_alloc(size: u32, align: u32) -> *mut u8 {
    // align の妥当性（2 の冪、> 0）を検証
    // Layout::from_size_align(size, align) で確保
}

pub unsafe extern "C" fn mp4_free(ptr: *mut u8, size: u32, align: u32) {
    // 同じ align で dealloc
}
```

- C ABI シグネチャ変更 (breaking change) が発生。既存 C consumer が引数追加に追従する必要あり
- 呼び出し側 (`allocate_and_copy_*`) が `mem::align_of::<T>()` を渡す
- cbindgen で `mp4.h` のシグネチャが更新されるため、既存 C consumer が対応する必要あり

### 案 B: `allocate_and_copy_*` 内部で `std::alloc::alloc` を直接呼ぶ（`mp4_alloc` は u8 用途のまま維持）

```rust
pub fn allocate_and_copy_u16_array(data: &[u16]) -> (*const u16, u32) {
    if data.is_empty() {
        return (std::ptr::null(), 0);
    }
    let byte_size = std::mem::size_of_val(data);
    let layout = Layout::from_size_align(byte_size, std::mem::align_of::<u16>())
        .expect("u16 の layout は失敗しない");
    let allocated = unsafe { std::alloc::alloc(layout) };
    if allocated.is_null() {
        return (std::ptr::null(), 0);
    }
    unsafe {
        std::ptr::copy_nonoverlapping(data.as_ptr() as *const u8, allocated, byte_size);
    }
    (allocated as *const u16, data.len() as u32)
}
```

- `mp4_alloc` の C ABI シグネチャは変わらない（後方互換）
- 対応する `free_*_array` / `free_array_list` は `Layout::from_size_align(size, mem::align_of::<T>())` で `dealloc` を呼ぶ
- 案 A より変更範囲が狭い

案 B の方が変更範囲が小さく後方互換なため、実装時に案 B を第一候補とする。

## 完了条件

- `allocate_and_copy_u16_array` / `free_u16_array` / `allocate_and_copy_array_list` / `free_array_list` が要素型のアライメント要件を満たす境界で確保・解放する
- `Layout::from_size_align(_, 1)` を `*const u16` / `*const u32` / `*const *const u8` として露出する経路が残らない
- 既存の WASM テスト (`cargo test -p wasm`) がすべて pass する
- `cargo test --workspace` がすべて pass する
- `cargo clippy --all-targets --all-features -- -D warnings` が warning なしで通る

## 解決方法

以下の順で対応する（案 B 採用時）:

1. `crates/wasm/src/boxes.rs` の `allocate_and_copy_u16_array` を `Layout::from_size_align(byte_size, mem::align_of::<u16>())` + `std::alloc::alloc` の直接呼び出しに書き換える
2. 同じく `free_u16_array` を `std::alloc::dealloc` に置き換える
3. `allocate_and_copy_array_list` は `data_ptrs` / `sizes` を `Layout::from_size_align(size, mem::align_of::<*const u8>())` / `Layout::from_size_align(size, mem::align_of::<u32>())` で確保するように書き換える
4. `free_array_list` を同じ align で `dealloc` するように追従する
5. `cargo test` と `cargo clippy` で回帰を確認する

## CHANGES.md

`[UPDATE]` として記載する。挙動変化はなく、Rust semantic 上の UB 契約違反を解消する内部修正のため。C ABI（`mp4_alloc` / `mp4_free`）のシグネチャは維持する。
