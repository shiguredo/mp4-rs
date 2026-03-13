#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::ElstBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((elst, _)) = ElstBox::decode(data) {
        let _ = elst.encode_to_vec();
    }
});
