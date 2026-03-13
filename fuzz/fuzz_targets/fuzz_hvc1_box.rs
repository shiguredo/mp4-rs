#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Hvc1Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((hvc1, _)) = Hvc1Box::decode(data) {
        let _ = hvc1.encode_to_vec();
    }
});
