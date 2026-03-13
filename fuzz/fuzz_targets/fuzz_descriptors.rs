#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{
    Decode, Encode,
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

fuzz_target!(|data: &[u8]| {
    if let Ok((es, _)) = EsDescriptor::decode(data) {
        let _ = es.encode_to_vec();
    }
    if let Ok((dec, _)) = DecoderConfigDescriptor::decode(data) {
        let _ = dec.encode_to_vec();
    }
    if let Ok((info, _)) = DecoderSpecificInfo::decode(data) {
        let _ = info.encode_to_vec();
    }
    if let Ok((sl, _)) = SlConfigDescriptor::decode(data) {
        let _ = sl.encode_to_vec();
    }
});
