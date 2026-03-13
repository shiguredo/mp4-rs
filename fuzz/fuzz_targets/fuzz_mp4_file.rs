#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{Decode, Encode, Mp4File, boxes::RootBox};

fuzz_target!(|data: &[u8]| {
    if let Ok((mp4_file, _)) = Mp4File::<RootBox>::decode(data) {
        let _ = mp4_file.encode_to_vec();
    }
});
