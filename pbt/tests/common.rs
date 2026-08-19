//! PBT の共通ヘルパー
//!
//! 複数の integration test ファイルから `mod common;` 経由で参照する。

use noprop::TestCaseContext;
use shiguredo_mp4::boxes::TrakBox;
use shiguredo_mp4::mux::TrackMetadata;
use shiguredo_mp4::{LanguageCode, Utf8String};

/// `LanguageCode` として有効な 3 バイト（各 `0x60..=0x7F`）を生成する
pub fn arb_language_code(ctx: &mut TestCaseContext) -> LanguageCode {
    let bytes = [
        noprop::sample_u64_in(ctx, 0x60..=0x7F) as u8,
        noprop::sample_u64_in(ctx, 0x60..=0x7F) as u8,
        noprop::sample_u64_in(ctx, 0x60..=0x7F) as u8,
    ];
    LanguageCode::new(bytes).expect("生成値は常に有効な言語コードである")
}

/// null を含まない UTF-8 文字列から `Utf8String` を生成する
///
/// 上限 32 は PBT の実行コストを抑えるための任意の値で、`HdlrBox::name` に
/// 仕様上の長さ制約は無い（null 終端 UTF-8 の任意長）
pub fn arb_track_name(ctx: &mut TestCaseContext) -> Utf8String {
    let len = noprop::sample_usize_in(ctx, 0..=32);
    // null 文字を除外する。null 文字が Unicode スカラー全体に占める比率は極小のため
    // 実質的にほぼループせず必要文字数を得られる
    let mut s = String::new();
    while s.chars().count() < len {
        let c = noprop::sample_char(ctx);
        if c != '\0' {
            s.push(c);
        }
    }
    Utf8String::new(&s).expect("null 文字を含まない")
}

/// トラックメタデータを生成する
pub fn arb_track_metadata(ctx: &mut TestCaseContext) -> TrackMetadata {
    TrackMetadata {
        language: arb_language_code(ctx),
        name: arb_track_name(ctx),
    }
}

/// `mdhd.language` / `hdlr.name` が入力メタデータと一致することを確認する
pub fn assert_track_metadata(trak_box: &TrakBox, expected: &TrackMetadata) {
    assert_eq!(
        expected.language, trak_box.mdia_box.mdhd_box.language,
        "mdhd.language が入力と一致しない"
    );
    assert_eq!(
        expected.name.clone().into_null_terminated_bytes(),
        trak_box.mdia_box.hdlr_box.name,
        "hdlr.name が入力と一致しない"
    );
}
