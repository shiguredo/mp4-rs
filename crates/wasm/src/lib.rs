//! shiguredo_mp4 の WebAssembly バインディング
//!
//! `shiguredo_mp4` の mux / demux 機能を JavaScript / TypeScript から利用できる形で公開する。
//! 生ポインタの受け渡しは行わず、文字列・バイト列・ハンドル型のみを境界に出す。
//!
//! # データ表現
//!
//! トラック情報・サンプル情報・サンプルエントリーは JSON 文字列として受け渡す。
//! バイナリデータ（SPS / PPS / NALU 等）は数値配列として表現する。
//! JSON のスキーマは `crates/wasm/README.md` に記載している。
#![warn(missing_docs)]

use wasm_bindgen::prelude::*;

mod boxes;
mod boxes_av01;
mod boxes_avc1;
mod boxes_flac;
mod boxes_hev1;
mod boxes_hevc;
mod boxes_hvc1;
mod boxes_mp4a;
mod boxes_opus;
mod boxes_stpp;
mod boxes_tx3g;
mod boxes_vp08;
mod boxes_vp09;
mod boxes_wvtt;
mod convert;
mod demux;
mod fmp4_segment_demux;
mod fmp4_segment_mux;
mod mp4_file_kind_detector;
mod mux;
mod mux_sample;
mod track;

/// ライブラリのバージョンを返す
///
/// 返り値はルートの `Cargo.toml` の `version` と同じ値になる。
/// npm パッケージのバージョンと一致しているかを利用側で検証できる。
///
/// wasm-bindgen は借用文字列を返せないため `String` を返す。
#[wasm_bindgen]
pub fn version() -> String {
    env!("SHIGUREDO_MP4_VERSION").to_string()
}
