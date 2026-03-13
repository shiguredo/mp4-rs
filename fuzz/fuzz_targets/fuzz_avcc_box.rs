#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::AvccBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((avcc, _)) = AvccBox::decode(data) {
        let _ = avcc.encode_to_vec();
    }
});
