//! shiguredo_mp4 の wasm バインディング
//!
//! c-api の機能に加えて、wasm 固有の機能を提供する
#![warn(missing_docs)]
#![expect(clippy::missing_safety_doc)]

pub mod boxes;
pub mod boxes_av01;
pub mod boxes_avc1;
pub mod boxes_flac;
pub mod boxes_hev1;
pub mod boxes_hvc1;
pub mod boxes_mp4a;
pub mod boxes_opus;
pub mod boxes_vp08;
pub mod boxes_vp09;
pub mod demux;
pub mod fmp4_demux;
pub mod fmp4_mux;
pub mod mux;

use std::alloc::Layout;

/// メモリを確保する
///
/// この関数で確保したメモリは `mp4_free()` で解放すること
///
/// # 引数
///
/// - `size`: 確保するバイト数
///
/// # 戻り値
///
/// 確保したメモリの先頭アドレス
#[unsafe(no_mangle)]
pub extern "C" fn mp4_alloc(size: u32) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    let layout = Layout::from_size_align(size as usize, 1)
        .expect("layout creation with alignment 1 should never fail");
    unsafe { std::alloc::alloc(layout) }
}

/// `mp4_alloc()` で確保したメモリを解放する
///
/// # 引数
/// - `ptr`: 解放するメモリの先頭アドレス
/// - `size`: 解放するバイト数
///
/// 以下の場合には関数呼び出しは単に無視される:
/// - `ptr` が NULL
/// - `size` が 0
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_free(ptr: *mut u8, size: u32) {
    if ptr.is_null() || size == 0 {
        return;
    }
    let layout = Layout::from_size_align(size as usize, 1)
        .expect("layout creation with alignment 1 should never fail");
    unsafe { std::alloc::dealloc(ptr, layout) };
}

/// `Vec<u8>` のポインタを取得する
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_vec_ptr(v: *const Vec<u8>) -> *const u8 {
    if v.is_null() {
        return std::ptr::null();
    }
    unsafe { (*v).as_ptr() }
}

/// `Vec<u8>` の長さを取得する
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_vec_len(v: *const Vec<u8>) -> u32 {
    if v.is_null() {
        return 0;
    }
    unsafe {
        u32::try_from((*v).len()).expect("vec length should not exceed u32::MAX in wasm32 target")
    }
}

/// `Vec<u8>` を解放する
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_vec_free(v: *mut Vec<u8>) {
    if !v.is_null() {
        let _ = unsafe { Box::from_raw(v) };
    }
}
