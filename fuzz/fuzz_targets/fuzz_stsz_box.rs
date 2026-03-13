#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::StszBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((stsz, _)) = StszBox::decode(data) {
        let _ = stsz.encode_to_vec();
    }
});
