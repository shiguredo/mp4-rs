#![no_main]

use libfuzzer_sys::fuzz_target;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, FullBoxFlags, FullBoxHeader, SampleFlags, Utf8String,
    boxes::Brand,
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
    if let Ok((flags, _)) = SampleFlags::decode(data) {
        let _ = flags.encode_to_vec();
    }

    // FullBoxFlags::from_flags と FullBoxFlags::is_set の任意入力に対する
    // パニック安全性を検証する。
    //
    // 上記の既存 fuzz 本体は Decode / Encode を通すだけで `from_flags` / `is_set` を
    // 呼ばないため、この 2 関数の任意ビット位置に対するパニック安全性はそのままでは
    // 担保されない。本追加パスでは先頭 8 バイトのうち前半 4 バイトを `u32` の flags 値、
    // 後半 4 バイトをビット位置 `i` (usize) として、両関数を直接叩く。
    if data.len() >= 8 {
        let flags = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let i = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let fbf = FullBoxFlags::new(flags);
        let _ = fbf.is_set(i);
        let _ = FullBoxFlags::from_flags([(i, true)]).get();
    }
});
