#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TfhdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((tfhd, _)) = TfhdBox::decode(data) {
        let _ = tfhd.encode_to_vec();
    }
});
