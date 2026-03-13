#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::DinfBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((dinf, _)) = DinfBox::decode(data) {
        let _ = dinf.encode_to_vec();
    }
});
