#![expect(clippy::missing_safety_doc)]
pub mod basic_types;
pub mod boxes;
pub mod demux;
pub mod error;
pub mod fmp4_segment_demux;
pub mod fmp4_segment_mux;
pub mod mux;

/// ライブラリのバージョンを取得する
///
/// # 戻り値
///
/// バージョン文字列へのポインタ（NULL終端）
#[unsafe(no_mangle)]
pub extern "C" fn mp4_library_version() -> *const std::ffi::c_char {
    concat!(env!("SHIGUREDO_MP4_VERSION"), "\0").as_ptr().cast()
}
