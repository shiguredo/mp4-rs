#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MoovBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((moov, _)) = MoovBox::decode(data) {
        let _ = moov.encode_to_vec();
    }
});
