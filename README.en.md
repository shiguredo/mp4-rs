# mp4-rs

[![shiguredo_mp4](https://img.shields.io/crates/v/shiguredo_mp4.svg)](https://crates.io/crates/shiguredo_mp4)
[![Documentation](https://docs.rs/shiguredo_mp4/badge.svg)](https://docs.rs/shiguredo_mp4)
[![GitHub Actions](https://github.com/shiguredo/mp4-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/shiguredo/mp4-rs/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

## About Shiguredo's Open Source Software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## Overview

A zero-dependency, Sans I/O MP4 file mux/demux library implemented in Rust. It can also be used in `no_std` environments.

## Features

- Sans I/O
  - <https://sans-io.readthedocs.io/index.html>
- Zero dependencies
- `no_std` compatible
  - <https://docs.rust-embedded.org/book/intro/no-std.html>
- MP4 file mux/demux
- Fragmented MP4 (fMP4) mux/demux
- Provides high-level APIs
- Provides C APIs
- Provides WebAssembly APIs
- Supports Windows / macOS / Linux

## Supported Codecs

- Audio
  - AAC (`mp4a`)
  - Opus (`Opus`)
  - FLAC (`fLaC`)
- Video
  - VP8 (`vp08`)
  - VP9 (`vp09`)
  - AV1 (`av01`)
  - H.264 / AVC (`avc1`)
  - H.265 / HEVC (`hev1`, `hvc1`)

## Usage

### Demultiplexing an MP4 File

Use `Mp4FileDemuxer` to retrieve track information and samples from an MP4 file.

```rust
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};

// Create a demuxer
let mut demuxer = Mp4FileDemuxer::new();

// Supply data incrementally (Sans I/O)
while let Some(required) = demuxer.required_input() {
    // Read data based on required.position and required.size
    let data: &[u8] = read_data(required.position, required.size);
    demuxer.handle_input(Input {
        position: required.position,
        data,
    });
}

// Retrieve track information
let tracks = demuxer.tracks()?;

// Retrieve samples in chronological order
while let Ok(Some(sample)) = demuxer.next_sample() {
    // sample.track, sample.timestamp, sample.data_offset, sample.data_size
}
```

### Creating and Reading Back fMP4 Segments

Use `Fmp4SegmentMuxer` to create media segments and `Fmp4SegmentDemuxer` to read them back.

```rust
use shiguredo_mp4::mux::{Fmp4SegmentMuxer, SegmentSample};
use shiguredo_mp4::demux::Fmp4SegmentDemuxer;

// Create a media segment with the muxer
let mut muxer = Fmp4SegmentMuxer::new()?;
let segment = muxer.create_media_segment(&samples)?;
let init_segment = muxer.init_segment_bytes()?;

// Read it back with the demuxer
let mut demuxer = Fmp4SegmentDemuxer::new();
demuxer.handle_init_segment(&init_segment)?;
let demuxed = demuxer.handle_media_segment(&segment)?;
```

## Examples

### demux

Displays track information and sample information from an MP4 file.

```bash
cargo run --example demux -- <mp4_file>
```

### fmp4

An example that performs fMP4 mux/demux. Can be run without arguments.

```bash
cargo run --example fmp4
```

### WebAssembly Examples

Sample implementations using WebAssembly are available on GitHub Pages.

- [MP4 Dump](https://shiguredo.github.io/mp4-rs/examples/dump/)
- [MP4 Transcode](https://shiguredo.github.io/mp4-rs/examples/transcode/)

## Specifications

- ISO/IEC 14496-1
- ISO/IEC 14496-12
- ISO/IEC 14496-14
- ISO/IEC 14496-15
- [VP Codec ISO Media File Format Binding](https://www.webmproject.org/vp9/mp4/)
- [AV1 Codec ISO Media File Format Binding](https://aomediacodec.github.io/av1-isobmff/)
- [Encapsulation of Opus in ISO Base Media File Format](https://gitlab.xiph.org/xiph/opus/-/blob/main/doc/opus_in_isobmff.html)
- [Encapsulation of FLAC in ISO Base Media File Format](https://github.com/xiph/flac/blob/master/doc/isoflac.txt)

## Roadmap

- Support for AV2
- Support for H.266 (VVC)

## License

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
