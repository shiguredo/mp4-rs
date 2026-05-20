# mux_mp4_file.rs で映像解像度の u16→i16 暗黙キャストにより符号反転が発生する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`src/mux_mp4_file.rs:865-866` で `max_width as i16` / `max_height as i16` の暗黙キャストが行われており、
32768 以上の解像度が指定された場合に符号が反転して負の値になる。

## 根拠

```rust
// src/mux_mp4_file.rs:865-866
width: FixedPointNumber::new(max_width as i16, 0),
height: FixedPointNumber::new(max_height as i16, 0),
```

`max_width` / `max_height` は `u16` 型。32768 (0x8000) 以上の値を `as i16` でキャストすると
符号ビットが立ってしまい、tkhd ボックスの width/height に負の値が書き込まれる。

比較対象として `src/mux_fmp4_segment.rs:615-623` では正しく `i16::try_from` を使用している:

```rust
// src/mux_fmp4_segment.rs:615-623
let w = i16::try_from(v.width).map_err(|_| {
    MuxError::EncodeError(crate::Error::invalid_data(
        "video width exceeds i16::MAX",
    ))
})?;
let h = i16::try_from(v.height).map_err(|_| {
    MuxError::EncodeError(crate::Error::invalid_data(
        "video height exceeds i16::MAX",
    ))
})?;
```

## 再現手順

1. `Mp4FileMuxer` に映像サンプルエントリーの `width` が 32768 以上のトラックを追加する
2. `finalize()` で MP4 ファイルを生成する
3. 生成された MP4 の tkhd ボックスの width が負の値になる

## 修正方針

`as i16` を `i16::try_from()` に変更し、エラーを返すようにする。
`mux_fmp4_segment.rs` のパターンに寄せることで一貫性も確保できる。
