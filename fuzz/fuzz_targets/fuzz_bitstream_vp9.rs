#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::bitstream::vp9::parse_frame_header;

fuzz_target!(|data: &[u8]| {
    // 任意バイト列に対して parse_frame_header が panic せずに Result で戻ることを検証する
    let _ = parse_frame_header(data);
});
