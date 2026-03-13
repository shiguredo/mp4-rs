#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Av1cBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((av1c, _)) = Av1cBox::decode(data) {
        let _ = av1c.encode_to_vec();
    }
});
