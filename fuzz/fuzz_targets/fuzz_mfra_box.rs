#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MfraBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mfra, _)) = MfraBox::decode(data) {
        let _ = mfra.encode_to_vec();
    }
});
