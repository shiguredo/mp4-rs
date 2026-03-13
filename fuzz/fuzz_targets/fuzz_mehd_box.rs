#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MehdBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mehd, _)) = MehdBox::decode(data) {
        let _ = mehd.encode_to_vec();
    }
});
