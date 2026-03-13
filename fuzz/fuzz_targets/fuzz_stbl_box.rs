#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::StblBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((stbl, _)) = StblBox::decode(data) {
        let _ = stbl.encode_to_vec();
    }
});
