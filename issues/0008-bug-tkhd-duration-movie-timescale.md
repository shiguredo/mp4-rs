# mux_mp4_file.rs の tkhd.duration が movie timescale 単位ではなく media timescale 単位で書かれている

- Priority: High
- Created: 2026-07-15
- Completed: YYYY-MM-DD
- Model: opencode-go glm-5.2
- Branch: feature/fix-tkhd-duration-movie-timescale
- Polished: YYYY-MM-DD

## 目的

`Mp4FileMuxer` が生成する MP4 の `tkhd.duration` が ISO/IEC 14496-12 の仕様に違反しており、音声と映像で timescale が異なる場合に壊れた MP4 を生成する問題を修正する。

## 優先度根拠

A/V で timescale が異なる（例: 音声 48000 / 映像 90000）典型的な MP4 で、短い方のトラックの `tkhd.duration` が映画時間軸上で誤った尺になる。プレイヤー・ツールが `tkhd.duration` を参照すると再生時間・シーク範囲が崩れる。実用的な A/V 同時 mux で顕在化するため High。

## 現状

`build_audio_trak_box` / `build_video_trak_box` は `tkhd.duration` に各トラックの media timescale 単位の合計をそのまま書き込んでいる。

```rust
// src/mux_mp4_file.rs:811-826 (build_audio_trak_box)
let total_duration = self
    .audio_chunks
    .iter()
    .flat_map(|c| c.samples.iter().map(|s| s.duration as u64))
    .sum::<u64>();
// ...
    duration: total_duration,
```

一方 `calculate_total_duration` は壁時計で長い方のトラックの media timescale と duration を `mvhd` に採用する。

```rust
// src/mux_mp4_file.rs:1057-1078
fn calculate_total_duration(&self) -> (NonZeroU32, u64) {
    // ...
    if normalized_audio_duration < normalized_video_duration {
        (self.video_track_timescale, video_duration)
    } else {
        (self.audio_track_timescale, audio_duration)
    }
}
```

ISO/IEC 14496-12 では `tkhd.duration` は Movie Header (`mvhd`) の timescale 単位で表さなければならない。`mdhd.duration` は media timescale 単位であり、現状の `mdhd` 側は正しい。

例: 音声 timescale=48000 duration=480000（10 秒）、映像 timescale=90000 duration=900000（10 秠）のとき、`mvhd` は音声側を採用し timescale=48000 になる。映像の `tkhd.duration=900000` を movie timescale=48000 で解釈すると約 18.75 秒になる（正しくは 480000）。

単一トラック、または音声・映像の timescale が同一の場合は偶然一致するため、既存テストでは検出されていない。

## 設計方針

`calculate_total_duration` が決めた movie timescale に合わせて、各トラックの `tkhd.duration` を `media_duration * movie_timescale / media_timescale` で換算する。乗除は `checked_*` 演算を使い、overflow やゼロ除算を防ぐ。端数の丸め方は仕様上の制約がないため切り捨てとする。

## 完了条件

- 音声と映像で timescale が異なる MP4 を生成したとき、両トラックの `tkhd.duration` が movie timescale 単位で正しい値になること
- 既存の単一トラック・同一 timescale のケースで従来どおり正しい MP4 が生成されること
- `cargo test` / `cargo clippy` が通ること

## 解決方法

1. `build_audio_trak_box` / `build_video_trak_box` に movie timescale を渡す
2. `tkhd.duration` を `total_duration * movie_timescale / media_timescale` で計算する（`checked_mul` / `checked_div` を使用）
3. 異 timescale A/V の回帰テストを追加する（`prop_mux_demux` または単体テスト）
