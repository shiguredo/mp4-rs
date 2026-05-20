# mux_mp4_file.rs で映像解像度の u16→i16 キャストにより符号反転が発生する

Created: 2026-05-20
Model: opencode mimo-v2.5-pro

## 概要

`src/mux_mp4_file.rs:865-866` (`build_video_trak_box` 内) で
`max_width as i16` / `max_height as i16` のキャストが行われており、
32768 以上の解像度が指定された場合に符号が反転して負の値になる。

## 根拠

tkhd ボックスの width/height は ISO/IEC 14496-12 で 16.16 fixed-point (符号付き i16 の整数部) として定義されている。
`VisualSampleEntryFields.width/height` (`src/boxes_sample_entry.rs:161-162`) は `u16` 型だが、
`TkhdBox.width/height` (`src/boxes_moov_tree.rs:349-350`) は `FixedPointNumber<i16, u16>` 型。

```rust
// src/mux_mp4_file.rs:865-866 (build_video_trak_box 内)
width: FixedPointNumber::new(max_width as i16, 0),
height: FixedPointNumber::new(max_height as i16, 0),
```

比較対象の `src/mux_fmp4_segment.rs:615-623` では正しく `i16::try_from` を使用している:

```rust
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

## エッジケース

- `0`: `i16::try_from(0u16)` は成功 (正常)
- `32767` (i16::MAX): `i16::try_from(32767u16)` は成功 (正常)
- `32768`: `i16::try_from(32768u16)` は Err を返す (エラー)
- `65535` (u16::MAX): `i16::try_from(65535u16)` は Err を返す (エラー)

## 再現手順

```rust
// src/mux_mp4_file.rs のテストモジュール内の create_avc1_sample_entry() を使用
let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
let initial_size = muxer.initial_boxes_bytes().len() as u64;

// width=32768 の映像サンプルエントリーを作成
let mut entry = create_avc1_sample_entry();
if let SampleEntry::Avc1(ref mut avc1) = entry {
    avc1.visual.width = 32768; // i16::MAX を超える
}

let sample = Sample {
    track_kind: TrackKind::Video,
    sample_entry: Some(entry),
    keyframe: true,
    timescale: NonZeroU32::MIN.saturating_add(30 - 1),
    duration: 1,
    composition_time_offset: None,
    data_offset: initial_size,
    data_size: 1024,
};
muxer.append_sample(&sample).expect("failed to append sample");
// 現状: finalize() が Ok を返し、tkhd の width が負の値になる
// 期待: append_sample() または finalize() でエラーが返る
```

## 修正方針

`src/mux_mp4_file.rs:865-866` の `as i16` を `i16::try_from()` に変更し、エラーを返す。
エラーメッセージは `mux_fmp4_segment.rs:615-623` のものをそのまま使用する。

```rust
// 修正前
width: FixedPointNumber::new(max_width as i16, 0),
height: FixedPointNumber::new(max_height as i16, 0),

// 修正後
width: FixedPointNumber::new(i16::try_from(max_width).map_err(|_| {
    MuxError::EncodeError(crate::Error::invalid_data(
        "video width exceeds i16::MAX",
    ))
})?, 0),
height: FixedPointNumber::new(i16::try_from(max_height).map_err(|_| {
    MuxError::EncodeError(crate::Error::invalid_data(
        "video height exceeds i16::MAX",
    ))
})?, 0),
```

## テスト戦略

- 単体テスト: `width` が `32767` で成功、`32768` でエラーを返すことの検証 (height も同様)
- `src/mux_mp4_file.rs` のテストモジュールに追加する

## 後方互換

修正後、32768 以上の解像度を持つ映像トラックを `Mp4FileMuxer` に渡した場合に
`MuxError` が返るようになる (動作変更)。
現状は符号反転して壊れた tkhd が生成されていたため、修正によりエラーで明示的に拒否される。

## CHANGES.md

`[FIX]` で記載する。
