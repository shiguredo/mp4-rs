#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::EsdsBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((esds, _)) = EsdsBox::decode(data) {
        let _ = esds.encode_to_vec();
    }
});
