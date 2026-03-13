#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux::{DemuxError, Fmp4FileDemuxer, Input};

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を完全な fMP4 ファイルとして incremental に処理してもパニックしないことを確認する
    let mut demuxer = Fmp4FileDemuxer::new();
    while let Some(required) = demuxer.required_input() {
        let start = required.position as usize;
        let Some(required_size) = required.size else {
            break;
        };
        let end = start.saturating_add(required_size).min(data.len());
        demuxer.handle_input(Input {
            position: required.position,
            data: data.get(start..end).unwrap_or(&[]),
        });
    }

    let _ = demuxer.tracks();

    // 全サンプルを読み出す
    loop {
        match demuxer.next_sample() {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(DemuxError::InputRequired(_)) => {
                while let Some(required) = demuxer.required_input() {
                    let start = required.position as usize;
                    let Some(required_size) = required.size else {
                        break;
                    };
                    let end = start.saturating_add(required_size).min(data.len());
                    demuxer.handle_input(Input {
                        position: required.position,
                        data: data.get(start..end).unwrap_or(&[]),
                    });
                }
            }
            Err(_) => break,
        }
    }
});
