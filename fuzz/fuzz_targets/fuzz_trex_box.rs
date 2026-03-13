#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::TrexBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((trex, _)) = TrexBox::decode(data) {
        let _ = trex.encode_to_vec();
    }
});
