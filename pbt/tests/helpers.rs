//! PBT の共通ヘルパー
//!
//! 複数の integration test ファイルから `mod helpers;` 経由で参照する。

use core::num::NonZeroU32;

use noprop::TestCaseContext;
use shiguredo_mp4::boxes::{ElstBox, ElstEntry, MoovBox, TrakBox};
use shiguredo_mp4::demux::TrackInfo;
use shiguredo_mp4::mux::{EditList, TrackMetadata};
use shiguredo_mp4::{FixedPointNumber, LanguageCode, Utf8String};

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

/// 編集リストを生成する（約半分の確率で `None`）
///
/// `media_timescale` にはトラックの `mdhd` の timescale を渡すこと。
/// `edit_duration` と `media_time` は換算で桁溢れしない小さな値に限定する。
pub fn arb_edit_list(ctx: &mut TestCaseContext, media_timescale: NonZeroU32) -> Option<EditList> {
    if !noprop::sample_bool(ctx) {
        return None;
    }

    let movie_timescale = NonZeroU32::new(noprop::sample_u64_in(ctx, 1..=100_000) as u32)
        .expect("1 以上なので非ゼロである");
    let entry_count = noprop::sample_u64_in(ctx, 1..=3);
    let mut entries = Vec::new();
    for _ in 0..entry_count {
        // 1/4 の確率で empty edit（media_time = -1）を生成する
        let media_time = if noprop::sample_u64_in(ctx, 0..=3) == 0 {
            -1
        } else {
            noprop::sample_u64_in(ctx, 0..=100_000) as i64
        };
        // 1.0 / 2.0 / 0.5 / -0.5 を生成する（i16.16 の小数部は 16 bit の生値で表す）
        let media_rate = match noprop::sample_u64_in(ctx, 0..=3) {
            0 => FixedPointNumber::new(2i16, 0i16),
            1 => FixedPointNumber::new(0i16, -32768i16),
            2 => FixedPointNumber::new(-1i16, -32768i16),
            _ => FixedPointNumber::new(1i16, 0i16),
        };
        entries.push(ElstEntry {
            edit_duration: noprop::sample_u64_in(ctx, 0..=100_000),
            media_time,
            media_rate,
        });
    }

    Some(EditList {
        movie_timescale,
        media_timescale,
        elst_box: ElstBox { entries },
    })
}

/// トラックメタデータを生成する
///
/// `media_timescale` にはトラックの `mdhd` の timescale を渡すこと。
pub fn arb_track_metadata(ctx: &mut TestCaseContext, media_timescale: NonZeroU32) -> TrackMetadata {
    TrackMetadata {
        language: arb_language_code(ctx),
        name: arb_track_name(ctx),
        edit_list: arb_edit_list(ctx, media_timescale),
    }
}

/// `mdhd.language` / `hdlr.name` / 編集リストが入力メタデータと一致することを確認する
///
/// `output_movie_timescale` には出力ファイルの `mvhd.timescale` を渡すこと。
/// 編集リストの `edit_duration` は、muxer が出力する movie timescale に換算されて
/// 書き出されるため、期待値も同じ換算で組み立てて比較する。
pub fn assert_track_metadata(
    trak_box: &TrakBox,
    expected: &TrackMetadata,
    output_movie_timescale: NonZeroU32,
) {
    assert_eq!(
        expected.language, trak_box.mdia_box.mdhd_box.language,
        "mdhd.language が入力と一致しない"
    );
    assert_eq!(
        expected.name.clone().into_null_terminated_bytes(),
        trak_box.mdia_box.hdlr_box.name,
        "hdlr.name が入力と一致しない"
    );

    match &expected.edit_list {
        Some(edit_list) => {
            let edts_box = trak_box
                .edts_box
                .as_ref()
                .expect("入力に編集リストがあるのに edts が出力されていない");
            let elst_box = edts_box
                .elst_box
                .as_ref()
                .expect("入力に編集リストがあるのに elst が出力されていない");
            assert_eq!(
                elst_box.entries.len(),
                edit_list.elst_box.entries.len(),
                "elst のエントリー数が入力と一致しない"
            );
            for (actual, expected_entry) in elst_box
                .entries
                .iter()
                .zip(edit_list.elst_box.entries.iter())
            {
                assert_eq!(
                    actual.media_time, expected_entry.media_time,
                    "elst の media_time が入力と一致しない"
                );
                assert_eq!(
                    actual.media_rate, expected_entry.media_rate,
                    "elst の media_rate が入力と一致しない"
                );
                assert_eq!(
                    actual.edit_duration,
                    convert_edit_duration_for_test(
                        expected_entry.edit_duration,
                        edit_list.movie_timescale,
                        output_movie_timescale,
                    ),
                    "elst の edit_duration が出力 movie timescale への換算と一致しない"
                );
            }
        }
        None => assert!(
            trak_box.edts_box.is_none(),
            "入力に編集リストがないのに edts が出力されている"
        ),
    }
}

/// demux 後の [`TrackInfo`] が `moov` の内容と一致することを確認する
pub fn assert_demuxed_track_info(track: &TrackInfo, moov_box: &MoovBox, index: usize) {
    assert_eq!(
        track.movie_timescale, moov_box.mvhd_box.timescale,
        "TrackInfo.movie_timescale が mvhd.timescale と一致しない"
    );
    let expected_edit_list = moov_box.trak_boxes[index]
        .edts_box
        .as_ref()
        .and_then(|edts_box| edts_box.elst_box.clone());
    assert_eq!(
        track.edit_list, expected_edit_list,
        "TrackInfo.edit_list が trak.edts_box.elst_box と一致しない"
    );
}

/// muxer と同じ四捨五入で `edit_duration` を換算する（テスト用）
fn convert_edit_duration_for_test(value: u64, from: NonZeroU32, to: NonZeroU32) -> u64 {
    let numerator = (value as u128) * (to.get() as u128) + (from.get() as u128) / 2;
    u64::try_from(numerator / (from.get() as u128)).expect("生成値の換算結果は u64 に収まる")
}
