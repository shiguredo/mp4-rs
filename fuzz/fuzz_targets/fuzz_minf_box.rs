#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MinfBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((minf, _)) = MinfBox::decode(data) {
        let _ = minf.encode_to_vec();
    }
});
