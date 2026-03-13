#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::SidxBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((sidx, _)) = SidxBox::decode(data) {
        let _ = sidx.encode_to_vec();
    }
});
