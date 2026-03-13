#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::StcoBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((stco, _)) = StcoBox::decode(data) {
        let _ = stco.encode_to_vec();
    }
});
