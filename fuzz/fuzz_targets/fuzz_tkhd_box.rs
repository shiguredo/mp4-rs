#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TkhdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((tkhd, _)) = TkhdBox::decode(data) {
        let _ = tkhd.encode_to_vec();
    }
});
