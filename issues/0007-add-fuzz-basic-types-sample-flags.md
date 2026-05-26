# fuzz_basic_types に SampleFlags を追加する

- Priority: Low
- Created: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-basic-types-sample-flags

## 目的

`basic_types.rs` の `SampleFlags` は `impl Decode for SampleFlags` (`basic_types.rs:780`) を持つが、
`fuzz_basic_types.rs` に含まれていない。

`fuzz_basic_types.rs` は `FullBoxHeader` / `FullBoxFlags` / `Utf8String` / `FixedPointNumber` / `Brand` を
対象としており、`Decode` を実装する基本型のパニック安全性を検証する役割を担っている。
`SampleFlags` も同じ役割に含まれるべき。

## 優先度根拠

`SampleFlags` は `fuzz_trun_box.rs` 経由で `TrunSample` 内の一部として間接的にカバーされている。
直接的なカバレッジがないだけで、パニック安全性の穴が大きいわけではない。Low とする。

## 現状

`fuzz_basic_types.rs` の対象型:

| 型 | 対象 |
|---|---|
| `FullBoxHeader` | 含まれている |
| `FullBoxFlags` | 含まれている |
| `Utf8String` | 含まれている |
| `FixedPointNumber<u16, u16>` | 含まれている |
| `Brand` | 含まれている |
| **`SampleFlags`** | **含まれていない** |

## 設計方針

`fuzz_basic_types.rs` の既存パターンに `SampleFlags` のデコード・エンコードを追加する:

```rust
if let Ok((flags, _)) = SampleFlags::decode(data) {
    let _ = flags.encode_to_vec();
}
```

## 完了条件

- `fuzz/fuzz_targets/fuzz_basic_types.rs` に `SampleFlags` のデコード・エンコードが追加されている
- `cargo fuzz build fuzz_basic_types` が成功する
