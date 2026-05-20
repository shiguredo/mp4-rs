# mux_mp4_file.rs で sample.data_size の usize→u32 暗黙トランケーションが発生する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`src/mux_mp4_file.rs:548` で `sample.data_size as u32` の暗黙キャストが行われており、
サンプルデータサイズが 4GB (u32::MAX) を超える場合にトランケーションが発生し、
壊れた MP4 ファイルが生成される。

## 根拠

```rust
// src/mux_mp4_file.rs:548
size: sample.data_size as u32,
```

`sample.data_size` は `usize` 型。4GB 超の値が渡された場合、上位ビットが切り捨てられ、
破損したサイズ値が MP4 に書き込まれる。

比較対象として `src/mux_fmp4_segment.rs:744` では正しく `u32::try_from` を使用している:

```rust
// src/mux_fmp4_segment.rs:744
size: Some(u32::try_from(sample.data_size).map_err(|_| {
    MuxError::EncodeError(Error::invalid_data(
        "sample data size exceeds u32::MAX",
    ))
})?),
```

## 再現手順

1. `Mp4FileMuxer` に 4GB を超えるサンプルデータを追加する
2. `finalize()` で MP4 ファイルを生成する
3. 生成された MP4 の stsz ボックスにトランケーションされたサイズ値が書き込まれる

## 修正方針

`as u32` を `u32::try_from()` に変更し、エラーを返すようにする。
`mux_fmp4_segment.rs` のパターンに寄せることで一貫性も確保できる。
