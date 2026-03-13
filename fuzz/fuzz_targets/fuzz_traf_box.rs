#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TrafBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((traf, _)) = TrafBox::decode(data) {
        let _ = traf.encode_to_vec();
    }
});
