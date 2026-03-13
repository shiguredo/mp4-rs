#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::FreeBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((free, _)) = FreeBox::decode(data) {
        let _ = free.encode_to_vec();
    }
});
