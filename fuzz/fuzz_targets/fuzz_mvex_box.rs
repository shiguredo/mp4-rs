#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::MvexBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mvex, _)) = MvexBox::decode(data) {
        let _ = mvex.encode_to_vec();
    }
});
