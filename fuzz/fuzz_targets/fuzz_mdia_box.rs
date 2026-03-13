#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MdiaBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mdia, _)) = MdiaBox::decode(data) {
        let _ = mdia.encode_to_vec();
    }
});
