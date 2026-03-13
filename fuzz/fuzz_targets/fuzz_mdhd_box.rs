#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MdhdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mdhd, _)) = MdhdBox::decode(data) {
        let _ = mdhd.encode_to_vec();
    }
});
