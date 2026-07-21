# wasm の boxes_av01 / boxes_mp4a / boxes_flac / boxes_avc1 / boxes_hev1 / boxes_hvc1 で from_raw_parts にサイズ 0 時に null ポインタを渡し UB になる

- Priority: Medium
- Created: 2026-07-20
- Completed: YYYY-MM-DD
- Model: qwen3.8-max-preview
- Branch: feature/fix-wasm-from-raw-parts-zero-size
- Polished: 2026-07-20

## 目的

WASM の JSON フォーマット処理で `std::slice::from_raw_parts(ptr, 0)` を呼び出している箇所がある。`ptr` が null の場合、Rust の仕様上これは UB（未定義動作）である（「even for zero-length slices, the pointer must be non-null and properly aligned」）。`allocate_and_copy_bytes` は空データに対して `(null, 0)` を返すため、JSON → 構造体 → JSON の往復でこの経路が通り得る。

## 優先度根拠

UB であるが、wasm32 ターゲットでは実際にはクラッシュしない可能性が高い。ただし Rust の仕様上は UB であり修正すべき。

## 現状

`allocate_and_copy_bytes`（`crates/wasm/src/boxes.rs:175-177`）は空データに `(null, 0)` を返す:

```rust
if data.is_empty() {
    return (std::ptr::null(), 0);
}
```

以下の 6 ファイルで `from_raw_parts` に null ポインタを渡し得る:

**1. `crates/wasm/src/boxes_av01.rs:29-30`**:

```rust
let config_obus =
    unsafe { std::slice::from_raw_parts(data.config_obus, data.config_obus_size as usize) };
```

**2. `crates/wasm/src/boxes_mp4a.rs:18-20`**:

```rust
let dec_specific_info = unsafe {
    std::slice::from_raw_parts(data.dec_specific_info, data.dec_specific_info_size as usize)
};
```

**3. `crates/wasm/src/boxes_flac.rs:15-17`**:

```rust
let streaminfo = unsafe {
    std::slice::from_raw_parts(data.streaminfo_data, data.streaminfo_size as usize)
};
```

**4. `crates/wasm/src/boxes_avc1.rs:154`**（NaluList 内）:

```rust
let nalu = unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
```

**5. `crates/wasm/src/boxes_hev1.rs:80`**（NaluArrays 内）:

```rust
unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
```

**6. `crates/wasm/src/boxes_hvc1.rs:80`**（NaluArrays 内）:

```rust
unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
```

## 設計方針

サイズ 0 の場合に `&[]` を使う分岐を追加する。av01/mp4a/flac は単純な `if size == 0 { &[] } else { ... }` 分岐。avc1/hev1/hvc1 の NaluList/NaluArrays も各 NALU 単位で同様のサイズ 0 チェックを追加する。

## 完了条件

- 6 ファイルすべての `from_raw_parts` 呼び出しで、サイズ 0 時に null ポインタを渡さない分岐が追加されること
- 空データ（サイズ 0）の JSON 往復テストが追加されること（例: `configObus: []` の JSON を parse して再度 JSON に変換するテスト）
- 既存のテストが通ること
- `cargo clippy` が通ること

## 解決方法

各箇所で `if size == 0 { &[] } else { unsafe { std::slice::from_raw_parts(ptr, size) } }` の分岐を追加する。avc1/hev1/hvc1 の NALU 単位でも同様のガードを追加する。

## 後方互換

UB の除去であり、正常系の実挙動は変わらない。空データの JSON 往復で UB が解消される。

## CHANGES.md

`[FIX]` で記載する。
