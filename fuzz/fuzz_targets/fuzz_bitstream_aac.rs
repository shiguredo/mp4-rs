#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::bitstream::aac::{parse_adts_frame, parse_audio_specific_config};

fuzz_target!(|data: &[u8]| {
    // 任意バイト列に対して ASC / ADTS パーサーが panic せずに Result で戻ることを検証する
    let _ = parse_audio_specific_config(data);
    let _ = parse_adts_frame(data);
});
