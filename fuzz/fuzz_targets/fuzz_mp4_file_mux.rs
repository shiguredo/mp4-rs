#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
use shiguredo_mp4::mux::{Mp4FileMuxer, Mp4FileMuxerOptions, Sample};

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を MP4 ファイルとしてデマルチプレクスし、
    // 取得したサンプル情報を Mp4FileMuxer で再マルチプレクスしてもパニックしないことを確認する

    if data.is_empty() {
        return;
    }

    // 先頭 1 バイトの最上位ビットで faststart 経路の分岐を決定する
    let use_faststart = data[0] & 0x80 != 0;

    // MP4 ファイルとしてデマルチプレクスを試みる
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

    // Mp4FileMuxer を生成する
    let mut muxer = if use_faststart {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        match Mp4FileMuxer::with_options(options) {
            Ok(m) => m,
            Err(_) => return,
        }
    } else {
        match Mp4FileMuxer::new() {
            Ok(m) => m,
            Err(_) => return,
        }
    };

    // data_offset を initial_boxes_bytes の長さを基準に再計算して mux に投入する
    let base_offset = muxer.initial_boxes_bytes().len() as u64;
    for sample in &mut samples {
        sample.data_offset += base_offset;
    }

    for sample in &samples {
        if muxer.append_sample(sample).is_err() {
            return;
        }
    }

    // finalize してもパニックしないことを確認する
    let finalized = match muxer.finalize() {
        Ok(f) => f,
        Err(_) => return,
    };

    // FinalizedBoxes のメソッドを呼び出してパニックしないことを確認する
    let _ = finalized.is_faststart_enabled();
    let _ = finalized.moov_box_size();
    let _ = finalized.moov_box();
    for _ in finalized.offset_and_bytes_pairs() {}
});
