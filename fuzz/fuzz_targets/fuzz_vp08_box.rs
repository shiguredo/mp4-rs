#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Vp08Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((vp08, _)) = Vp08Box::decode(data) {
        let _ = vp08.encode_to_vec();
    }
});
