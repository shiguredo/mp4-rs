# fuzz_basic_types に SampleFlags を追加する

- Priority: Low
- Created: 2026-05-26
- Completed: 2026-05-26
- Model: Opus 4.7
- Branch: feature/add-fuzz-basic-types-sample-flags

## 目的

`fuzz_basic_types.rs` は `basic_types.rs` 内で `Decode` を実装する全ての基本型を対象とする
fuzz ターゲットだが、`SampleFlags` だけが漏れている。
`fuzz_basic_types.rs` の網羅性を担保するために追加する。

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

## CHANGES.md

fuzz ターゲットの変更は機能に直接影響しない変更のため、`### misc` に記載する。

## 完了条件

- `fuzz/fuzz_targets/fuzz_basic_types.rs` に `SampleFlags` のデコード・エンコードが追加されている
- `cargo fuzz build fuzz_basic_types` が成功する

## 解決方法

- `fuzz/fuzz_targets/fuzz_basic_types.rs` に `SampleFlags` の decode/encode を既存パターンに従って追加した
- import に `SampleFlags` を追加し、`Brand` の decode/encode の後に同じパターンで追記した
- 30 秒間の fuzzing 実行（4,844,853 回）でクラッシュなしを確認した
