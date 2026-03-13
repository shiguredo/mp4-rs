#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TrunBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((trun, _)) = TrunBox::decode(data) {
        let _ = trun.encode_to_vec();
    }
});
