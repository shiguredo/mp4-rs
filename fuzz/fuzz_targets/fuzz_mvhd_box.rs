#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MvhdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mvhd, _)) = MvhdBox::decode(data) {
        let _ = mvhd.encode_to_vec();
    }
});
