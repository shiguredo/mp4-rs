# mp4-rs

[![shiguredo_mp4](https://img.shields.io/crates/v/shiguredo_mp4.svg)](https://crates.io/crates/shiguredo_mp4)
[![Documentation](https://docs.rs/shiguredo_mp4/badge.svg)](https://docs.rs/shiguredo_mp4)
[![GitHub Actions](https://github.com/shiguredo/mp4-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/shiguredo/mp4-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

Rust で実装された依存 0 かつ Sans I/O な MP4 ファイルの mux/demux ライブラリです。`no_std` 環境でも利用できます。

## 特徴

- Sans I/O
  - <https://sans-io.readthedocs.io/index.html>
- 依存ライブラリ 0
- `no_std` 対応
  - <https://docs.rust-embedded.org/book/intro/no-std.html>
- MP4 ファイルの mux/demux
- Fragmented MP4 (fMP4) の mux/demux
- 高レベル API の提供
- C API の提供
- WebAssembly API の提供
- Windows / macOS / Linux 対応

## 対応コーデック

- 音声
  - AAC (`mp4a`)
  - Opus (`Opus`)
  - FLAC (`fLaC`)
- 映像
  - VP8 (`vp08`)
  - VP9 (`vp09`)
  - AV1 (`av01`)
  - H.264 / AVC (`avc1`)
  - H.265 / HEVC (`hev1`, `hvc1`)

## 使い方

### MP4 ファイルのデマルチプレックス

`Mp4FileDemuxer` を使って MP4 ファイルからトラック情報やサンプルを取得できます。

```rust
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};

// デマルチプレクサーを生成する
let mut demuxer = Mp4FileDemuxer::new();

// 必要なデータを段階的に供給する (Sans I/O)
while let Some(required) = demuxer.required_input() {
    // required.position と required.size に基づいてデータを読み込む
    let data: &[u8] = read_data(required.position, required.size);
    demuxer.handle_input(Input {
        position: required.position,
        data,
    });
}

// トラック情報を取得する
let tracks = demuxer.tracks()?;

// サンプルを時系列順に取得する
while let Ok(Some(sample)) = demuxer.next_sample() {
    // sample.track, sample.timestamp, sample.data_offset, sample.data_size
}
```

### fMP4 セグメントの生成と読み戻し

`Fmp4SegmentMuxer` でメディアセグメントを生成し、`Fmp4SegmentDemuxer` で読み戻せます。

```rust
use shiguredo_mp4::mux::{Fmp4SegmentMuxer, SegmentSample};
use shiguredo_mp4::demux::Fmp4SegmentDemuxer;

// Muxer でメディアセグメントを生成する
let mut muxer = Fmp4SegmentMuxer::new()?;
let segment = muxer.create_media_segment(&samples)?;
let init_segment = muxer.init_segment_bytes()?;

// Demuxer で読み戻す
let mut demuxer = Fmp4SegmentDemuxer::new();
demuxer.handle_init_segment(&init_segment)?;
let demuxed = demuxer.handle_media_segment(&segment)?;
```

## サンプル

### demux

MP4 ファイルのトラック情報とサンプル情報を表示します。

```bash
cargo run --example demux -- <mp4_file>
```

### fmp4

fMP4 の mux/demux を行うサンプルです。引数なしで実行できます。

```bash
cargo run --example fmp4
```

### WebAssembly サンプル

WebAssembly を使ったサンプルを GitHub Pages に用意しています。

- [MP4 Dump](https://shiguredo.github.io/mp4-rs/examples/dump/)
- [MP4 Transcode](https://shiguredo.github.io/mp4-rs/examples/transcode/)

## 規格書

- ISO/IEC 14496-1
- ISO/IEC 14496-12
- ISO/IEC 14496-14
- ISO/IEC 14496-15
- [VP Codec ISO Media File Format Binding](https://www.webmproject.org/vp9/mp4/)
- [AV1 Codec ISO Media File Format Binding](https://aomediacodec.github.io/av1-isobmff/)
- [Encapsulation of Opus in ISO Base Media File Format](https://gitlab.xiph.org/xiph/opus/-/blob/main/doc/opus_in_isobmff.html)
- [Encapsulation of FLAC in ISO Base Media File Format](https://github.com/xiph/flac/blob/master/doc/isoflac.txt)

## ロードマップ

- AV2 のサポート
- H.266 (VVC) のサポート

## ライセンス

Apache License 2.0

```text
Copyright 2024-2026, Takeru Ohta (Original Author)
Copyright 2024-2026, Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
