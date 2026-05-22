//! MP4 のボックスのエンコードおよびデコードを行うためのライブラリ
#![cfg_attr(not(test), no_std)]
// library target (src/) 向けの追加 deny (restriction / pedantic。単体では allow)。
// `not(test)` … lib を cfg(test) なしでビルドするときだけ有効。tests/ は別 crate のため届かない。
// `[workspace.lints.clippy]` に書かない理由 … 同じ package の tests/ にも lint が乗るため。
// src/ 内の #[test] / #[cfg(test)] … clippy.toml の allow-*-in-tests で緩和。
// Clippy 組み込みデフォルト (correctness / style 等) … ここでは触らない (CI の `-D warnings` で昇格)。
#![warn(missing_docs)]
// no_std
// `core` で済む import に `std` を使うことを禁止する
#![cfg_attr(not(test), deny(clippy::std_instead_of_core))]
// `alloc` で済む import に `std` を使うことを禁止する
#![cfg_attr(not(test), deny(clippy::std_instead_of_alloc))]
// panic
// panic! を禁止する
#![cfg_attr(not(test), deny(clippy::panic))]
// unreachable! を禁止する
#![cfg_attr(not(test), deny(clippy::unreachable))]
// unwrap を Result / ? に寄せる
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
// expect を明示的なエラー処理に寄せる
#![cfg_attr(not(test), deny(clippy::expect_used))]
// すべての as キャストを禁止する (From / TryFrom / validate ヘルパへ。cast_* は包含されるため不要)
#![cfg_attr(not(test), deny(clippy::as_conversions))]
// 範囲外で panic しうるインデックス・スライスを禁止する
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]
// Result を返す関数内の panic! / assert! 等を禁止する
#![cfg_attr(not(test), deny(clippy::panic_in_result_fn))]
// 長さ未検証の refutable スライスパターンを禁止する
#![cfg_attr(not(test), deny(clippy::index_refutable_slice))]
// ゼロ除算で panic しうる整数除算を禁止する
#![cfg_attr(not(test), deny(clippy::integer_division))]
// ゼロ除算で panic しうる整数剰余を禁止する
#![cfg_attr(not(test), deny(clippy::integer_division_remainder_used))]

extern crate alloc;

mod auxiliary;
mod basic_types;
pub mod boxes;
mod boxes_fmp4;
mod boxes_moov_tree;
mod boxes_sample_entry;
mod codec;
pub mod demux;
mod demux_fmp4_file;
mod demux_fmp4_segment;
mod demux_mp4_file;
mod demux_mp4_file_kind_detector;
pub mod descriptors;
pub mod mux;
mod mux_fmp4_segment;
mod mux_mp4_file;

pub use basic_types::{
    BaseBox, BoxHeader, BoxSize, BoxType, Either, FixedPointNumber, FullBox, FullBoxFlags,
    FullBoxHeader, Mp4File, Mp4FileTime, SampleFlags, TrackKind, Uint, Utf8String,
};
pub use codec::{Decode, Encode, Error, ErrorKind, Result};

// [NOTE]
// Windows 環境では aux.rs というファイル名が予約語で、リポジトリに含まれていると git clone に失敗するため、
// ファイル名自体は auxiliary.rs にして lib.rs の中で aux モジュール以下に再エクスポートしている。
pub mod aux {
    //! MP4 の仕様とは直接は関係がない、実装上便利な補助的なコンポーネントを集めたモジュール

    pub use crate::auxiliary::{
        ChunkAccessor, SampleAccessor, SampleTableAccessor, SampleTableAccessorError,
    };
}
