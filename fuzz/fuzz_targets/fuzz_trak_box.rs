#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TrakBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((trak, _)) = TrakBox::decode(data) {
        let _ = trak.encode_to_vec();
    }
});
