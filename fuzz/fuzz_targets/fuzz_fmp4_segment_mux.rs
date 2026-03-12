#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
use shiguredo_mp4::mux::{Fmp4SegmentMuxer, Sample};

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を MP4 ファイルとしてデマルチプレクスし、
    // 取得したサンプル情報を fMP4 セグメントとして再マルチプレクスしてもパニックしないことを確認する

    // まず MP4 ファイルとしてデマルチプレクスを試みる
    let mut demuxer = Mp4FileDemuxer::new();
    let input = Input {
        position: 0,
        data,
    };
    demuxer.handle_input(input);

    let tracks = match demuxer.tracks() {
        Ok(tracks) => tracks.to_vec(),
        Err(_) => return,
    };

    if tracks.is_empty() {
        return;
    }

    // サンプルを収集する
    let mut samples = Vec::new();
    let mut data_offset = 0u64;
    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => {
                let mux_sample = Sample {
                    track_kind: sample.track.kind,
                    timescale: sample.track.timescale,
                    sample_entry: sample.sample_entry.cloned(),
                    duration: sample.duration,
                    keyframe: sample.keyframe,
                    composition_time_offset: sample.composition_time_offset,
                    data_offset,
                    data_size: sample.data_size,
                };
                data_offset += sample.data_size as u64;
                samples.push(mux_sample);
            }
            Ok(None) => break,
            Err(_) => return,
        }
    }

    if samples.is_empty() {
        return;
    }

    // fMP4 セグメントとしてマルチプレクスする
    let mut muxer = match Fmp4SegmentMuxer::new() {
        Ok(m) => m,
        Err(_) => return,
    };

    let _ = muxer.create_media_segment_metadata(&samples);
    let _ = muxer.init_segment_bytes();
    let _ = muxer.mfra_bytes();
});
