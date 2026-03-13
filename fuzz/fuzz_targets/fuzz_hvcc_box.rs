#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::HvccBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((hvcc, _)) = HvccBox::decode(data) {
        let _ = hvcc.encode_to_vec();
    }
});
