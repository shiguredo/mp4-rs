#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::EdtsBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((edts, _)) = EdtsBox::decode(data) {
        let _ = edts.encode_to_vec();
    }
});
