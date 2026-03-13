#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Av01Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((av01, _)) = Av01Box::decode(data) {
        let _ = av01.encode_to_vec();
    }
});
