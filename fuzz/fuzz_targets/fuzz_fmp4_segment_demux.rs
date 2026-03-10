#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::demux_fmp4_segment::Fmp4SegmentDemuxer;

fuzz_target!(|data: &[u8]| {
    // 任意のバイト列を init segment + media segment として処理してもパニックしないことを確認する
    let mut demuxer = Fmp4SegmentDemuxer::new();

    // data の先半分を init segment、後半分を media segment として扱う
    if data.len() < 16 {
        let _ = demuxer.handle_init_segment(data);
        return;
    }

    let split = data.len() / 2;
    let init_data = &data[..split];
    let media_data = &data[split..];

    if demuxer.handle_init_segment(init_data).is_ok() {
        let _ = demuxer.tracks();
        let _ = demuxer.handle_media_segment(media_data);
    }
});
