#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::SdtpBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((sdtp, _)) = SdtpBox::decode(data) {
        let _ = sdtp.encode_to_vec();
    }
});
