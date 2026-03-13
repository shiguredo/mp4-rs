#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, FullBoxFlags, FullBoxHeader, Utf8String, boxes::Brand,
};

fuzz_target!(|data: &[u8]| {
    if let Ok((header, _)) = FullBoxHeader::decode(data) {
        let _ = header.encode_to_vec();
    }
    if let Ok((flags, _)) = FullBoxFlags::decode(data) {
        let _ = flags.encode_to_vec();
    }
    if let Ok((utf8, _)) = Utf8String::decode(data) {
        let _ = utf8.encode_to_vec();
    }
    if let Ok((fixed, _)) = FixedPointNumber::<u16, u16>::decode(data) {
        let _ = fixed.encode_to_vec();
    }
    if let Ok((brand, _)) = Brand::decode(data) {
        let _ = brand.encode_to_vec();
    }
});
