#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MfhdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mfhd, _)) = MfhdBox::decode(data) {
        let _ = mfhd.encode_to_vec();
    }
});
