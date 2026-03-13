#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MfroBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mfro, _)) = MfroBox::decode(data) {
        let _ = mfro.encode_to_vec();
    }
});
