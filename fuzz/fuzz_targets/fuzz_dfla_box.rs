#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::DflaBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((dfla, _)) = DflaBox::decode(data) {
        let _ = dfla.encode_to_vec();
    }
});
