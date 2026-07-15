# boxes_fmp4.rs の trun version 0 の composition_time_offset を as i32 で格納しており符号が化ける

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-trun-v0-cto-as-i32
- Polished: YYYY-MM-DD

## 目的

`TrunBox` の decode で version 0 の `composition_time_offset`（仕様上 unsigned 32-bit）を `as i32` で格納しており、`> i32::MAX` の正当な値が負値に化ける問題を修正する。

## 優先度根拠

ISO/IEC 14496-12 で version 0 は unsigned、version 1 は signed。格納型が `Option<i32>` のため u32 全域を保持できず、境界で黙って破壊的変換している。`ctts` 側は `i64` で保持しており表現力が不整合で、fMP4 demux の `composition_time_offset` が汚染され PTS が誤る。

## 現状

```rust
// src/boxes_fmp4.rs:732-738
let composition_time_offset =
    if flags & Self::FLAG_SAMPLE_COMPOSITION_TIME_OFFSETS_PRESENT != 0 {
        if version == 1 {
            Some(i32::decode_at(payload, &mut offset)?)
        } else {
            Some(u32::decode_at(payload, &mut offset)? as i32)
        }
    } else {
        None
    };
```

```rust
// src/boxes_fmp4.rs:790
pub composition_time_offset: Option<i32>,
```

version 0 で `u32 as i32` は `0x8000_0000 ..= 0xFFFF_FFFF` を負の i32 に再解釈する。対照的に `CttsEntry.sample_offset` は `i64` で version 0 を `u32 as i64` で保持している（`src/boxes_moov_tree.rs:1878-1879`）。

## 設計方針

`TrunSample.composition_time_offset` の型を `Option<i64>` に変更し、version 0 は `u32 as i64`、version 1 は `i32 as i64` で格納する。`ctts` 側と表現力を揃える。公開 API の `composition_time_offset` は既に `i64` で公開されている箇所があり、整合する。

encode 側も併せて修正する（version 判定 `uses_version_1` と encode 処理）。

## 完了条件

- version 0 の `composition_time_offset` が `> i32::MAX` でも正しく保持されること
- version 1 の signed 値も従来どおり正しく保持されること
- encode / decode の往復が正しいこと
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `TrunSample.composition_time_offset` の型を `Option<i64>` に変更する
2. decode で version 0 は `u32 as i64`、version 1 は `i32 as i64` で格納する
3. encode で `i64` から version 0 は `u32`、version 1 は `i32` に変換する
4. `uses_version_1` の判定も併せて修正する
5. 境界値の roundtrip テストを追加する
