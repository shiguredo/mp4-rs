#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::UrlBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((url, _)) = UrlBox::decode(data) {
        let _ = url.encode_to_vec();
    }
});
