#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TfdtBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((tfdt, _)) = TfdtBox::decode(data) {
        let _ = tfdt.encode_to_vec();
    }
});
