#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::StscBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((stsc, _)) = StscBox::decode(data) {
        let _ = stsc.encode_to_vec();
    }
});
