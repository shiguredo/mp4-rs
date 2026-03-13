#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::FlacBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((flac, _)) = FlacBox::decode(data) {
        let _ = flac.encode_to_vec();
    }
});
