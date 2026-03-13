#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::Vp09Box};

fuzz_target!(|data: &[u8]| {
    if let Ok((vp09, _)) = Vp09Box::decode(data) {
        let _ = vp09.encode_to_vec();
    }
});
