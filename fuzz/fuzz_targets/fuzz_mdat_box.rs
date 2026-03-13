#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MdatBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mdat, _)) = MdatBox::decode(data) {
        let _ = mdat.encode_to_vec();
    }
});
