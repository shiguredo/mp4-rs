#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MoofBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((moof, _)) = MoofBox::decode(data) {
        let _ = moof.encode_to_vec();
    }
});
