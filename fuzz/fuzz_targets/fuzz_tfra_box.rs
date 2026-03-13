#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TfraBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((tfra, _)) = TfraBox::decode(data) {
        let _ = tfra.encode_to_vec();
    }
});
