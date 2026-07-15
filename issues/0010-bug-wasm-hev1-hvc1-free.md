# wasm の hev1/hvc1 sample entry free が mp4_free(ptr, 0) で no-op になりリーク + free_array_list の count 不一致で UB になる

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-wasm-hev1-hvc1-free
- Polished: YYYY-MM-DD

## 目的

WebAssembly 版の HEV1 / HVC1 サンプルエントリのメモリ解放処理が壊れており、(1) `mp4_free(ptr, 0)` で常に no-op になりヒープリークする、(2) `free_array_list` に渡す count が確保時の総数と不一致で dealloc サイズが合わずヒープ破壊 UB になる、これら 2 つの問題を修正する。

## 優先度根拠

ヒープ破壊（UB）は即座にクラッシュ・セキュリティリスクに直結する。ストリーミング用途で sample entry を繰り返し parse/free するたびにリークが蓄積し、長時間実行で OOM にも至る。wasm の公開 API 経路で確実に発火するため High。

## 現状

### 問題 1: mp4_free(ptr, 0) で no-op（リーク）

```rust
// crates/wasm/src/lib.rs:55-58
pub unsafe extern "C" fn mp4_free(ptr: *mut u8, size: u32) {
    if ptr.is_null() || size == 0 {
        return;
    }
```

```rust
// crates/wasm/src/boxes_hev1.rs:203-214
if !entry.nalu_types.is_null() {
    unsafe {
        crate::mp4_free(entry.nalu_types.cast_mut(), 0);
    }
    // ...
}
if !entry.nalu_counts.is_null() {
    unsafe {
        crate::mp4_free(entry.nalu_counts.cast_mut() as *mut u8, 0);
    }
```

`mp4_free` は `size == 0` で即 return する。一方 free 側は常に `size = 0` を渡している。確保側（`allocate_and_copy_bytes`）は実サイズで確保しているため、解放されずに常にリークする。`boxes_hvc1.rs:203-214` も同型。

### 問題 2: free_array_list の count 不一致（UB）

```rust
// crates/wasm/src/boxes_hev1.rs:136-137 (確保側)
let (nalu_data, nalu_sizes, _) = crate::boxes::allocate_and_copy_array_list(&nalu_data_vec);
```

```rust
// crates/wasm/src/boxes_hev1.rs:191
    nalu_array_count: nalu_types_vec.len() as u32,
```

```rust
// crates/wasm/src/boxes_hev1.rs:219-222 (解放側)
crate::boxes::free_array_list(
    entry.nalu_data as *mut *mut u8,
    entry.nalu_sizes as *mut u32,
    entry.nalu_array_count,
);
```

`allocate_and_copy_array_list` は全 NALU を平坦化したリスト（`nalu_data_vec`）で確保するため、戻り値の `count` は **全 NALU 総数** である。しかし保存する `nalu_array_count` は **配列数**（`nalu_types_vec.len()`）である。free は `nalu_array_count`（配列数）を渡す。

1 配列に複数 NALU があると:
- 余剰 NALU バッファが未解放（リーク）
- ポインタ配列・sizes 配列を確保サイズと異なる layout で `dealloc` → UB

各 array に 1 unit しかない場合は偶然一致して表に出にくい。

## 設計方針

- 問題 1: `mp4_free` に確保時の実バイト数を渡す。`nalu_types` は `nalu_types_vec.len()` バイト、`nalu_counts` は `nalu_counts_vec.len() * size_of::<u32>()` バイト
- 問題 2: 全 NALU 総数を保持するフィールドを追加するか、`nalu_data` / `nalu_sizes` を配列数ではなく総数で管理する。`free_array_list` には確保時と同じ count を渡す

## 完了条件

- HEV1 / HVC1 sample entry を parse → free したときにメモリリークが発生しないこと
- 1 配列に複数 NALU がある場合でも正しく解放されること
- 既存の wasm テストが通ること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `crates/wasm/src/boxes_hev1.rs` / `boxes_hvc1.rs` の `mp4_sample_entry_*_free` で `mp4_free` に実サイズを渡す
2. `Mp4SampleEntryHev1` / `Mp4SampleEntryHvc1` に全 NALU 総数を保持するフィールドを追加する（または既存フィールドの意味を見直す）
3. `free_array_list` に確保時と同じ count を渡す
4. 1 配列・複数 NALU の parse → free を検証するテストを追加する
