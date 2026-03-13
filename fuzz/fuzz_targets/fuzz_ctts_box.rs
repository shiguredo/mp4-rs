#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::CttsBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((ctts, _)) = CttsBox::decode(data) {
        let _ = ctts.encode_to_vec();
    }
});
