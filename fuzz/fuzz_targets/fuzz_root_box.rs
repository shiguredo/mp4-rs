#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::RootBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((root_box, _)) = RootBox::decode(data) {
        let _ = root_box.encode_to_vec();
    }
});
