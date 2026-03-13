#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, boxes::FtypBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((ftyp, _)) = FtypBox::decode(data) {
        let _ = ftyp.encode_to_vec();
    }
});
