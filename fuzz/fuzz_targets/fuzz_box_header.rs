#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{BoxHeader, Decode, Encode};

fuzz_target!(|data: &[u8]| {
    if let Ok((header, _)) = BoxHeader::decode(data) {
        let _ = header.encode_to_vec();
    }
});
