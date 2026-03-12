#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux::{Input, Mp4FileKindDetector};

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列に対してファイル種別判定を行ってもパニックしないことを確認する
    let mut detector = Mp4FileKindDetector::new();

    while let Some(required) = detector.required_input() {
        let start = required.position as usize;
        let end = match required.size {
            Some(size) => start.saturating_add(size).min(data.len()),
            None => data.len(),
        };
        detector.handle_input(Input {
            position: required.position,
            data: data.get(start..end).unwrap_or(&[]),
        });

        // 判定結果が出たら終了する
        if let Ok(Some(_)) = detector.file_kind() {
            break;
        }
    }

    // 最終結果を取得する
    let _ = detector.file_kind();
});
