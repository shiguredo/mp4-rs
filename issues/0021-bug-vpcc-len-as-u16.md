# boxes_sample_entry.rs の VpccBox::encode で codec_initialization_data.len() as u16 が黙って切り捨てられる

- Priority: Medium
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-vpcc-len-as-u16
- Polished: 2026-07-29

## 目的

`VpccBox::encode` で `codec_initialization_data.len()` を `as u16` でキャストしており、`len > 65535` で長さフィールドだけ切り捨てられ、実データは全量書き込まれるため長さと実体が不一致の壊れた vpcC を生成する問題を修正する。

## 優先度根拠

同ファイルの AvcC / HvcC は `u16::try_from(...).map_err(...)` で拒否しているのに Vpcc だけ `as u16` で黙って切り捨てており、一貫性がない。decode 側は `u16` で読むため roundtrip でデータ欠落する。コア API から到達可能。

## 現状

```rust
// src/boxes_sample_entry.rs の VpccBox::encode
offset += (self.codec_initialization_data.len() as u16).encode(&mut buf[offset..])?;
offset += self.codec_initialization_data.encode(&mut buf[offset..])?;
```

`Vec::len()` は `usize`。`as u16` は `u16::MAX`（65535）超で上位ビットを黙って切り捨てる。長さフィールドだけ壊れ、実データは全量書き込まれるため、decode 側は `u16` 分だけ読み、残りは未読になる。

対照的に同ファイルの `AvccBox::encode` / `HvccBox::encode` は `u16::try_from(...).map_err(...)` で拒否する（例: `AvccBox::encode` の `Too long SPS`）。

## 設計方針

`as u16` を `u16::try_from(...).map_err(...)?` に変更し、`len > 65535` でエラーを返す。AvcC / HvcC と同じ防御パターンに揃える。

## 完了条件

- `codec_initialization_data.len() > 65535` でエラーを返すこと
- `len <= 65535` では従来どおり正しくエンコードされること
- roundtrip でデータが一致すること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `src/boxes_sample_entry.rs` の `VpccBox::encode` 内にある `self.codec_initialization_data.len() as u16` を `u16::try_from(self.codec_initialization_data.len()).map_err(|_| Error::invalid_input("codec_initialization_data exceeds u16::MAX"))?` に置き換える
2. 境界値テストを追加する
