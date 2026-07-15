# boxes_fmp4.rs の encode_variable_uint がバッファ長を検査せず panic する

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-encode-variable-uint-buffer-check
- Polished: YYYY-MM-DD

## 目的

`encode_variable_uint` が `byte_count` 1〜3 のときバッファ長を検査せず `buf[0]` 等に直接書き込み、短いバッファで panic する問題を修正する。

## 優先度根拠

`Encode` トレイトの契約ではバッファ不足時に `InsufficientBuffer` エラーを返すべきであり、panic は契約違反。`TfraBox::encode` 経由で到達する。対の `decode_variable_uint` は長さ検査あり、`byte_count == 4` の `u32::encode` も `Error::check_buffer_size` を通るのに、ここだけ抜けている。

## 現状

```rust
// src/boxes_fmp4.rs:1340-1355
fn encode_variable_uint(value: u32, byte_count: u8, buf: &mut [u8]) -> Result<usize> {
    match byte_count {
        1 => {
            buf[0] = value as u8;
            Ok(1)
        }
        2 => {
            buf[0] = (value >> 8) as u8;
            buf[1] = value as u8;
            Ok(2)
        }
        3 => {
            buf[0] = (value >> 16) as u8;
            buf[1] = (value >> 8) as u8;
            buf[2] = value as u8;
            Ok(3)
        }
        4 => value.encode(buf),
        _ => Err(crate::Error::invalid_data(
            "Invalid byte count for variable uint",
        )),
    }
}
```

`byte_count` 1〜3 で `buf[0]` 等に直接代入し、`buf.len()` 検査がない。`buf` が不足するとインデックス範囲外で panic する。`byte_count == 4` のみ `u32::encode` 経由で `Error::check_buffer_size` が走る。

## 設計方針

`byte_count` 1〜3 の各ケースで `Error::check_buffer_size(byte_count as usize, buf)?` を先に呼び、バッファ不足時は `InsufficientBuffer` を返す。`decode_variable_uint`（`src/boxes_fmp4.rs:1366-1368`）と同じ検査パターンに揃える。

## 完了条件

- 短いバッファで panic せず `InsufficientBuffer` エラーを返すこと
- 十分なバッファでは従来どおり正しくエンコードされること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `encode_variable_uint` の `byte_count` 1〜3 の各ケースで `Error::check_buffer_size(byte_count as usize, buf)?` を先に呼ぶ
2. 短いバッファで `InsufficientBuffer` が返るテストを追加する
