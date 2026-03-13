#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Hev1Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((hev1, _)) = Hev1Box::decode(data) {
        let _ = hev1.encode_to_vec();
    }
});
