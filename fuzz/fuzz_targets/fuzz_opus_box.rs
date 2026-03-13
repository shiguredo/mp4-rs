#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::OpusBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((opus, _)) = OpusBox::decode(data) {
        let _ = opus.encode_to_vec();
    }
});
