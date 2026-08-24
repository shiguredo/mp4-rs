#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::bitstream::h264::{parse_annexb_nal_units, parse_sps};

fuzz_target!(|data: &[u8]| {
    // 任意バイト列に対して Annex B 列挙と SPS 解析が panic せず Result で戻ることを検証する
    let _ = parse_annexb_nal_units(data);
    let _ = parse_sps(data);

    // length-prefixed 列挙は長さ幅 1 / 2 / 4 の全てに対して検証する
    for length_size in [1u8, 2, 4] {
        let _ = shiguredo_mp4::bitstream::h264::parse_length_prefixed_nal_units(data, length_size);
    }
});
