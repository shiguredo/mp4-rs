#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux::{DemuxError, Input, Mp4FileDemuxer};

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を MP4 ファイルとして incremental に処理してもパニックしないことを確認する
    let mut demuxer = Mp4FileDemuxer::new();

    // 必要なデータを段階的に供給する
    while let Some(required) = demuxer.required_input() {
        let start = required.position as usize;
        let Some(required_size) = required.size else {
            // EOF まで必要な場合はファイル末尾までのデータを供給する
            demuxer.handle_input(Input {
                position: required.position,
                data: data.get(start..).unwrap_or(&[]),
            });
            break;
        };
        let end = start.saturating_add(required_size).min(data.len());
        demuxer.handle_input(Input {
            position: required.position,
            data: data.get(start..end).unwrap_or(&[]),
        });
    }

    // トラック情報を取得する
    if demuxer.tracks().is_err() {
        return;
    }

    // 全サンプルを読み出す
    loop {
        match demuxer.next_sample() {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(DemuxError::InputRequired(_)) => break,
            Err(_) => break,
        }
    }
});
