#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::StsdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((stsd, _)) = StsdBox::decode(data) {
        let _ = stsd.encode_to_vec();
    }
});
