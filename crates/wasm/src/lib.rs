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

pub mod boxes;
pub mod boxes_av01;
pub mod boxes_avc1;
pub mod boxes_flac;
pub mod boxes_hev1;
pub mod boxes_hvc1;
pub mod boxes_mp4a;
pub mod boxes_opus;
pub mod boxes_stpp;
pub mod boxes_tx3g;
pub mod boxes_vp08;
pub mod boxes_vp09;
pub mod boxes_wvtt;
pub mod demux;
pub mod fmp4_segment_demux;
pub mod fmp4_segment_mux;
pub mod mp4_file_kind_detector;
pub mod mux;
pub mod track;

/// ライブラリのバージョンを返す
///
/// 返り値はルートの `Cargo.toml` の `version` と同じ値になる。
/// npm パッケージのバージョンと一致しているかを利用側で検証できる。
#[wasm_bindgen]
pub fn version() -> String {
    env!("SHIGUREDO_MP4_VERSION").to_string()
}
