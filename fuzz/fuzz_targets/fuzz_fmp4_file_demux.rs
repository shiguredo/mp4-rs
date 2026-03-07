#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux_file_fmp4::Fmp4FileDemuxer;

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を完全な fMP4 ファイルとして処理してもパニックしないことを確認する
    let Ok(mut demuxer) = Fmp4FileDemuxer::new(data.to_vec()) else {
        return;
    };

    let _ = demuxer.tracks();

    // 全サンプルを読み出す
    loop {
        match demuxer.next_sample() {
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => break,
        }
    }
});
