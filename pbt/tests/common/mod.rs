//! PBT の共通ヘルパー
//!
//! 複数の integration test ファイル（各ファイルが独立した test binary として
//! ビルドされる）から `mod common;` 経由で参照する。
//! 未使用関数の警告を各バイナリごとに抑えるため `#[allow(dead_code)]` を付ける。

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use shiguredo_mp4::boxes::TrakBox;
use shiguredo_mp4::mux::TrackMetadata;
use shiguredo_mp4::{LanguageCode, Utf8String};

/// `LanguageCode` として有効な 3 バイト（各 `0x60..=0x7F`）を生成する
#[allow(dead_code)]
pub fn arb_language_code() -> impl Strategy<Value = LanguageCode> {
    prop::array::uniform3(0x60u8..=0x7F).prop_map(|bytes| {
        LanguageCode::new(bytes).expect("Strategy が生成する値は常に有効な言語コードである")
    })
}

/// null を含まない UTF-8 文字列から `Utf8String` を生成する
///
/// 上限 32 は PBT の実行コストを抑えるための任意の値で、`HdlrBox::name` に
/// 仕様上の長さ制約は無い（null 終端 UTF-8 の任意長）
#[allow(dead_code)]
pub fn arb_track_name() -> impl Strategy<Value = Utf8String> {
    "[^\x00]{0,32}"
        .prop_map(|s| Utf8String::new(&s).expect("Strategy が生成する文字列は null を含まない"))
}

/// トラックメタデータを生成する
#[allow(dead_code)]
pub fn arb_track_metadata() -> impl Strategy<Value = TrackMetadata> {
    (arb_language_code(), arb_track_name())
        .prop_map(|(language, name)| TrackMetadata { language, name })
}

/// `mdhd.language` / `hdlr.name` が入力メタデータと一致することを確認する
#[allow(dead_code)]
pub fn assert_track_metadata(
    trak_box: &TrakBox,
    expected: &TrackMetadata,
) -> Result<(), TestCaseError> {
    // `MdhdBox::decode` は 5 ビットマスクで各バイトを `0x60..=0x7F` に丸めるため
    // 理論的にはここは必ず `Some` になる。防御的に prop_assert! で扱う
    let raw = trak_box.mdia_box.mdhd_box.language;
    let actual_language = LanguageCode::new(raw);
    prop_assert!(
        actual_language.is_some(),
        "muxer が LanguageCode 範囲外 (0x60..=0x7F 外) の language={:?} を出力した",
        raw
    );
    let actual_language = actual_language.expect("上の prop_assert! で Some を保証済み");
    prop_assert_eq!(
        expected.language,
        actual_language,
        "mdhd.language が入力と一致しない"
    );
    prop_assert_eq!(
        &expected.name.clone().into_null_terminated_bytes(),
        &trak_box.mdia_box.hdlr_box.name,
        "hdlr.name が入力と一致しない"
    );
    Ok(())
}
