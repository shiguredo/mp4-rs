#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Avc1Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((avc1, _)) = Avc1Box::decode(data) {
        let _ = avc1.encode_to_vec();
    }
});
