#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::CslgBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((cslg, _)) = CslgBox::decode(data) {
        let _ = cslg.encode_to_vec();
    }
});
