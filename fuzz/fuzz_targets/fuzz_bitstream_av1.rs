#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::bitstream::av1::{
    Av1ObuParseContext, Av1SampleEntryConfig, build_av01_box, decode_leb128,
    parse_frame_header_prefix, parse_obus, parse_sequence_header,
};

fuzz_target!(|data: &[u8]| {
    // 任意バイト列に対して公開パーサーが panic せず Result で戻ることを検証する
    let _ = decode_leb128(data);
    let _ = parse_obus(data, Av1ObuParseContext::ConfigObus);
    let _ = parse_obus(data, Av1ObuParseContext::Sample);
    if let Ok(seq) = parse_sequence_header(data) {
        let _ = parse_frame_header_prefix(data, &seq);
        let _ = build_av01_box(
            &seq,
            data,
            &Av1SampleEntryConfig {
                initial_presentation_delay_minus_one: None,
            },
        );
    }
});
