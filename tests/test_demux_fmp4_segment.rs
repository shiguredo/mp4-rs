//! `Fmp4SegmentDemuxer` の意図的なエラーパスと、
//! `Fmp4SegmentMuxer` の出力を書き換えないと作れない入力の単体テスト
//!
//! `DemuxError::InvalidState` を返す次の 3 つのエラーパスを対象にする:
//! - 二重 `handle_init_segment`
//! - init 前の `tracks`
//! - init 前の `handle_media_segment`（正当な `moof` + `mdat` を渡した場合）
//!
//! `handle_media_segment` は構文解析が成功したあとに初めて `InvalidState` を返す。
//! 空・不正バイト列では `DecodeError` になるため、別の `Fmp4SegmentMuxer` で
//! 正当なセグメントを組み立ててから未初期化 demuxer に渡す。
//!
//! `handle_media_segment` が `DemuxError::DecodeError` を返す次のエラーパスを対象にする:
//! - 空のデータ
//! - `moof` が見つからないデータ（`styp` だけのデータ、`sidx` だけのデータ、
//!   `moof` より前、または `moof` の後ろのボックスの宣言サイズが入力の末尾を超えるデータ）
//! - `moof` より前にサイズが 0 のボックスがあるデータ（32 ビットの size=0 と、size=1 + largesize=0 の両方）
//! - `mdat` より先に `moof` が出るデータ
//! - `moof` と `mdat` の間にサイズが 0 のボックスがあるデータ（32 ビットの size=0 と、size=1 + largesize=0 の両方）
//! - `mdat` の後ろに size=1 + largesize=0 のボックスがあるデータ
//! - `mdat` の後ろのボックスの宣言サイズが入力の末尾を超えるデータ
//! - `mdat` の後ろにボックスヘッダー（8 バイト）に満たない端数があるデータ
//! - `moof` より前、`moof` と `mdat` の間、`mdat` の後ろのボックスのサイズが大きすぎて、
//!   次のボックスの位置を計算できないデータ
//! - `moof` より前のボックスの後ろで、次のボックスのヘッダーが途中で切れているデータ
//! - `moov` に存在しない track_id の `traf` を含むデータ
//! - 読み飛ばすトラックに `trex` がなく、その `traf` のサンプルサイズを `trun` と `tfhd` のどちらからも決められないデータ
//!   （サンプル行を持つ `trun` と、1 件以上の `TrunSamples::CountOnly` の `trun` の両方）
//! - 読み飛ばすトラックの `TrunSamples::CountOnly` の `trun` で、データ末尾が `usize` に収まらないデータ
//!
//! `moof` の前後にあるボックスを読み飛ばせる正常系は PBT で検証する。
//! 先頭の `styp` だけは型付きで読むため、`styp()` の値 (CMAF 指定で muxer が組み立てた値を含む)、
//! `styp` なしの場合、2 個目以降の `styp` の無視、壊れた `styp` の読み飛ばし、エラー時の値の不変も
//! 単体テストで固定する。
//! 意図的なエラーパスは固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//!
//! 読み飛ばすトラックのデータ末尾の計算のように、`Fmp4SegmentMuxer` の出力を書き換えないと作れない
//! 正常系の入力も、`build_skipped_track_segment` で組み立てて単体テストとして置く。
//!
//! CENC (DRM) の初期化データについては、`pssh` を保持した init セグメントと `Encv` / `Enca` を
//! 使ったメディアセグメントを組み立て、`pssh_boxes()` と各トラックの最初のサンプルの
//! `sample_entry` を確認する。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    BoxHeader, BoxSize, BoxType, Decode, Encode, Error, ErrorKind, FixedPointNumber, TrackKind,
    Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, Brand, EncaBox, EncvBox, FrmaBox, FtypBox,
        MoofBox, MoovBox, PsshBox, SaioBox, SaizBox, SampleEncryptionInfo, SampleEntry, SchiBox,
        SchmBox, SidxBox, SinfBox, StypBox, Subsample, TencBox, TrunBox, TrunSamples, UnknownBox,
        VisualSampleEntryFields,
    },
    demux::{DemuxError, Fmp4SegmentDemuxer},
    mux::{CmafTrackFileBrand, Fmp4SegmentMuxer, Sample, SegmentMuxerOptions},
};

const VIDEO_TIMESCALE: u32 = 90_000;
const AUDIO_TIMESCALE: u32 = 48_000;

/// 指定解像度の `SampleEntry::Avc1` を組み立てる
fn create_avc1_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Avc1(Avc1Box {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width,
            height,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        avcc_box: AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// payload が `data_size` バイトの映像サンプル 1 個分の `Sample` を組み立てる
fn create_video_sample(data_size: usize) -> Sample {
    Sample {
        encryption_info: None,
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(create_avc1_sample_entry(320, 240)),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset: 0,
        data_size,
    }
}

/// 正当な init セグメントとメディアセグメント（`moof` + `mdat` payload 付き）を組み立てる
///
/// demux 側の検証では構文的に正しいバイト列が必要なため、別 muxer の公開 API だけで生成する。
fn build_init_and_media_segments() -> (Vec<u8>, Vec<u8>) {
    // payload 長はこのファイルで検証するエラーパスには無関係の任意値。
    // `mdat` のヘッダーは payload サイズに応じて 8 / 16 バイトを選ぶため、
    // `u32::MAX - 8` を超えない範囲であれば境界の選択に影響しない。
    let payload = [0u8; 16];

    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[create_video_sample(payload.len())])
        .expect("media セグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);

    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    (init_segment, media_segment)
}

/// メディアセグメントを `moof` と、それに続くバイト列（`mdat` 以降）に分ける
fn split_moof(media_segment: &[u8]) -> (&[u8], &[u8]) {
    let (_moof_box, moof_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    media_segment.split_at(moof_size)
}

/// init セグメントの `moov` を `f` で書き換えたバイト列を返す
fn rewrite_init_segment(init_segment: &[u8], f: impl FnOnce(&mut MoovBox)) -> Vec<u8> {
    let (ftyp_box, ftyp_box_size) =
        FtypBox::decode(init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let (mut moov_box, moov_box_size) = MoovBox::decode(&init_segment[ftyp_box_size..])
        .expect("init セグメントからの moov デコードに失敗した");
    assert_eq!(
        ftyp_box_size + moov_box_size,
        init_segment.len(),
        "init セグメントは ftyp + moov のみを含む"
    );
    f(&mut moov_box);

    let mut rewritten = ftyp_box
        .encode_to_vec()
        .expect("init セグメント書き換え中の ftyp エンコードに失敗した");
    rewritten.extend_from_slice(
        &moov_box
            .encode_to_vec()
            .expect("init セグメント書き換え中の moov エンコードに失敗した"),
    );
    rewritten
}

/// メディアセグメントの `moof` を `f` で書き換え、`trun` の `data_offset` を `moof` のサイズの変化分だけ補正する
///
/// muxer の出力は `default_base_is_moof = true` かつ `base_data_offset` なしで、`trun` の `data_offset` は
/// `moof` の先頭からの相対値である。書き換えで `moof` のサイズが変わると `mdat` の位置もずれるため、
/// サイズの差を各 `trun` の `data_offset` に足してサンプルデータを指す位置を保つ
fn rewrite_media_segment_moof(media_segment: &[u8], f: impl FnOnce(&mut MoofBox)) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    f(&mut moof_box);

    // `data_offset` の補正は、書き換えた後のすべての `trun` の基準が `moof` の先頭であることを前提にしている
    for traf_box in &moof_box.traf_boxes {
        assert!(
            traf_box.tfhd_box.default_base_is_moof,
            "書き換えた後の traf は default_base_is_moof = true である"
        );
        assert_eq!(
            traf_box.tfhd_box.base_data_offset, None,
            "書き換えた後の traf は base_data_offset を明示しない"
        );
    }

    let rewritten_moof_size = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した")
        .len();
    let size_delta = i32::try_from(rewritten_moof_size as i64 - moof_box_size as i64)
        .expect("moof のサイズの差は i32 に収まる");
    for traf_box in &mut moof_box.traf_boxes {
        for trun_box in &mut traf_box.trun_boxes {
            let data_offset = trun_box
                .data_offset
                .expect("muxer は trun に data_offset を書く");
            trun_box.data_offset = Some(
                data_offset
                    .checked_add(size_delta)
                    .expect("補正した data_offset は i32 に収まる"),
            );
        }
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した");
    // `data_offset` の値だけを変えるので `moof` のサイズは変わらない
    assert_eq!(
        rewritten.len(),
        rewritten_moof_size,
        "data_offset の補正で moof のサイズが変わった"
    );
    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

/// 32 ビットの size=0 のボックス（入力の末尾まで続く）を組み立てる
fn build_variable_size_box(box_type: &[u8; 4], payload_len: usize) -> Vec<u8> {
    let mut box_bytes = Vec::new();
    box_bytes.extend_from_slice(&0u32.to_be_bytes());
    box_bytes.extend_from_slice(box_type);
    box_bytes.extend_from_slice(&vec![0u8; payload_len]);
    box_bytes
}

/// size=1 + largesize=0 のボックスを組み立てる
fn build_large_variable_size_box(box_type: &[u8; 4]) -> Vec<u8> {
    let mut box_bytes = Vec::new();
    box_bytes.extend_from_slice(&1u32.to_be_bytes());
    box_bytes.extend_from_slice(box_type);
    box_bytes.extend_from_slice(&0u64.to_be_bytes());
    box_bytes
}

/// 正当な init を 2 回 `handle_init_segment` すると `InvalidState` になること
#[test]
fn invalid_state_double_init() {
    let (init_segment, _media_segment) = build_init_and_media_segments();
    let mut demuxer = Fmp4SegmentDemuxer::new();

    demuxer
        .handle_init_segment(&init_segment)
        .expect("1 回目の handle_init_segment に失敗した");

    let result = demuxer.handle_init_segment(&init_segment);
    assert!(
        matches!(result, Err(DemuxError::InvalidState(_))),
        "二重 init では InvalidState を期待したが {:?} だった",
        result
    );
}

/// init 前に `tracks` を呼ぶと `InvalidState` になること
#[test]
fn invalid_state_tracks_before_init() {
    let demuxer = Fmp4SegmentDemuxer::new();
    let result = demuxer.tracks();
    assert!(
        matches!(result, Err(DemuxError::InvalidState(_))),
        "init 前の tracks では InvalidState を期待したが {:?} だった",
        result
    );
}

/// init 前に正当なメディアセグメントを渡すと `InvalidState` になること
#[test]
fn invalid_state_media_before_init() {
    let (_init_segment, media_segment) = build_init_and_media_segments();
    let mut demuxer = Fmp4SegmentDemuxer::new();

    let result = demuxer.handle_media_segment(&media_segment);
    assert!(
        matches!(result, Err(DemuxError::InvalidState(_))),
        "init 前の handle_media_segment では InvalidState を期待したが {:?} だった",
        result
    );
}

/// `init_segment` で初期化した新しい demuxer に `data` を `handle_media_segment` で渡し、`DecodeError` の中身を返す
///
/// 成功した場合や `DecodeError` 以外のエラーになった場合は、テスト失敗としてパニックする
fn expect_media_segment_decode_error(init_segment: &[u8], data: &[u8]) -> Error {
    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(init_segment)
        .expect("handle_init_segment に失敗した");

    match demuxer.handle_media_segment(data) {
        Err(DemuxError::DecodeError(error)) => error,
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// 空のデータを渡すと `InvalidInput` の `DecodeError` になること
///
/// 空のデータは、呼び出し側がセグメントを渡し損ねたものとして `InvalidInput` にする。
/// 中身はあるのに `moof` が見つからない壊れたセグメント（`InvalidData`）とは区別する
#[test]
fn decode_error_empty_media_segment() {
    let (init_segment, _media_segment) = build_init_and_media_segments();

    let error = expect_media_segment_decode_error(&init_segment, &[]);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidInput,
        "空のデータでは InvalidInput を期待した"
    );
    assert_eq!(
        error.reason, "empty media segment",
        "空のデータ専用のエラー理由を期待した"
    );
}

/// `styp` だけで `moof` がないデータを渡すと `InvalidData` の `DecodeError` になること
///
/// `moof` より前のボックスは読み飛ばすが、読み飛ばした結果として末尾に達した場合は
/// `moof` が見つからないことをエラーにする
#[test]
fn decode_error_moof_not_found_after_styp() {
    let (init_segment, _media_segment) = build_init_and_media_segments();

    // size=24 + 種別 `styp` + major_brand + minor_version + compatible_brands (2 個)
    //
    // 先頭の `styp` は demuxer が型付きで読むため、構文的に正しいブランドを並べる。
    // DASH のメディアセグメントでよく使われる `msdh` / `msix` にしておく
    let mut styp = Vec::new();
    styp.extend_from_slice(&24u32.to_be_bytes());
    styp.extend_from_slice(b"styp");
    styp.extend_from_slice(b"msdh");
    styp.extend_from_slice(&0u32.to_be_bytes());
    styp.extend_from_slice(b"msdh");
    styp.extend_from_slice(b"msix");

    let error = expect_media_segment_decode_error(&init_segment, &styp);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "moof がないデータでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "moof box not found in media segment",
        "moof が見つからないことを示すエラー理由を期待した"
    );
}

/// `sidx` だけで `moof` がないデータを渡すと、`styp` だけの場合と同じ `InvalidData` の `DecodeError` になること
///
/// `sidx` もほかのボックスと同じく読み飛ばす対象であり、`sidx` を特別扱いしないことを確認する
#[test]
fn decode_error_moof_not_found_after_sidx() {
    // muxer が生成する `sidx` 付きメディアセグメントから、先頭の `sidx` だけを切り出す
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    // payload 長はこのテストの検証内容には無関係の任意値。payload 自体は付けない
    let media_segment_metadata = muxer
        .create_media_segment_metadata_with_sidx(&[create_video_sample(16)])
        .expect("sidx 付き media セグメントの作成に失敗した");
    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    let (_sidx_box, sidx_size) =
        SidxBox::decode(&media_segment_metadata).expect("先頭の sidx のデコードに失敗した");

    let error =
        expect_media_segment_decode_error(&init_segment, &media_segment_metadata[..sidx_size]);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "sidx だけのデータでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "moof box not found in media segment",
        "moof が見つからないことを示すエラー理由を期待した"
    );
}

/// `moof` より前のボックスの宣言サイズが入力の末尾を超えると、`moof` が見つからない `InvalidData` の `DecodeError` になること
///
/// 宣言サイズで読み飛ばした先の位置は、入力の末尾をちょうど指すとは限らず、末尾を超えることがある。
/// 位置が末尾と一致する場合だけを判定する誤りがあると、範囲外のスライスで panic するため、
/// 末尾を超える場合も `moof` が見つからないエラーになることを固定する
#[test]
fn decode_error_box_exceeds_media_segment() {
    let (init_segment, _media_segment) = build_init_and_media_segments();

    // size=100 を宣言した `styp` ボックスのヘッダーの後ろに 16 バイトだけ置く（合計 24 バイト）
    let mut data = Vec::new();
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(b"styp");
    data.extend_from_slice(&[0u8; 16]);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "宣言サイズが入力を超えるボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "moof box not found in media segment",
        "moof が見つからないことを示すエラー理由を期待した"
    );
}

/// `moof` より前に 32 ビットの size=0 のボックスがあると `InvalidData` の `DecodeError` になること
///
/// size=0 のボックスは入力の末尾まで続くことを意味し、その後ろに `moof` は存在し得ない。
/// 後ろに `moof` + `mdat` のバイト列が続いていても、仕様上はそれもこのボックスの中身にあたるため、
/// `moof` としては扱わずエラーにする
#[test]
fn decode_error_size_zero_box_before_moof() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=0 の `free` ボックスのヘッダーだけを正当なメディアセグメントの前に置く
    let mut data = build_variable_size_box(b"free", 0);
    data.extend_from_slice(&media_segment);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "moof より前の size=0 のボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "found box with size=0 before moof in media segment",
        "moof より前の size=0 のボックスを示すエラー理由を期待した"
    );
}

/// `moof` より前に size=1 + largesize=0 のボックスがあると、32 ビットの size=0 と同じエラーになること
///
/// largesize=0 は仕様上の意味が定められておらず、読み飛ばし先を決められない。
/// ボックスサイズとしては 32 ビットの size=0 と同じく 0 になるため、同じエラーにする
#[test]
fn decode_error_large_size_zero_box_before_moof() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=1 + 種別 `free` + largesize=0 のヘッダーだけを正当なメディアセグメントの前に置く
    let mut data = build_large_variable_size_box(b"free");
    data.extend_from_slice(&media_segment);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "moof より前の largesize=0 のボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "found box with size=0 before moof in media segment",
        "moof より前の size=0 のボックスを示すエラー理由を期待した"
    );
}

/// `moof` より前のボックスの largesize が大きすぎて次のボックスの位置を計算できないと、
/// `InvalidData` の `DecodeError` になること
///
/// 64 ビット環境では位置の加算がオーバーフローし、32 ビット環境ではボックスサイズを `usize` に変換できない。
/// どちらも次のボックスを読めないため、同じ種別のエラーにする。
///
/// largesize を 32 ビットに切り詰めるような誤りがあった場合も、エラー理由の違いで検出できる。
/// 64 ビット環境では位置の加算がオーバーフローせずに `moof` が見つからないエラーになり、
/// 32 ビット環境では `usize` への変換を通過して位置の加算がオーバーフローするためである
#[test]
fn decode_error_box_offset_overflow_before_moof() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // 8 バイトの `free` ボックスの後ろに、largesize=u64::MAX の `free` ボックスのヘッダーを置く。
    // 64 ビット環境では、先頭の `free` がないと位置が 0 のまま u64::MAX を足すことになり、オーバーフローしない
    let mut data = Vec::new();
    data.extend_from_slice(&8u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&u64::MAX.to_be_bytes());
    data.extend_from_slice(&media_segment);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "位置を計算できないボックスでは InvalidData を期待した"
    );
    let expected_reason = if cfg!(target_pointer_width = "64") {
        "box offset overflow in media segment"
    } else {
        "box size exceeds usize::MAX"
    };
    assert_eq!(
        error.reason, expected_reason,
        "位置を計算できないことを示すエラー理由を期待した"
    );
}

/// `moof` より前のボックスを読み飛ばした後ろで、次のボックスのヘッダーが途中で切れていると、
/// `InsufficientBuffer` の `DecodeError` になること
///
/// ボックスヘッダーを読むのに足りないバイト列は、`BoxHeader` のデコードエラーをそのまま返す。
/// `handle_init_segment` の読み飛ばしや `moof` / `mdat` の解析でも同じ扱いであり、
/// ここだけ別の種別に変換しない
#[test]
fn decode_error_truncated_box_header_after_skipped_box() {
    let (init_segment, _media_segment) = build_init_and_media_segments();

    // 8 バイトの `styp` ボックス（ヘッダーだけ）の後ろに、ボックスヘッダーの最小サイズ（8 バイト）に
    // 満たない 4 バイトだけを置く
    let mut data = Vec::new();
    data.extend_from_slice(&8u32.to_be_bytes());
    data.extend_from_slice(b"styp");
    data.extend_from_slice(&[0u8; 4]);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InsufficientBuffer,
        "ヘッダーが途中で切れているデータでは InsufficientBuffer を期待した"
    );
}

/// `mdat` より先に `moof` が出ると `InvalidData` の `DecodeError` になること
///
/// 1 回の呼び出しで処理できるのは `moof` + `mdat` 1 組だけであり、
/// `moof` の後ろに別の `moof` があってもその中身は解析しない
#[test]
fn decode_error_moof_before_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (moof_bytes, _mdat_bytes) = split_moof(&media_segment);

    // `moof` の後ろに、もう 1 組の `moof` + `mdat` を置く
    let mut data = Vec::new();
    data.extend_from_slice(moof_bytes);
    data.extend_from_slice(&media_segment);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "mdat より先に moof が出るデータでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "expected mdat box after moof but got BoxType(\"moof\")",
        "mdat の代わりに moof が出たことを示すエラー理由を期待した"
    );
}

/// `moof` と `mdat` の間のボックスの largesize が大きすぎて次のボックスの位置を計算できないと、
/// `InvalidData` の `DecodeError` になること
///
/// 64 ビット環境では位置の加算がオーバーフローし、32 ビット環境ではボックスサイズを `usize` に変換できない。
/// どちらも次のボックスを読めないため、同じ種別のエラーにする。
///
/// largesize を 32 ビットに切り詰めるような誤りがあった場合も、エラー理由の違いで検出できる
#[test]
fn decode_error_box_offset_overflow_between_moof_and_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (moof_bytes, mdat_bytes) = split_moof(&media_segment);

    // size=1 + 種別 `free` + largesize=u64::MAX のヘッダーを `moof` と `mdat` の間に置く
    let mut data = Vec::new();
    data.extend_from_slice(moof_bytes);
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&u64::MAX.to_be_bytes());
    data.extend_from_slice(mdat_bytes);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "位置を計算できないボックスでは InvalidData を期待した"
    );
    let expected_reason = if cfg!(target_pointer_width = "64") {
        "box offset overflow in media segment"
    } else {
        "box size exceeds usize::MAX"
    };
    assert_eq!(
        error.reason, expected_reason,
        "位置を計算できないことを示すエラー理由を期待した"
    );
}

/// `mdat` の後ろのボックスの largesize が大きすぎて次のボックスの位置を計算できないと、
/// `InvalidData` の `DecodeError` になること
#[test]
fn decode_error_box_offset_overflow_after_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=1 + 種別 `free` + largesize=u64::MAX のヘッダーを `mdat` の後ろに置く
    let mut data = media_segment;
    data.extend_from_slice(&1u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&u64::MAX.to_be_bytes());

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "位置を計算できないボックスでは InvalidData を期待した"
    );
    let expected_reason = if cfg!(target_pointer_width = "64") {
        "box offset overflow in media segment"
    } else {
        "box size exceeds usize::MAX"
    };
    assert_eq!(
        error.reason, expected_reason,
        "位置を計算できないことを示すエラー理由を期待した"
    );
}

/// `moof` の後ろのボックスの宣言サイズが入力の末尾を超えると、`mdat` が見つからない `InvalidData` の `DecodeError` になること
///
/// 宣言サイズで読み飛ばした先の位置は、入力の末尾をちょうど指すとは限らず、末尾を超えることがある。
/// 位置が末尾と一致する場合だけを判定する誤りがあると、範囲外のスライスで panic する
#[test]
fn decode_error_box_after_moof_exceeds_media_segment() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (moof_bytes, _mdat_bytes) = split_moof(&media_segment);

    // `moof` の直後に、size=100 を宣言した `free` ボックスのヘッダーと 8 バイトだけ置く
    let mut data = Vec::new();
    data.extend_from_slice(moof_bytes);
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&[0u8; 8]);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "宣言サイズが入力を超えるボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "mdat box not found after moof",
        "mdat が見つからないことを示すエラー理由を期待した"
    );
}

/// `moof` と `mdat` の間に 32 ビットの size=0 のボックスがあると `InvalidData` の `DecodeError` になること
///
/// size=0 のボックスは入力の末尾まで続くことを意味し、その後ろに `mdat` は存在し得ない。
/// `moof` より前の size=0 のボックスと同じ理由でエラーにする
#[test]
fn decode_error_size_zero_box_between_moof_and_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (moof_bytes, mdat_bytes) = split_moof(&media_segment);

    let mut data = Vec::new();
    data.extend_from_slice(moof_bytes);
    data.extend_from_slice(&build_variable_size_box(b"free", 4));
    data.extend_from_slice(mdat_bytes);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "moof と mdat の間の size=0 のボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "found box with size=0 between moof and mdat in media segment",
        "moof と mdat の間の size=0 のボックスを示すエラー理由を期待した"
    );
}

/// `moof` と `mdat` の間に size=1 + largesize=0 のボックスがあると、32 ビットの size=0 と同じエラーになること
#[test]
fn decode_error_large_size_zero_box_between_moof_and_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();
    let (moof_bytes, mdat_bytes) = split_moof(&media_segment);

    let mut data = Vec::new();
    data.extend_from_slice(moof_bytes);
    data.extend_from_slice(&build_large_variable_size_box(b"free"));
    data.extend_from_slice(mdat_bytes);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "moof と mdat の間の largesize=0 のボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "found box with size=0 between moof and mdat in media segment",
        "moof と mdat の間の size=0 のボックスを示すエラー理由を期待した"
    );
}

/// `mdat` の後ろに size=1 + largesize=0 のボックスがあると `InvalidData` の `DecodeError` になること
///
/// largesize=0 は仕様上の意味が定められておらず、読み飛ばし先を決められない
#[test]
fn decode_error_large_size_zero_box_after_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    let mut data = media_segment;
    data.extend_from_slice(&build_large_variable_size_box(b"free"));

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "mdat の後ろの largesize=0 のボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "found box with size=0 after mdat in media segment",
        "mdat の後ろの size=0 のボックスを示すエラー理由を期待した"
    );
}

/// `mdat` の後ろのボックスの宣言サイズが入力の末尾を超えると `InvalidData` の `DecodeError` になること
///
/// 宣言サイズで読み飛ばした先の位置は、入力の末尾をちょうど指すとは限らず、末尾を超えることがある。
/// 位置が末尾と一致する場合だけを判定する誤りがあると、範囲外のスライスで panic する
#[test]
fn decode_error_box_after_mdat_exceeds_media_segment() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=100 を宣言した `free` ボックスのヘッダーの後ろに 8 バイトだけ置く（合計 16 バイト）
    let mut data = media_segment;
    data.extend_from_slice(&100u32.to_be_bytes());
    data.extend_from_slice(b"free");
    data.extend_from_slice(&[0u8; 8]);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "宣言サイズが入力を超えるボックスでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "box after mdat exceeds media segment boundary",
        "mdat の後ろのボックスが入力を超えることを示すエラー理由を期待した"
    );
}

/// `mdat` の後ろにボックスヘッダーに満たない端数があると `InsufficientBuffer` の `DecodeError` になること
///
/// `moof` より前や `moof` と `mdat` の間の読み飛ばしと同じく、
/// `BoxHeader` のデコードエラーをそのまま返す
#[test]
fn decode_error_trailing_bytes_after_mdat() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // ボックスヘッダーの最小サイズ（8 バイト）に満たない 4 バイトだけを置く
    let mut data = media_segment;
    data.extend_from_slice(&[0u8; 4]);

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InsufficientBuffer,
        "端数があるデータでは InsufficientBuffer を期待した"
    );
}

/// 読み飛ばすトラックのサンプル 1 個分のデータのサイズ
///
/// 読み飛ばすトラックの `trex` または `tfhd` の既定のサンプルサイズと揃える
const SKIPPED_SAMPLE_SIZE: u32 = 8;

/// 対応しているトラックのサンプルの payload のサイズ
const TRACK_SAMPLE_SIZE: usize = 8;

/// 32 ビットの `mdat` のヘッダーのサイズ
const MDAT_HEADER_SIZE: usize = 8;

/// 読み飛ばすトラックの各 `trun` のサンプルの持ち方
#[derive(Debug, Clone, Copy)]
enum SkippedRun {
    /// サイズを書かないサンプル行を 1 つ持つ（尺とフラグは書く）
    OneRowWithoutSize,
    /// サンプルごとのフィールドを持たない、件数だけの形式
    CountOnly(u32),
}

impl SkippedRun {
    /// この `trun` 1 つに含まれるサンプル数
    fn sample_count(self) -> usize {
        match self {
            Self::OneRowWithoutSize => 1,
            Self::CountOnly(sample_count) => sample_count as usize,
        }
    }
}

/// 読み飛ばすトラックの `traf` を含む init セグメントとメディアセグメントを組み立てる
///
/// 読み飛ばすトラックは track_id を 2、ハンドラー種別を `meta` にしたトラックで、`moof` の先頭の `traf` に置く。
/// そのサンプルサイズは `tfhd` の `default_sample_size`（`tfhd_default_sample_size`）と
/// `trex` の `default_sample_size`（`trex_default_sample_size`）のどちらかから決め、`trun` には書かない。
/// `trex_default_sample_size` が `None` の場合は、読み飛ばすトラックの `trex` を init セグメントに置かない。
/// 両方とも `None` の場合はサンプルサイズを決められない入力になる。
///
/// 対応しているトラック（track_id は 1）のサンプルは、読み飛ばすトラックのデータの直後に置く。
/// すべての `traf` の `tfhd` を `default_base_is_moof = false` にしてあるため、
/// 対応しているトラックの基準位置は読み飛ばすトラックのデータ末尾になる。
///
/// `skipped_run_count` は読み飛ばすトラックの `traf` の `trun` の数で、各 `trun` のサンプルの持ち方は
/// `skipped_run` で決める。`skipped_run_count` が 1 より大きい場合は 2 つ目以降の `data_offset` を省く
/// （直前の `trun` のデータ末尾から始まる）。`mdat` の payload には、読み飛ばすトラックのデータが
/// `skipped_run_count` × `skipped_run` のサンプル数の分だけ入る
///
/// 戻り値は (init セグメント, メディアセグメント, 書き換えた `moof` のサイズ)
fn build_skipped_track_segment(
    tfhd_default_sample_size: Option<u32>,
    trex_default_sample_size: Option<u32>,
    skipped_run_count: usize,
    skipped_run: SkippedRun,
) -> (Vec<u8>, Vec<u8>, usize) {
    let payload = [0u8; TRACK_SAMPLE_SIZE];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[create_video_sample(payload.len())])
        .expect("media セグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    // 読み飛ばすトラックと、必要ならその `trex` を足す
    let init_segment = rewrite_init_segment(&init_segment, |moov_box| {
        let mut trak = moov_box.trak_boxes[0].clone();
        trak.tkhd_box.track_id = 2;
        trak.mdia_box.hdlr_box.handler_type = *b"meta";
        moov_box.trak_boxes.push(trak);

        let mvex_box = moov_box.mvex_box.as_mut().expect("mvex を含む");
        // muxer は `trex` の既定のサンプルサイズに 0 を書く。
        // 読み飛ばすトラックの計算にこの値が使われていないことを確かめられるようにする
        assert_eq!(
            mvex_box.trex_boxes[0].default_sample_size, 0,
            "muxer は trex の既定のサンプルサイズに 0 を書く"
        );
        if let Some(default_sample_size) = trex_default_sample_size {
            let mut trex_box = mvex_box.trex_boxes[0].clone();
            trex_box.track_id = 2;
            trex_box.default_sample_size = default_sample_size;
            mvex_box.trex_boxes.push(trex_box);
        }
    });

    // メディアセグメントの先頭に、読み飛ばすトラックの `traf` を足す
    let (mut moof_box, original_moof_size) =
        MoofBox::decode(&media_segment).expect("media セグメントからの moof デコードに失敗した");
    moof_box.traf_boxes[0].tfhd_box.default_base_is_moof = false;
    let mut skipped_traf_box = moof_box.traf_boxes[0].clone();
    skipped_traf_box.tfhd_box.track_id = 2;
    skipped_traf_box.tfhd_box.default_base_is_moof = false;
    skipped_traf_box.tfhd_box.default_sample_size = tfhd_default_sample_size;
    // 読み飛ばすトラックに、`skipped_run` で決めた `trun` を `skipped_run_count` 個並べる。
    // 2 つ目以降は `data_offset` を省く（`moof` のサイズが確定してから最初の `trun` にだけ設定する）
    let base_trun_box = skipped_traf_box.trun_boxes[0].clone();
    skipped_traf_box.trun_boxes = (0..skipped_run_count)
        .map(|index| {
            let mut trun_box = base_trun_box.clone();
            trun_box.samples = match skipped_run {
                SkippedRun::OneRowWithoutSize => {
                    let TrunSamples::PerSample(samples) = &base_trun_box.samples else {
                        panic!("muxer は trun にサンプルごとの値を書く");
                    };
                    let mut sample = samples[0].clone();
                    sample.size = None;
                    TrunSamples::PerSample(vec![sample])
                }
                SkippedRun::CountOnly(sample_count) => TrunSamples::CountOnly(sample_count),
            };
            // 最初の `trun` は `moof` のサイズが確定してから値を入れる。
            // フィールドの有無で `moof` のサイズが変わらないよう、ここでは仮の値を入れておく
            trun_box.data_offset = (index == 0).then_some(0);
            trun_box
        })
        .collect();
    moof_box.traf_boxes.insert(0, skipped_traf_box);

    // `trun` の `data_offset` は `moof` のサイズで決まるため、いったん符号化してサイズを確定させる。
    // 最初の `trun` には仮の値が入っており、値を入れ替えても `moof` のサイズは変わらない
    let moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    // 読み飛ばすトラックのデータは `mdat` の payload の先頭にある。
    // 2 つ目以降の `trun` の `data_offset` は省いたままにする
    moof_box.traf_boxes[0].trun_boxes[0].data_offset =
        Some(i32::try_from(moof_size + MDAT_HEADER_SIZE).expect("i32 に収まる"));
    // 対応しているトラックのサンプルは、読み飛ばすトラックのデータの直後にある
    moof_box.traf_boxes[1].trun_boxes[0].data_offset = Some(0);

    // `mdat` の payload の先頭に、読み飛ばすトラックのデータを挿し込む
    let skipped_data_size =
        SKIPPED_SAMPLE_SIZE as usize * skipped_run_count * skipped_run.sample_count();
    let mut mdat = media_segment[original_moof_size..].to_vec();
    let mdat_size = u32::from_be_bytes(mdat[0..4].try_into().expect("mdat のサイズは 4 バイト"));
    mdat[0..4].copy_from_slice(
        &(mdat_size + u32::try_from(skipped_data_size).expect("u32 に収まる")).to_be_bytes(),
    );
    mdat.splice(
        MDAT_HEADER_SIZE..MDAT_HEADER_SIZE,
        vec![0u8; skipped_data_size],
    );

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        rewritten.len(),
        moof_size,
        "data_offset の設定で moof のサイズは変わらない"
    );
    rewritten.extend_from_slice(&mdat);
    (init_segment, rewritten, moof_size)
}

/// [`build_skipped_track_segment`] が作る入力で、対応しているトラックのサンプルがある位置
///
/// `mdat` の payload の先頭に読み飛ばすトラックのデータ（`skipped_sample_count` 個分）があり、
/// その直後に対応しているトラックのサンプルがある
fn expected_sample_data_offset(moof_size: usize, skipped_sample_count: usize) -> u64 {
    u64::try_from(
        moof_size + MDAT_HEADER_SIZE + SKIPPED_SAMPLE_SIZE as usize * skipped_sample_count,
    )
    .expect("u64 に収まる")
}

/// `moov` に存在しない track_id の `traf` を含むメディアセグメントは `InvalidData` の `DecodeError` になること
///
/// 初期化セグメントで読み飛ばすトラックは track_id を記録しているが、`moov` に存在しない track_id は
/// 読み飛ばす対象でもないため、これまでどおりエラーにする
#[test]
fn decode_error_unknown_track_id_in_media_segment() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // `moov` に存在しない track_id に書き換える（muxer は track_id を 1 から採番する）
    let data = rewrite_media_segment_moof(&media_segment, |moof_box| {
        moof_box.traf_boxes[0].tfhd_box.track_id = 2;
    });

    let error = expect_media_segment_decode_error(&init_segment, &data);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "存在しない track_id の traf では InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "unknown track_id in media segment: 2",
        "存在しない track_id を示すエラー理由を期待した"
    );
}

/// 読み飛ばすトラックの `traf` で `trex` の既定値が要るのに `trex` がないと `InvalidData` の `DecodeError` になること
///
/// 読み飛ばすトラックの `traf` はサンプルを返さないが、`default_base_is_moof` の値によらずデータ末尾を計算する
/// （`false` の場合は次の `traf` の基準位置になる）。サンプルサイズを `trun` と `tfhd` の
/// どちらからも決められない場合は `trex` の既定値が要る。
/// サイズを書かないサンプル行を持つ `trun` と、1 件以上の `TrunSamples::CountOnly` の `trun` の両方で確認する
#[test]
fn decode_error_skipped_track_without_trex() {
    for skipped_run in [SkippedRun::OneRowWithoutSize, SkippedRun::CountOnly(2)] {
        let (init_segment, data, _moof_size) =
            build_skipped_track_segment(None, None, 1, skipped_run);

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_segment)
            .expect("読み飛ばすトラックに trex がなくても init セグメントの処理は成功する");
        assert_eq!(
            demuxer.tracks().expect("tracks の取得に失敗した").len(),
            1,
            "未対応のハンドラー種別のトラックはトラック情報に登録されない"
        );

        let state_before_error = format!("{demuxer:?}");
        let result = demuxer
            .handle_media_segment(&data)
            .map(|samples| samples.len());
        let Err(DemuxError::DecodeError(error)) = &result else {
            panic!("{skipped_run:?}: DecodeError を期待したが {result:?} だった");
        };
        assert_eq!(
            error.kind,
            ErrorKind::InvalidData,
            "{skipped_run:?}: trex がない読み飛ばすトラックでは InvalidData を期待した"
        );
        assert_eq!(
            error.reason, "trex not found for skipped track_id=2",
            "{skipped_run:?}: trex がないことを示すエラー理由を期待した"
        );
        assert_eq!(
            format!("{demuxer:?}"),
            state_before_error,
            "{skipped_run:?}: エラーを返した後に内部状態が変わった"
        );
    }
}

/// 読み飛ばすトラックの `TrunSamples::CountOnly` の `trun` で、データ末尾の計算が溢れると
/// `InvalidData` の `DecodeError` になること
///
/// 読み飛ばすトラックの `tfhd` の `base_data_offset` を、既定サイズ 8 バイトのサンプル 2 個分（16 バイト）を
/// 足すとちょうど 1 バイトだけ `usize` に収まらない位置にし、`trun` の `data_offset` を省く。
/// サンプル 1 個分（8 バイト）だけなら収まるため、件数を掛けずに求める誤りも検出できる。
/// `usize` の上限から値を決めるため、32 ビット環境でも同じエラーになる。
/// 対応しているトラックの `traf` は `default_base_is_moof = true` にして、読み飛ばすトラックのデータ末尾を
/// 基準位置に使わないようにする。エラーが読み飛ばすトラックの計算からしか出ないようにするためである
#[test]
fn decode_error_skipped_track_count_only_run_data_end_overflow() {
    let skipped_run = SkippedRun::CountOnly(2);
    let (init_segment, data, _moof_size) =
        build_skipped_track_segment(Some(SKIPPED_SAMPLE_SIZE), None, 1, skipped_run);
    let (mut moof_box, original_moof_size) =
        MoofBox::decode(&data).expect("media セグメントからの moof デコードに失敗した");
    let skipped_traf_box = &mut moof_box.traf_boxes[0];
    assert_eq!(
        skipped_traf_box.tfhd_box.track_id, 2,
        "moof の先頭の traf は読み飛ばすトラックのもの"
    );
    let skipped_data_size = SKIPPED_SAMPLE_SIZE as usize * skipped_run.sample_count();
    skipped_traf_box.tfhd_box.base_data_offset =
        Some(u64::try_from(usize::MAX - (skipped_data_size - 1)).expect("usize は u64 に収まる"));
    skipped_traf_box.trun_boxes[0].data_offset = None;
    moof_box.traf_boxes[1].tfhd_box.default_base_is_moof = true;

    // 対応しているトラックのサンプルは、mdat の payload の先頭にある読み飛ばすトラックのデータの直後にある。
    // `moof` の先頭からの位置で指すため、書き換えた後の `moof` のサイズを先に確定させる
    let moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    moof_box.traf_boxes[1].trun_boxes[0].data_offset = Some(
        i32::try_from(
            moof_size
                + MDAT_HEADER_SIZE
                + SKIPPED_SAMPLE_SIZE as usize * skipped_run.sample_count(),
        )
        .expect("i32 に収まる"),
    );
    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        rewritten.len(),
        moof_size,
        "data_offset の値の変更で moof のサイズは変わらない"
    );
    rewritten.extend_from_slice(&data[original_moof_size..]);

    let error = expect_media_segment_decode_error(&init_segment, &rewritten);
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "データ末尾の計算が溢れる場合は InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "sample data offset overflow",
        "データ末尾の計算が溢れたことを示すエラー理由を期待した"
    );
}

/// 読み飛ばすトラックのデータ末尾を `trex` の既定のサンプルサイズから計算すること
///
/// `default_base_is_moof = false` かつ `base_data_offset` なしの場合、2 番目以降の `traf` の基準位置は
/// 直前の `traf` のデータ末尾になる。読み飛ばすトラックのサンプルサイズが `trun` と `tfhd` の
/// どちらにもない場合は `trex` の `default_sample_size` を使う
#[test]
fn skipped_track_data_end_uses_trex_default_sample_size() {
    let (init_segment, data, moof_size) = build_skipped_track_segment(
        None,
        Some(SKIPPED_SAMPLE_SIZE),
        1,
        SkippedRun::OneRowWithoutSize,
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("media セグメントの処理に失敗した");
    assert_eq!(
        samples.len(),
        1,
        "対応しているトラックのサンプルが 1 つ返る"
    );
    assert_eq!(
        samples[0].data_offset,
        expected_sample_data_offset(moof_size, 1),
        "読み飛ばすトラックのデータ末尾の分だけ data_offset がずれる"
    );
    assert_eq!(
        samples[0].data_size, TRACK_SAMPLE_SIZE,
        "サンプルサイズは変わらない"
    );
}

/// 読み飛ばすトラックに `trex` がなくても、`tfhd` の既定のサンプルサイズからデータ末尾を計算すること
///
/// サンプルサイズは `trun`、`tfhd` の `default_sample_size`、`trex` の `default_sample_size` の順に決めるため、
/// `tfhd` に既定値があれば `trex` がなくてもエラーにならない
#[test]
fn skipped_track_data_end_uses_tfhd_default_sample_size() {
    let (init_segment, data, moof_size) = build_skipped_track_segment(
        Some(SKIPPED_SAMPLE_SIZE),
        None,
        1,
        SkippedRun::OneRowWithoutSize,
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("media セグメントの処理に失敗した");
    assert_eq!(
        samples.len(),
        1,
        "対応しているトラックのサンプルが 1 つ返る"
    );
    assert_eq!(
        samples[0].data_offset,
        expected_sample_data_offset(moof_size, 1),
        "読み飛ばすトラックのデータ末尾の分だけ data_offset がずれる"
    );
}

/// 読み飛ばすトラックの `traf` に `data_offset` のない 2 つ目の `trun` があるとき、
/// データ末尾を直前の `trun` のデータの直後から計算すること
///
/// `skipped_traf_data_end` も `handle_media_segment` と同じ規則で run の開始位置を決める。
/// 規則が古いままだと、読み飛ばす `traf` のデータ末尾が短くなり、次のトラックの基準位置がずれる
#[test]
fn skipped_track_data_end_uses_previous_run() {
    let (init_segment, data, moof_size) = build_skipped_track_segment(
        None,
        Some(SKIPPED_SAMPLE_SIZE),
        2,
        SkippedRun::OneRowWithoutSize,
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("media セグメントの処理に失敗した");
    assert_eq!(
        samples.len(),
        1,
        "対応しているトラックのサンプルが 1 つ返る"
    );
    assert_eq!(
        samples[0].data_offset,
        expected_sample_data_offset(moof_size, 2),
        "読み飛ばすトラックの 2 つの run のデータの分だけ data_offset がずれる"
    );
}

/// 読み飛ばすトラックの `TrunSamples::CountOnly` の `trun` から、既定のサンプルサイズと件数の積で
/// データ末尾を計算すること
///
/// `TrunSamples::CountOnly` の `trun` は全サンプルが同じ既定サイズになる。ここでは `trex` の
/// `default_sample_size` を使う。`data_offset` のない 2 つ目の `trun` は、1 つ目の `trun` のデータの直後から始まる。
/// PBT のヘルパー（`rewrite_media_segment_default_base_is_moof_false`）はすべての `trun` が `data_offset` を
/// 持つことを前提にしており、`data_offset` のない 2 つ目の `trun` と `trex` の既定値の組み合わせを作れないため、
/// 固定入力で確認する
#[test]
fn skipped_track_data_end_uses_count_only_runs() {
    let skipped_run = SkippedRun::CountOnly(3);
    let (init_segment, data, moof_size) =
        build_skipped_track_segment(None, Some(SKIPPED_SAMPLE_SIZE), 2, skipped_run);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("media セグメントの処理に失敗した");
    assert_eq!(
        samples.len(),
        1,
        "対応しているトラックのサンプルが 1 つ返る"
    );
    assert_eq!(
        samples[0].data_offset,
        expected_sample_data_offset(moof_size, 2 * skipped_run.sample_count()),
        "読み飛ばすトラックのデータ（3 サンプルの trun が 2 つ）の分だけ data_offset がずれる"
    );
}

/// 読み飛ばすトラックの `TrunSamples::CountOnly(0)` の `trun` では、既定のサンプルサイズを決めないこと
///
/// データがないため既定サイズは要らない。読み飛ばすトラックに `trex` がなく、`tfhd` にも既定値がなくても
/// エラーにならず、読み飛ばすトラックのデータ末尾は `trun` のデータの開始位置になる
#[test]
fn skipped_track_count_only_run_without_samples_does_not_need_trex() {
    let (init_segment, data, moof_size) =
        build_skipped_track_segment(None, None, 1, SkippedRun::CountOnly(0));

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("読み飛ばすトラックに trex がなくても init セグメントの処理は成功する");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("件数 0 の trun だけなら trex がなくても media セグメントの処理は成功する");
    assert_eq!(
        samples.len(),
        1,
        "対応しているトラックのサンプルが 1 つ返る"
    );
    assert_eq!(
        samples[0].data_offset,
        expected_sample_data_offset(moof_size, 0),
        "読み飛ばすトラックのデータがないので、対応しているトラックのサンプルは mdat の payload の先頭にある"
    );
}

// ===== CENC (DRM) の初期化データのテスト =====

/// DRM テスト用の映像トラックの KID
const VIDEO_KID: [u8; 16] = [0x11; 16];

/// DRM テスト用の音声トラックの KID
const AUDIO_KID: [u8; 16] = [0x22; 16];

/// `frma=hev1` の `EncvBox` を持つ `SampleEntry::Encv` を組み立てる
///
/// CMSF 4.2 が要求する `schm` と `schi` / `tenc` を含める。`cbcs` はパターン暗号化を
/// 使うスキームなので、`tenc` はパターン暗号化パラメーターを持つ version 1 にする。
fn create_encv_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Encv(EncvBox {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width,
            height,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        sinf_box: SinfBox {
            frma_box: FrmaBox {
                data_format: *b"hev1",
            },
            schm_box: Some(SchmBox {
                scheme_type: *b"cbcs",
                // 保護スキームのバージョンは任意値
                scheme_version: 0x0001_0000,
                scheme_uri: None,
            }),
            schi_box: Some(SchiBox {
                tenc_box: Some(TencBox {
                    default_is_protected: 1,
                    default_per_sample_iv_size: 16,
                    default_kid: VIDEO_KID,
                    default_constant_iv: None,
                    default_crypt_byte_block: Some(Uint::new(1)),
                    default_skip_byte_block: Some(Uint::new(9)),
                }),
                unknown_boxes: vec![],
            }),
            unknown_boxes: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// `frma=mp4a` の `EncaBox` を持つ `SampleEntry::Enca` を組み立てる
///
/// `cenc` はパターン暗号化を使わないスキームなので、
/// `tenc` はパターン暗号化パラメーターを持たない version 0 にする。
fn create_enca_sample_entry() -> SampleEntry {
    SampleEntry::Enca(EncaBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: 2,
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(48_000, 0),
        },
        sinf_box: SinfBox {
            frma_box: FrmaBox {
                data_format: *b"mp4a",
            },
            schm_box: Some(SchmBox {
                scheme_type: *b"cenc",
                // 保護スキームのバージョンは任意値
                scheme_version: 0x0001_0000,
                scheme_uri: None,
            }),
            schi_box: Some(SchiBox {
                tenc_box: Some(TencBox {
                    default_is_protected: 1,
                    default_per_sample_iv_size: 8,
                    default_kid: AUDIO_KID,
                    default_constant_iv: None,
                    default_crypt_byte_block: None,
                    default_skip_byte_block: None,
                }),
                unknown_boxes: vec![],
            }),
            unknown_boxes: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// init セグメントに格納する `pssh` の組を組み立てる
///
/// version 0（KID 一覧なし）と version 1（KID 一覧あり）を 1 つずつ含め、
/// 複数の `pssh` がそのままの順序で返ることを確認できるようにする。
fn create_pssh_boxes() -> Vec<PsshBox> {
    vec![
        PsshBox {
            version: 1,
            system_id: [0x01; 16],
            kids: vec![VIDEO_KID, AUDIO_KID],
            data: vec![0xAA, 0xBB, 0xCC],
        },
        PsshBox {
            version: 0,
            system_id: [0x02; 16],
            kids: Vec::new(),
            data: vec![0xDD],
        },
    ]
}

/// DRM サンプルエントリーを持つ映像サンプルを組み立てる
fn create_video_drm_sample(
    sample_entry: SampleEntry,
    data_offset: u64,
    data_size: usize,
) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size,
        // `create_encv_sample_entry()` の `tenc` の `default_per_sample_iv_size` は 16
        encryption_info: Some(SampleEncryptionInfo {
            iv: vec![0x33; 16],
            subsamples: Vec::new(),
        }),
    }
}

/// DRM サンプルエントリーを持つ音声サンプルを組み立てる
fn create_audio_drm_sample(
    sample_entry: SampleEntry,
    data_offset: u64,
    data_size: usize,
) -> Sample {
    Sample {
        track_kind: TrackKind::Audio,
        timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 960,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size,
        // `create_enca_sample_entry()` の `tenc` の `default_per_sample_iv_size` は 8
        encryption_info: Some(SampleEncryptionInfo {
            iv: vec![0x44; 8],
            subsamples: Vec::new(),
        }),
    }
}

/// `pssh` と `Encv` / `Enca` を demux できること
///
/// muxer が生成した init セグメントとメディアセグメントを `Fmp4SegmentDemuxer` に渡し、
/// 次の 3 点を確認する。
/// - init セグメントの処理前は `pssh_boxes()` が空であること
/// - init セグメントの処理後は `pssh_boxes()` が入力の `pssh` をそのまま返すこと
/// - 各トラックの最初のサンプルの `sample_entry` が `Encv` / `Enca` で、
///   `sinf` / `schm` / `schi` / `tenc` を型付きで読めること
#[test]
fn pssh_and_drm_sample_entries_are_available_from_init_and_media_segments() {
    let pssh_boxes = create_pssh_boxes();
    let options = SegmentMuxerOptions {
        pssh_boxes: pssh_boxes.clone(),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");

    let video_size = 16usize;
    let audio_size = 8usize;
    // サンプルはトラックごとに data_offset の昇順で連続して配置する
    let mut media_segment = muxer
        .create_media_segment_metadata(&[
            create_video_drm_sample(create_encv_sample_entry(320, 240), 0, video_size),
            create_audio_drm_sample(create_enca_sample_entry(), video_size as u64, audio_size),
        ])
        .expect("Encv / Enca のメディアセグメントの作成に失敗した");
    let payload = vec![0u8; video_size + audio_size];
    media_segment.extend_from_slice(&payload);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("DRM の init セグメントの作成に失敗した");

    let mut demuxer = Fmp4SegmentDemuxer::new();
    assert!(
        demuxer.pssh_boxes().is_empty(),
        "init セグメントを処理する前の pssh_boxes は空であるべき"
    );
    demuxer
        .handle_init_segment(&init_segment)
        .expect("DRM の init セグメントの処理に失敗した");
    assert_eq!(
        demuxer.pssh_boxes(),
        pssh_boxes.as_slice(),
        "pssh_boxes が init セグメントに含まれる pssh と一致しない"
    );
    assert_eq!(
        demuxer
            .tracks()
            .expect("init セグメント処理後の tracks の取得に失敗した")
            .len(),
        2,
        "映像・音声の 2 トラックが登録されるべき"
    );

    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("Encv / Enca のメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2, "2 トラック分のサンプルが返るべき");

    let mut has_video = false;
    let mut has_audio = false;
    for sample in &samples {
        let sample_entry = sample
            .sample_entry
            .expect("各トラックの最初のサンプルは sample_entry を持つ");
        match (sample.track.kind, sample_entry) {
            (TrackKind::Video, SampleEntry::Encv(encv_box)) => {
                has_video = true;
                assert_eq!(
                    encv_box.sinf_box.frma_box.data_format, *b"hev1",
                    "映像トラックの encv の frma が hev1 でない"
                );
                let schm_box = encv_box
                    .sinf_box
                    .schm_box
                    .as_ref()
                    .expect("encv の sinf に schm が含まれるべき");
                assert_eq!(
                    schm_box.scheme_type, *b"cbcs",
                    "encv の schm の scheme_type が cbcs でない"
                );
                let tenc_box = encv_box
                    .sinf_box
                    .schi_box
                    .as_ref()
                    .expect("encv の sinf に schi が含まれるべき")
                    .tenc_box
                    .as_ref()
                    .expect("encv の schi に tenc が含まれるべき");
                assert_eq!(
                    tenc_box.default_kid, VIDEO_KID,
                    "encv の tenc の default_KID が入力と一致しない"
                );
                assert_eq!(
                    tenc_box.default_crypt_byte_block,
                    Some(Uint::new(1)),
                    "encv の tenc の暗号化ブロック数が入力と一致しない"
                );
                assert_eq!(
                    tenc_box.default_skip_byte_block,
                    Some(Uint::new(9)),
                    "encv の tenc のスキップブロック数が入力と一致しない"
                );
            }
            (TrackKind::Audio, SampleEntry::Enca(enca_box)) => {
                has_audio = true;
                assert_eq!(
                    enca_box.sinf_box.frma_box.data_format, *b"mp4a",
                    "音声トラックの enca の frma が mp4a でない"
                );
                let schm_box = enca_box
                    .sinf_box
                    .schm_box
                    .as_ref()
                    .expect("enca の sinf に schm が含まれるべき");
                assert_eq!(
                    schm_box.scheme_type, *b"cenc",
                    "enca の schm の scheme_type が cenc でない"
                );
                let tenc_box = enca_box
                    .sinf_box
                    .schi_box
                    .as_ref()
                    .expect("enca の sinf に schi が含まれるべき")
                    .tenc_box
                    .as_ref()
                    .expect("enca の schi に tenc が含まれるべき");
                assert_eq!(
                    tenc_box.default_kid, AUDIO_KID,
                    "enca の tenc の default_KID が入力と一致しない"
                );
                assert_eq!(
                    tenc_box.default_crypt_byte_block, None,
                    "cenc の tenc はパターン暗号化パラメーターを持たないべき"
                );
            }
            (kind, entry) => panic!(
                "DRM のサンプルエントリーとして解釈できないトラック: kind={kind:?}, entry={entry:?}"
            ),
        }
    }
    assert!(
        has_video && has_audio,
        "映像トラックの Encv と音声トラックの Enca の両方を確認できていない"
    );
}

// ===== サンプル補助情報 (senc / saiz / saio) の読み取りテスト =====

/// 保護スキームと `tenc` の既定値を指定して `encv` のサンプルエントリーを組み立てる
fn create_encv_sample_entry_with_tenc(
    scheme_type: [u8; 4],
    default_is_protected: u8,
    default_per_sample_iv_size: u8,
    default_constant_iv: Option<Vec<u8>>,
) -> SampleEntry {
    SampleEntry::Encv(EncvBox {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width: 320,
            height: 240,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        sinf_box: SinfBox {
            frma_box: FrmaBox {
                data_format: *b"hev1",
            },
            schm_box: Some(SchmBox {
                scheme_type,
                scheme_version: 0x0001_0000,
                scheme_uri: None,
            }),
            schi_box: Some(SchiBox {
                tenc_box: Some(TencBox {
                    default_is_protected,
                    default_per_sample_iv_size,
                    default_kid: VIDEO_KID,
                    default_constant_iv,
                    default_crypt_byte_block: None,
                    default_skip_byte_block: None,
                }),
                unknown_boxes: vec![],
            }),
            unknown_boxes: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// 暗号化情報を指定できる映像サンプルを組み立てる
fn create_encrypted_video_sample(
    sample_entry: SampleEntry,
    encryption_info: Option<SampleEncryptionInfo>,
    data_offset: u64,
) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size: ENCRYPTED_SAMPLE_DATA_SIZE,
        encryption_info,
    }
}

/// 暗号化テスト用のサンプルデータサイズ
const ENCRYPTED_SAMPLE_DATA_SIZE: usize = 16;

/// 暗号化されたサンプルを含む初期化セグメントとメディアセグメントを組み立てる
///
/// メディアセグメントには `mdat` の payload も含める。
fn build_encrypted_init_and_media_segments(
    sample_entry: SampleEntry,
    encryption_infos: Vec<Option<SampleEncryptionInfo>>,
) -> (Vec<u8>, Vec<u8>) {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut samples = Vec::new();
    let mut payload = Vec::new();
    for (index, encryption_info) in encryption_infos.into_iter().enumerate() {
        let data_offset = (index * ENCRYPTED_SAMPLE_DATA_SIZE) as u64;
        samples.push(create_encrypted_video_sample(
            sample_entry.clone(),
            encryption_info,
            data_offset,
        ));
        // サンプルごとに区別できる payload を作る
        payload.extend_from_slice(&[(index as u8).wrapping_add(0x10); ENCRYPTED_SAMPLE_DATA_SIZE]);
    }
    let mut media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("暗号化サンプルのメディアセグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("暗号化サンプルの init セグメントの作成に失敗した");
    (init_segment, media_segment)
}

/// メディアセグメントの `moof` 直後にある `mdat` のヘッダーサイズと payload サイズを返す
///
/// テストで扱うセグメントは payload が小さいため、32 bit サイズの `mdat` だけを前提にする。
fn mdat_header_and_payload_size(segment: &[u8], moof_size: usize) -> (usize, usize) {
    let mdat_size_field = u32::from_be_bytes(
        segment[moof_size..moof_size + 4]
            .try_into()
            .expect("mdat のサイズフィールドは 4 バイト"),
    );
    assert_ne!(
        mdat_size_field, 1,
        "テストでは 32 bit サイズの mdat のみを扱う"
    );
    let mdat_size = usize::try_from(mdat_size_field).expect("mdat のサイズは usize に収まる");
    let header_size = BoxHeader::MIN_SIZE;
    assert!(
        mdat_size >= header_size,
        "mdat のサイズがヘッダーより小さい"
    );
    (header_size, mdat_size - header_size)
}

/// 最初の `traf` の `senc` から、サンプルごとの SAI のバイト列を取り出す
fn extract_senc_sai_bytes(segment: &[u8]) -> Vec<u8> {
    let (moof_box, _) = MoofBox::decode(segment).expect("moof のデコードに失敗した");
    let senc_box = moof_box.traf_boxes[0]
        .unknown_boxes
        .iter()
        .find(|unknown_box| unknown_box.box_type.as_bytes() == b"senc")
        .expect("traf に senc が見つからない");
    let senc_bytes = senc_box
        .encode_to_vec()
        .expect("senc のエンコードに失敗した");
    // senc のヘッダー 8 バイト + FullBox 4 バイト + sample_count 4 バイトの後ろが SAI
    senc_bytes[16..].to_vec()
}

/// `senc` を削除し、SAI を `mdat` の payload の後ろに移したメディアセグメントを返す
///
/// `saio` の offset は、移動後の最初の SAI の位置に書き換える。
/// `trun` の `data_offset` と `mdat` のサイズも、移動後のレイアウトに合わせて直す。
fn relocate_sai_after_mdat(segment: &[u8]) -> Vec<u8> {
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(segment).expect("moof のデコードに失敗した");
    let (mdat_header_size, mdat_payload_size) =
        mdat_header_and_payload_size(segment, old_moof_size);
    let mdat_start = old_moof_size;
    let payload_start = mdat_start + mdat_header_size;
    let payload = &segment[payload_start..payload_start + mdat_payload_size];
    let sai_bytes = extract_senc_sai_bytes(segment);

    {
        let traf_box = &mut moof_box.traf_boxes[0];
        traf_box
            .unknown_boxes
            .retain(|unknown_box| unknown_box.box_type.as_bytes() != b"senc");
    }

    // 1 パス目: `senc` を削除した後の moof のサイズを求める
    let new_moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    // `mdat` は削除したボックス分だけ前へ詰まるため、`trun` の data_offset をその分だけ減らす
    let shift = i32::try_from(old_moof_size - new_moof_size).expect("差分は i32 に収まる");
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        for trun_box in &mut traf_box.trun_boxes {
            if let Some(data_offset) = trun_box.data_offset {
                trun_box.data_offset = Some(data_offset - shift);
            }
        }
    }

    // SAI は mdat の payload の後ろに置くため、最初の SAI は payload の末尾から始まる
    let sai_offset = new_moof_size + mdat_header_size + mdat_payload_size;
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        let saio_index = traf_box
            .unknown_boxes
            .iter()
            .position(|unknown_box| unknown_box.box_type.as_bytes() == b"saio")
            .expect("traf に saio が見つからない");
        let (mut saio_box, _) = SaioBox::decode(
            &traf_box.unknown_boxes[saio_index]
                .encode_to_vec()
                .expect("saio のエンコードに失敗した"),
        )
        .expect("saio のデコードに失敗した");
        saio_box.offsets = vec![u64::try_from(sai_offset).expect("SAI の位置は u64 に収まる")];
        let encoded = saio_box
            .encode_to_vec()
            .expect("書き換えた saio のエンコードに失敗した");
        let (new_unknown_box, _) =
            UnknownBox::decode(&encoded).expect("書き換えた saio のデコードに失敗した");
        traf_box.unknown_boxes[saio_index] = new_unknown_box;
    }

    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        moof_bytes.len(),
        new_moof_size,
        "saio の offset を書き換えても moof のサイズは変わらない"
    );

    // mdat のサイズに SAI の分を足す
    let new_mdat_size = mdat_header_size + mdat_payload_size + sai_bytes.len();
    let mut mdat_header = segment[mdat_start..mdat_start + mdat_header_size].to_vec();
    mdat_header[..4].copy_from_slice(
        &u32::try_from(new_mdat_size)
            .expect("テスト用の mdat サイズは u32 に収まる")
            .to_be_bytes(),
    );

    let mut result = moof_bytes;
    result.extend_from_slice(&mdat_header);
    result.extend_from_slice(payload);
    result.extend_from_slice(&sai_bytes);
    result
}

/// 最初の `traf` から `senc` / `saiz` / `saio` をすべて削除したメディアセグメントを返す
///
/// `trun` の `data_offset` は、詰まった分だけ減らす。
fn remove_all_sample_aux_info(segment: &[u8]) -> Vec<u8> {
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(segment).expect("moof のデコードに失敗した");
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        traf_box.unknown_boxes.retain(|unknown_box| {
            let box_type = unknown_box.box_type.as_bytes();
            box_type != b"senc" && box_type != b"saiz" && box_type != b"saio"
        });
    }
    let new_moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    let shift = i32::try_from(old_moof_size - new_moof_size).expect("差分は i32 に収まる");
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        for trun_box in &mut traf_box.trun_boxes {
            if let Some(data_offset) = trun_box.data_offset {
                trun_box.data_offset = Some(data_offset - shift);
            }
        }
    }
    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        moof_bytes.len(),
        new_moof_size,
        "data_offset を書き換えても moof のサイズは変わらない"
    );
    let mut result = moof_bytes;
    result.extend_from_slice(&segment[old_moof_size..]);
    result
}

/// init セグメントの `tenc.default_is_protected` を書き換える
fn rewrite_init_tenc_default_is_protected(
    init_segment: &[u8],
    default_is_protected: u8,
) -> Vec<u8> {
    let (ftyp_box, ftyp_size) =
        FtypBox::decode(init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let (mut moov_box, _) = MoovBox::decode(&init_segment[ftyp_size..])
        .expect("init セグメントからの moov デコードに失敗した");
    let stsd_box = &mut moov_box.trak_boxes[0].mdia_box.minf_box.stbl_box.stsd_box;
    let SampleEntry::Encv(encv_box) = &mut stsd_box.entries[0] else {
        panic!("テスト対象のサンプルエントリーが Encv でない");
    };
    let tenc_box = encv_box
        .sinf_box
        .schi_box
        .as_mut()
        .expect("encv の sinf に schi がある")
        .tenc_box
        .as_mut()
        .expect("encv の schi に tenc がある");
    tenc_box.default_is_protected = default_is_protected;

    let mut result = ftyp_box
        .encode_to_vec()
        .expect("ftyp のエンコードに失敗した");
    result.extend_from_slice(
        &moov_box
            .encode_to_vec()
            .expect("moov のエンコードに失敗した"),
    );
    result
}

/// `senc` の version を書き換える
///
/// version は FullBox ヘッダーの先頭 1 バイトであり、サイズは変わらない。
fn rewrite_senc_version(segment: &[u8], version: u8) -> Vec<u8> {
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(segment).expect("moof のデコードに失敗した");
    let senc_box = moof_box.traf_boxes[0]
        .unknown_boxes
        .iter_mut()
        .find(|unknown_box| unknown_box.box_type.as_bytes() == b"senc")
        .expect("traf に senc が見つからない");
    senc_box.payload[0] = version;
    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        moof_bytes.len(),
        old_moof_size,
        "version の書き換えで moof のサイズは変わらない"
    );
    let mut result = moof_bytes;
    result.extend_from_slice(&segment[old_moof_size..]);
    result
}

/// 任意のペイロードから `UnknownBox` を組み立てる
fn build_unknown_box(box_type: [u8; 4], payload: Vec<u8>) -> UnknownBox {
    let box_type = BoxType::Normal(box_type);
    UnknownBox {
        box_type,
        box_size: BoxSize::with_payload_size(box_type, payload.len() as u64)
            .expect("テスト用のペイロード長は総サイズが u64 に収まる"),
        payload,
    }
}

/// 最初の `traf` の指定された種別のボックスを置き換えたメディアセグメントを返す
///
/// 置き換え後のボックスのサイズが変わっても、demux が SAI の解決の途中でエラーになる
/// テストにだけ使う。`trun` の `data_offset` は補正しない。
fn replace_traf_unknown_box(segment: &[u8], box_type: [u8; 4], new_box: UnknownBox) -> Vec<u8> {
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(segment).expect("moof のデコードに失敗した");
    let traf_box = &mut moof_box.traf_boxes[0];
    let index = traf_box
        .unknown_boxes
        .iter()
        .position(|unknown_box| unknown_box.box_type.as_bytes() == box_type)
        .unwrap_or_else(|| {
            panic!(
                "traf に {} が見つからない",
                String::from_utf8_lossy(&box_type)
            )
        });
    traf_box.unknown_boxes[index] = new_box;
    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    let mut result = moof_bytes;
    result.extend_from_slice(&segment[old_moof_size..]);
    result
}

/// サブサンプル暗号化のサンプル 2 個分の暗号化情報を返す
fn two_subsample_encryption_infos() -> Vec<Option<SampleEncryptionInfo>> {
    vec![
        Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: vec![Subsample {
                bytes_of_clear_data: 5,
                bytes_of_protected_data: 10,
            }],
        }),
        Some(SampleEncryptionInfo {
            iv: vec![0x22; 16],
            subsamples: Vec::new(),
        }),
    ]
}

/// `senc` から暗号化情報を復元できること
///
/// サブサンプルを持つサンプルと持たないサンプルが混在していても、入力どおりに復元される。
#[test]
fn demuxes_encryption_info_from_senc() {
    let encryption_infos = two_subsample_encryption_infos();
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        encryption_infos.clone(),
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("senc を含むメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2);
    for (sample, expected) in samples.iter().zip(encryption_infos.iter()) {
        assert_eq!(
            sample.encryption_info.as_ref(),
            expected.as_ref(),
            "senc から復元した暗号化情報が入力と一致しない"
        );
    }
}

/// `senc` が無く `saiz` / `saio` だけの入力でも暗号化情報を復元できること
///
/// SAI を `mdat` の payload の後ろに移し、`saio` の offset をそこへ向けたセグメントを使う。
#[test]
fn demuxes_encryption_info_from_saiz_and_saio() {
    let encryption_infos = two_subsample_encryption_infos();
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        encryption_infos.clone(),
    );
    let rewritten = relocate_sai_after_mdat(&media_segment);
    // 書き換えたセグメントに senc が含まれていないことを確認する
    let (moof_box, _) = MoofBox::decode(&rewritten).expect("書き換えた moof のデコードに失敗した");
    assert!(
        moof_box.traf_boxes[0]
            .unknown_boxes
            .iter()
            .all(|unknown_box| unknown_box.box_type.as_bytes() != b"senc"),
        "書き換えたセグメントに senc が残っている"
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("saiz / saio だけを含むメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2);
    for (sample, expected) in samples.iter().zip(encryption_infos.iter()) {
        assert_eq!(
            sample.encryption_info.as_ref(),
            expected.as_ref(),
            "saiz / saio から復元した暗号化情報が入力と一致しない"
        );
    }
}

/// `tenc.default_is_protected = 0` で SAI が無い入力では暗号化情報が `None` になること
#[test]
fn clear_track_without_sai_returns_no_encryption_info() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry_with_tenc(*b"cenc", 0, 16, None),
        vec![None],
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("暗号化されていないトラックのメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 1);
    assert!(
        samples[0].encryption_info.is_none(),
        "暗号化されていないトラックのサンプルに暗号化情報が付いている"
    );
}

/// `tenc.default_is_protected = 1` かつ `default_per_sample_iv_size = 0` で SAI が無い入力が、
/// 空の `iv` / `subsamples` の暗号化情報として復元されること
///
/// constant IV によるサンプル全体の暗号化を表す。ISO/IEC 23001-7:2023 8.2.3 は
/// counter mode (`cenc` / `cens`) で constant IV を使うことを禁じているため、
/// constant IV を使える CBC のスキーム (`cbc1`) で組み立てる。
#[test]
fn constant_iv_without_sai_returns_empty_encryption_info() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry_with_tenc(*b"cbc1", 1, 0, Some(vec![0x77; 8])),
        vec![Some(SampleEncryptionInfo {
            iv: Vec::new(),
            subsamples: Vec::new(),
        })],
    );
    let rewritten = remove_all_sample_aux_info(&media_segment);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("constant IV のメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 1);
    assert_eq!(
        samples[0].encryption_info,
        Some(SampleEncryptionInfo {
            iv: Vec::new(),
            subsamples: Vec::new(),
        }),
        "constant IV のサンプル全体暗号化として復元されていない"
    );
}

/// `tenc.default_is_protected = 1` かつ per-sample IV が必要なのに SAI が無い入力が
/// InvalidData になること
#[test]
fn missing_sai_with_per_sample_iv_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry_with_tenc(*b"cenc", 1, 8, None),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 8],
            subsamples: Vec::new(),
        })],
    );
    let rewritten = remove_all_sample_aux_info(&media_segment);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("SAI の無い per-sample IV のトラックを受け付けてしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("requires sample auxiliary information"),
                "SAI の有無の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `tenc.default_is_protected = 0` のトラックに SAI がある入力 (選択的暗号化) が
/// InvalidData になること
#[test]
fn selective_encryption_sai_with_clear_default_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    let init_segment = rewrite_init_tenc_default_is_protected(&init_segment, 0);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&media_segment)
        .expect_err("選択的暗号化の入力を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("selective encryption is not supported"),
                "選択的暗号化の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `grouping_type` が `seig` の `sbgp` / `sgpd` を持つ `traf` が Unsupported になること
#[test]
fn seig_sample_group_is_unsupported() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );

    // `sbgp` と `sgpd` はどちらも、version 0, flags 0 の直後に grouping_type が 4 バイトで並ぶ。
    // ここでは grouping_type 'seig', entry_count 0 のボックスを追加する
    for (box_type, description) in [(*b"sbgp", "sbgp"), (*b"sgpd", "sgpd")] {
        let mut payload = vec![0x00, 0x00, 0x00, 0x00];
        payload.extend_from_slice(b"seig");
        payload.extend_from_slice(&0u32.to_be_bytes());
        let rewritten = rewrite_media_segment_moof(&media_segment, move |moof_box| {
            assert_eq!(moof_box.traf_boxes.len(), 1, "テストは 1 traf を前提にする");
            moof_box.traf_boxes[0]
                .unknown_boxes
                .push(build_unknown_box(box_type, payload));
        });

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_segment)
            .expect("init セグメントの処理に失敗した");
        let err = demuxer
            .handle_media_segment(&rewritten)
            .expect_err("seig サンプルグループを持つ入力を解釈してしまった");
        match err {
            DemuxError::DecodeError(error) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::Unsupported,
                    "{description} のエラー種別が Unsupported ではない (reason={})",
                    error.reason,
                );
                assert!(
                    error.reason.contains("seig sample group"),
                    "{description} の seig の検査とは別のエラーである (reason={})",
                    error.reason,
                );
            }
            other => panic!("DecodeError を期待したが {other:?} だった"),
        }
    }
}

/// `senc` の version が 0 以外の入力が InvalidData になること
#[test]
fn senc_version_1_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    let rewritten = rewrite_senc_version(&media_segment, 1);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("version 1 の senc を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("Unsupported SencBox version"),
                "senc のバージョンの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `saiz` の `aux_info_type_parameter` が 0 以外の入力が InvalidData になること
#[test]
fn saiz_non_zero_aux_info_type_parameter_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    // version 0, flags 0x000001 (aux_info_type あり), aux_info_type 'cenc',
    // aux_info_type_parameter 1
    let mut payload = vec![0x00, 0x00, 0x00, 0x01];
    payload.extend_from_slice(b"cenc");
    payload.extend_from_slice(&1u32.to_be_bytes());
    payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    let rewritten = replace_traf_unknown_box(
        &media_segment,
        *b"saiz",
        build_unknown_box(*b"saiz", payload),
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("aux_info_type_parameter が 0 以外の saiz を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("aux_info_type_parameter must be 0"),
                "パラメーターの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `saio` の `aux_info_type_parameter` が 0 以外の入力が InvalidData になること
#[test]
fn saio_non_zero_aux_info_type_parameter_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    // version 0, flags 0x000001 (aux_info_type あり), aux_info_type 'cenc',
    // aux_info_type_parameter 1, entry_count 0
    let mut payload = vec![0x00, 0x00, 0x00, 0x01];
    payload.extend_from_slice(b"cenc");
    payload.extend_from_slice(&1u32.to_be_bytes());
    payload.extend_from_slice(&0u32.to_be_bytes());
    let rewritten = replace_traf_unknown_box(
        &media_segment,
        *b"saio",
        build_unknown_box(*b"saio", payload),
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("aux_info_type_parameter が 0 以外の saio を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("aux_info_type_parameter must be 0"),
                "パラメーターの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// 複数の暗号化トラックがあるセグメントでも、`traf` ごとの `saio` offset が正しく解決されること
///
/// 映像 (IV 16 バイト) と音声 (IV 8 バイト) の `traf` が並ぶセグメントを作り、
/// それぞれのサンプルに別々の暗号化情報が復元されることを確認する。
#[test]
fn demuxes_encryption_info_for_multiple_encrypted_tracks() {
    let video_encryption = SampleEncryptionInfo {
        iv: vec![0x11; 16],
        subsamples: vec![Subsample {
            bytes_of_clear_data: 5,
            bytes_of_protected_data: 10,
        }],
    };
    let audio_encryption = SampleEncryptionInfo {
        iv: vec![0x22; 8],
        subsamples: vec![Subsample {
            bytes_of_clear_data: 3,
            bytes_of_protected_data: 4,
        }],
    };
    let video_size = 16usize;
    let audio_size = 8usize;
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[
            create_encrypted_video_sample(
                create_encv_sample_entry(320, 240),
                Some(video_encryption.clone()),
                0,
            ),
            Sample {
                track_kind: TrackKind::Audio,
                timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("タイムスケールは非ゼロ"),
                sample_entry: Some(create_enca_sample_entry()),
                duration: 960,
                keyframe: true,
                composition_time_offset: None,
                data_offset: video_size as u64,
                data_size: audio_size,
                encryption_info: Some(audio_encryption.clone()),
            },
        ])
        .expect("複数トラックの暗号化サンプルのメディアセグメントの作成に失敗した");
    media_segment.extend_from_slice(&vec![0u8; video_size + audio_size]);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("暗号化サンプルの init セグメントの作成に失敗した");

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("複数トラックの暗号化サンプルの処理に失敗した");
    assert_eq!(samples.len(), 2);
    for sample in &samples {
        let expected = match sample.track.kind {
            TrackKind::Video => &video_encryption,
            TrackKind::Audio => &audio_encryption,
            TrackKind::Subtitle => panic!("字幕トラックは作っていない"),
        };
        assert_eq!(
            sample.encryption_info.as_ref(),
            Some(expected),
            "traf ごとの暗号化情報が正しく解決されていない (kind={:?})",
            sample.track.kind,
        );
    }
}

/// `senc` があっても、`saiz` / `saio` を持つ `traf` に `base_data_offset` も
/// `default_base_is_moof` も無い入力が InvalidData になること
///
/// 仕様上、`saiz` / `saio` の SAI の位置は `tfhd` の base data offset と同じ基準で解決するため、
/// どちらの基準も無い入力は不正になる。
#[test]
fn saiz_saio_without_base_data_offset_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(&media_segment).expect("moof のデコードに失敗した");
    let traf_box = &mut moof_box.traf_boxes[0];
    traf_box.tfhd_box.default_base_is_moof = false;
    assert_eq!(
        traf_box.tfhd_box.base_data_offset, None,
        "mux は base_data_offset を明示しない"
    );
    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        moof_bytes.len(),
        old_moof_size,
        "フラグの書き換えで moof のサイズは変わらない"
    );
    let mut rewritten = moof_bytes;
    rewritten.extend_from_slice(&media_segment[old_moof_size..]);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("base の無い saiz / saio を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("saiz/saio require base_data_offset or default_base_is_moof"),
                "base の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

// ===== CENC 以外の保護スキームと汎用 SAI の扱い =====

/// init セグメントの `encv` / `enca` 内の `schm.scheme_type` を書き換える
///
/// `schm` のペイロードは FullBox ヘッダー (4 バイト) の後に `scheme_type` (4 バイト) が
/// 並ぶ。ボックス種別の 4 バイトの直後から数えて 8 バイト目が `scheme_type` になる。
/// サイズは変わらないため、ほかのボックスの位置を補正する必要はない。
fn rewrite_init_scheme_type(init_segment: &[u8], scheme_type: [u8; 4]) -> Vec<u8> {
    let mut result = init_segment.to_vec();
    let position = result
        .windows(4)
        .position(|window| window == b"schm")
        .expect("init セグメントに schm が見つからない");
    // `schm` のボックス種別 (4 バイト) + FullBox ヘッダー (4 バイト) の後ろ
    let scheme_type_start = position + 8;
    result[scheme_type_start..scheme_type_start + 4].copy_from_slice(&scheme_type);
    result
}

/// `tenc` を持つが CENC 系以外の保護スキームのトラックが Unsupported になること
///
/// 平文として返すと、暗号化されたサンプルを復号せずに呼び出し元へ渡してしまう。
#[test]
fn non_cenc_protection_scheme_is_unsupported() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    // `EncvBox::encode` は CENC 系以外のスキームを書き出せないため、
    // エンコード済みの init セグメントの `schm.scheme_type` だけを書き換える
    let init_segment = rewrite_init_scheme_type(&init_segment, *b"piff");

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&media_segment)
        .expect_err("CENC 以外の保護スキームを持つトラックを平文として解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::Unsupported,
                "エラー種別が Unsupported ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("unsupported protection scheme"),
                "保護スキームの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// 保護されていないトラックの `saiz` / `saio` が、CENC 系以外の `aux_info_type` なら
/// 無視されること
///
/// 汎用 SAI は CENC 以外の用途でも使われるため、`aux_info_type` が CENC 系でなければ
/// エラーにせず、暗号化情報なしのサンプルとして返す。
#[test]
fn generic_sai_with_non_cenc_aux_info_type_is_ignored() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // 汎用 SAI を表す `saiz` / `saio`。`aux_info_type` は CENC 系ではない 'abcd' にする
    let saiz_box = SaizBox {
        aux_info_type: Some(*b"abcd"),
        aux_info_type_parameter: 0,
        default_sample_info_size: 0,
        sample_count: 1,
        sample_info_sizes: vec![0],
    };
    let saio_box = SaioBox {
        version: 0,
        aux_info_type: Some(*b"abcd"),
        aux_info_type_parameter: 0,
        offsets: vec![0],
    };
    let rewritten = rewrite_media_segment_moof(&media_segment, move |moof_box| {
        let traf_box = &mut moof_box.traf_boxes[0];
        traf_box.unknown_boxes.push(unknown_box_from_bytes(
            saiz_box
                .encode_to_vec()
                .expect("saiz のエンコードに失敗した"),
        ));
        traf_box.unknown_boxes.push(unknown_box_from_bytes(
            saio_box
                .encode_to_vec()
                .expect("saio のエンコードに失敗した"),
        ));
    });

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("汎用 SAI を持つ保護されていないトラックの処理に失敗した");
    assert_eq!(samples.len(), 1, "サンプルが 1 つ返る");
    assert!(
        samples[0].encryption_info.is_none(),
        "無視した汎用 SAI を暗号化情報として解釈してしまった"
    );
}

// ===== 巨大な sample_count を宣言する入力 =====

/// constant IV のトラックが巨大な `sample_count` を宣言していても、
/// サンプル数分の暗号化情報を確保せずに InvalidData になること
///
/// 暗号化情報をサンプルごとの `Vec` で確保する実装では、この小さな入力から
/// 数 GB のメモリを確保してしまう。constant IV は全サンプルで同じ暗号化情報を
/// 共有できるため、`mdat` の範囲検査でエラーになることを固定する。
#[test]
fn constant_iv_with_huge_sample_count_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry_with_tenc(*b"cbc1", 1, 0, Some(vec![0x77; 8])),
        vec![Some(SampleEncryptionInfo {
            iv: Vec::new(),
            subsamples: Vec::new(),
        })],
    );
    // constant IV では SAI が無くても暗号化情報を復元できるため、SAI をすべて削除する
    let no_sai_segment = remove_all_sample_aux_info(&media_segment);
    let rewritten = rewrite_media_segment_moof(&no_sai_segment, |moof_box| {
        let traf_box = &mut moof_box.traf_boxes[0];
        // サンプルサイズを `mdat` のサイズより大きくして、最初のサンプルの範囲検査でエラーにする
        traf_box.tfhd_box.default_sample_size = Some(1_000_000);
        let mut huge_trun_box = traf_box.trun_boxes[0].clone();
        huge_trun_box.samples = TrunSamples::CountOnly(16_777_216);
        // 合計 67,108,864 サンプルを宣言する小さな `moof` にする
        traf_box.trun_boxes = vec![huge_trun_box; 4];
    });

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("巨大な sample_count を宣言する入力を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("sample data range exceeds mdat boundary"),
                "サンプルデータの範囲検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `saiz` / `saio` が入力サイズに比例しない `sample_count` を宣言した入力を、
/// per-sample の SAI を確保せずに InvalidData にする上限があること
#[test]
fn per_sample_sai_with_huge_sample_count_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        vec![Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        })],
    );
    // `saiz` の `default_sample_info_size` を 1 にすると、`sample_info_sizes` を
    // 持たなくてもサンプル数分の SAI 長を宣言できる
    const HUGE_SAMPLE_COUNT: u32 = 16_777_216;
    let rewritten = rewrite_media_segment_moof(&media_segment, |moof_box| {
        let traf_box = &mut moof_box.traf_boxes[0];
        // `senc` を削除し、`saiz` / `saio` だけで SAI を指す構成にする
        traf_box
            .unknown_boxes
            .retain(|unknown_box| unknown_box.box_type.as_bytes() != b"senc");
        let mut huge_trun_box = traf_box.trun_boxes[0].clone();
        huge_trun_box.samples = TrunSamples::CountOnly(HUGE_SAMPLE_COUNT);
        traf_box.trun_boxes = vec![huge_trun_box];
        let saiz_index = traf_box
            .unknown_boxes
            .iter()
            .position(|unknown_box| unknown_box.box_type.as_bytes() == b"saiz")
            .expect("traf に saiz が見つからない");
        let saiz_box = SaizBox {
            aux_info_type: None,
            aux_info_type_parameter: 0,
            default_sample_info_size: 1,
            sample_count: HUGE_SAMPLE_COUNT,
            sample_info_sizes: Vec::new(),
        };
        traf_box.unknown_boxes[saiz_index] = unknown_box_from_bytes(
            saiz_box
                .encode_to_vec()
                .expect("saiz のエンコードに失敗した"),
        );
    });

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("入力サイズに比例しない sample_count を宣言する SAI を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("sample auxiliary information sample count"),
                "サンプル数の上限の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

// ===== 複数の trun と saiz.default_sample_info_size =====

/// `saiz` / `saio` を書き換えるときに `saio` へ書く offset の決め方
#[derive(Debug, Clone, Copy)]
enum SaioOffsetMode {
    /// 全サンプルの SAI が連続していることを示す 1 個の offset
    Single,

    /// `trun` ごとの最初の SAI を指す offset
    PerTrun,

    /// `trun` の数と一致しない個数の offset (エラーパスのテスト用)
    InvalidCount(usize),
}

/// `trun` に含まれるサンプル数を返す
fn trun_sample_count(trun_box: &TrunBox) -> usize {
    match &trun_box.samples {
        TrunSamples::CountOnly(sample_count) => *sample_count as usize,
        TrunSamples::PerSample(samples) => samples.len(),
    }
}

/// エンコード済みのボックスバイト列を [`UnknownBox`] に変換する
fn unknown_box_from_bytes(bytes: Vec<u8>) -> UnknownBox {
    let (unknown_box, size) =
        UnknownBox::decode(&bytes).expect("エンコード済みボックスのデコードに失敗した");
    assert_eq!(
        size,
        bytes.len(),
        "エンコード結果が複数のボックスを含んでいる"
    );
    unknown_box
}

/// 最初の `traf` の `saio` の offset を書き換える
///
/// offset の個数を変えないため、書き換えても `moof` のサイズは変わらない。
fn replace_saio_offsets(moof_box: &mut MoofBox, offsets: &[usize]) {
    let traf_box = &mut moof_box.traf_boxes[0];
    let index = traf_box
        .unknown_boxes
        .iter()
        .position(|unknown_box| unknown_box.box_type.as_bytes() == b"saio")
        .expect("traf に saio が見つからない");
    let (mut saio_box, _) = SaioBox::decode(
        &traf_box.unknown_boxes[index]
            .encode_to_vec()
            .expect("saio のエンコードに失敗した"),
    )
    .expect("saio のデコードに失敗した");
    saio_box.offsets = offsets
        .iter()
        .map(|offset| u64::try_from(*offset).expect("テスト用の offset は u64 に収まる"))
        .collect();
    traf_box.unknown_boxes[index] = unknown_box_from_bytes(
        saio_box
            .encode_to_vec()
            .expect("saio のエンコードに失敗した"),
    );
}

/// `senc` を削除して SAI を `mdat` の後ろへ移し、`trun` と `saiz` / `saio` を
/// 指定どおりに書き換えたメディアセグメントを返す
///
/// - `split_truns`: `true` なら各サンプルを別々の `trun` に分ける
/// - `default_sample_info_size`: `saiz` の既定 SAI 長。0 なら `sample_info_sizes` を並べる
/// - `sample_info_sizes`: `default_sample_info_size` が 0 の場合の、サンプル順の SAI 長
/// - `saio_offset_mode`: `saio` に書く offset の決め方
///
/// SAI の内容は `senc` のサンプルごとの領域をそのまま使う。
fn rewrite_sai_layout(
    segment: &[u8],
    split_truns: bool,
    default_sample_info_size: u8,
    sample_info_sizes: &[u8],
    saio_offset_mode: SaioOffsetMode,
) -> Vec<u8> {
    let (mut moof_box, old_moof_size) =
        MoofBox::decode(segment).expect("moof のデコードに失敗した");
    let (mdat_header_size, mdat_payload_size) =
        mdat_header_and_payload_size(segment, old_moof_size);
    let payload_start = old_moof_size + mdat_header_size;
    let payload = &segment[payload_start..payload_start + mdat_payload_size];
    let sai_bytes = extract_senc_sai_bytes(segment);

    // `trun` をサンプルごとに分ける。`data_offset` は先頭の `trun` にだけ残し、
    // それ以外は直前の run のデータ末尾から続くものとして省略する
    if split_truns {
        let traf_box = &mut moof_box.traf_boxes[0];
        assert_eq!(
            traf_box.trun_boxes.len(),
            1,
            "テストは分割前の trun が 1 個であることを前提にする"
        );
        let trun_box = traf_box.trun_boxes[0].clone();
        let TrunSamples::PerSample(rows) = &trun_box.samples else {
            panic!("テストはサンプル行を持つ trun を前提にする");
        };
        let mut split_trun_boxes = Vec::new();
        for (index, row) in rows.iter().enumerate() {
            let mut split_trun_box = trun_box.clone();
            split_trun_box.samples = TrunSamples::PerSample(vec![row.clone()]);
            split_trun_box.data_offset = if index == 0 {
                trun_box.data_offset
            } else {
                None
            };
            split_trun_boxes.push(split_trun_box);
        }
        traf_box.trun_boxes = split_trun_boxes;
    }

    let total_sample_count = moof_box.traf_boxes[0]
        .trun_boxes
        .iter()
        .map(trun_sample_count)
        .sum::<usize>();
    assert!(
        total_sample_count > 0,
        "テストは 1 件以上のサンプルを前提にする"
    );
    let sample_sai_sizes: Vec<usize> = if default_sample_info_size == 0 {
        sample_info_sizes
            .iter()
            .map(|size| usize::from(*size))
            .collect()
    } else {
        vec![usize::from(default_sample_info_size); total_sample_count]
    };
    assert_eq!(
        sample_sai_sizes.len(),
        total_sample_count,
        "sample_info_sizes の個数がサンプル数と一致しない"
    );
    let total_sai_size = sample_sai_sizes.iter().sum::<usize>();
    assert_eq!(
        total_sai_size,
        sai_bytes.len(),
        "SAI の長さが sample_info_sizes の合計と一致しない"
    );

    // `senc` / `saiz` / `saio` を削除し、書き換えた `saiz` / `saio` を末尾に追加する。
    // `saio` の offset は、moof のサイズが確定した後に書き換える
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        traf_box.unknown_boxes.retain(|unknown_box| {
            let box_type = unknown_box.box_type.as_bytes();
            box_type != b"senc" && box_type != b"saiz" && box_type != b"saio"
        });
        let sample_count =
            u32::try_from(total_sample_count).expect("テスト用のサンプル数は u32 に収まる");
        let saiz_box = SaizBox {
            aux_info_type: None,
            aux_info_type_parameter: 0,
            default_sample_info_size,
            sample_count,
            sample_info_sizes: if default_sample_info_size == 0 {
                sample_info_sizes.to_vec()
            } else {
                Vec::new()
            },
        };
        traf_box.unknown_boxes.push(unknown_box_from_bytes(
            saiz_box
                .encode_to_vec()
                .expect("saiz のエンコードに失敗した"),
        ));
        let saio_offset_count = match saio_offset_mode {
            SaioOffsetMode::Single => 1,
            SaioOffsetMode::PerTrun => traf_box.trun_boxes.len(),
            SaioOffsetMode::InvalidCount(count) => count,
        };
        let saio_box = SaioBox {
            version: 0,
            aux_info_type: None,
            aux_info_type_parameter: 0,
            offsets: vec![0; saio_offset_count],
        };
        traf_box.unknown_boxes.push(unknown_box_from_bytes(
            saio_box
                .encode_to_vec()
                .expect("saio のエンコードに失敗した"),
        ));
    }

    // 仮の値でエンコードして moof のサイズを確定する。`saio` の offset と `trun` の
    // `data_offset` は固定長なので、値を書き換えてもサイズは変わらない
    let new_moof_size = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した")
        .len();
    let sample_data_start = new_moof_size + mdat_header_size;
    let sai_start = sample_data_start + mdat_payload_size;

    // サンプルごとの SAI の開始位置を求める
    let mut sample_sai_starts = Vec::new();
    let mut cursor = sai_start;
    for size in &sample_sai_sizes {
        sample_sai_starts.push(cursor);
        cursor += *size;
    }
    // `trun` ごとの最初のサンプルの SAI 開始位置を求める
    let mut trun_sai_starts = Vec::new();
    {
        let mut sample_index = 0;
        for trun_box in &moof_box.traf_boxes[0].trun_boxes {
            trun_sai_starts.push(sample_sai_starts[sample_index]);
            sample_index += trun_sample_count(trun_box);
        }
    }
    let saio_offsets: Vec<usize> = match saio_offset_mode {
        SaioOffsetMode::Single => vec![sample_sai_starts[0]],
        SaioOffsetMode::PerTrun => trun_sai_starts,
        // 個数が不正な場合は offset を使わないため、先頭から順に埋める
        SaioOffsetMode::InvalidCount(count) => (0..count)
            .map(|index| sample_sai_starts[index % sample_sai_starts.len()])
            .collect(),
    };
    replace_saio_offsets(&mut moof_box, &saio_offsets);

    // `trun` の `data_offset` を新しい `mdat` の位置に合わせる
    {
        let traf_box = &mut moof_box.traf_boxes[0];
        for (index, trun_box) in traf_box.trun_boxes.iter_mut().enumerate() {
            trun_box.data_offset = (index == 0).then_some(
                i32::try_from(sample_data_start).expect("テスト用の data_offset は i32 に収まる"),
            );
        }
    }

    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("moof のエンコードに失敗した");
    assert_eq!(
        moof_bytes.len(),
        new_moof_size,
        "offset を書き換えても moof のサイズは変わらない"
    );

    let new_mdat_size = mdat_header_size + mdat_payload_size + total_sai_size;
    let mut mdat_header = segment[old_moof_size..payload_start].to_vec();
    mdat_header[..4].copy_from_slice(
        &u32::try_from(new_mdat_size)
            .expect("テスト用の mdat サイズは u32 に収まる")
            .to_be_bytes(),
    );

    let mut result = moof_bytes;
    result.extend_from_slice(&mdat_header);
    result.extend_from_slice(payload);
    result.extend_from_slice(&sai_bytes);
    result
}

/// サブサンプル暗号化のサンプル 2 個分の SAI 長を返す
///
/// 1 個目は IV 16 バイト + `subsample_count` 2 バイト + サブサンプル 1 個 (6 バイト)、
/// 2 個目は IV 16 バイト + `subsample_count` 2 バイトになる。
/// `senc` の flags は全サンプル共通なので、サブサンプルを持たないサンプルにも
/// `subsample_count` が入る。
fn subsample_sai_sizes() -> [u8; 2] {
    [24, 18]
}

/// `trun` が 2 個あり `saio` の offset も 2 個ある場合に、`trun` ごとに SAI が
/// 解決されること
#[test]
fn demuxes_encryption_info_from_two_truns_with_per_trun_saio_offsets() {
    let encryption_infos = two_subsample_encryption_infos();
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        encryption_infos.clone(),
    );
    let rewritten = rewrite_sai_layout(
        &media_segment,
        true,
        0,
        &subsample_sai_sizes(),
        SaioOffsetMode::PerTrun,
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("2 個の trun と 2 個の saio offset を持つセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2, "2 サンプルが返る");
    for (sample, expected) in samples.iter().zip(encryption_infos.iter()) {
        assert_eq!(
            sample.encryption_info.as_ref(),
            expected.as_ref(),
            "trun ごとの saio offset から復元した暗号化情報が入力と一致しない"
        );
    }
}

/// `trun` が 2 個あり `saio` の offset が 1 個だけの場合に、全サンプルの SAI が
/// 連続しているものとして解決されること
#[test]
fn demuxes_encryption_info_from_two_truns_with_single_saio_offset() {
    let encryption_infos = two_subsample_encryption_infos();
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        encryption_infos.clone(),
    );
    let rewritten = rewrite_sai_layout(
        &media_segment,
        true,
        0,
        &subsample_sai_sizes(),
        SaioOffsetMode::Single,
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("2 個の trun と 1 個の saio offset を持つセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2, "2 サンプルが返る");
    for (sample, expected) in samples.iter().zip(encryption_infos.iter()) {
        assert_eq!(
            sample.encryption_info.as_ref(),
            expected.as_ref(),
            "連続した SAI から復元した暗号化情報が入力と一致しない"
        );
    }
}

/// `saio` の offset の個数が 1 でも `trun` の数でもない入力が InvalidData になること
#[test]
fn invalid_saio_offset_count_is_invalid_data() {
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        two_subsample_encryption_infos(),
    );
    let rewritten = rewrite_sai_layout(
        &media_segment,
        true,
        0,
        &subsample_sai_sizes(),
        SaioOffsetMode::InvalidCount(3),
    );

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let err = demuxer
        .handle_media_segment(&rewritten)
        .expect_err("saio の offset の個数が不正な入力を解釈してしまった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains(
                    "saio offset count 3 must be 1 or equal to the number of trun boxes (2)"
                ),
                "offset の個数の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
}

/// `saiz.default_sample_info_size` が 0 以外の場合に、サンプルごとの長さを
/// 持たなくても暗号化情報が復元されること
#[test]
fn demuxes_encryption_info_with_default_sample_info_size() {
    let encryption_infos = vec![
        Some(SampleEncryptionInfo {
            iv: vec![0x11; 16],
            subsamples: Vec::new(),
        }),
        Some(SampleEncryptionInfo {
            iv: vec![0x22; 16],
            subsamples: Vec::new(),
        }),
    ];
    let (init_segment, media_segment) = build_encrypted_init_and_media_segments(
        create_encv_sample_entry(320, 240),
        encryption_infos.clone(),
    );
    // IV 16 バイトを持つサンプルで、サブサンプル暗号化ではないため SAI 長は全サンプル 16 バイトになる
    let rewritten = rewrite_sai_layout(&media_segment, false, 16, &[], SaioOffsetMode::Single);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&rewritten)
        .expect("default_sample_info_size を持つセグメントの処理に失敗した");
    assert_eq!(samples.len(), 2, "2 サンプルが返る");
    for (sample, expected) in samples.iter().zip(encryption_infos.iter()) {
        assert_eq!(
            sample.encryption_info.as_ref(),
            expected.as_ref(),
            "default_sample_info_size から復元した暗号化情報が入力と一致しない"
        );
    }
}

// ===== styp (セグメント種別ボックス) のテスト =====

/// テスト用の `styp` を組み立てる
///
/// `major_brand` を変えた値を複数作り、どの `styp` が採用されたかを区別できるようにする。
fn create_styp(major_brand: [u8; 4]) -> StypBox {
    StypBox {
        major_brand: Brand::new(major_brand),
        minor_version: 0,
        compatible_brands: vec![Brand::new(major_brand)],
    }
}

/// メディアセグメントの `styp` の指定方法
///
/// `SegmentMuxerOptions::styp` と `SegmentMuxerOptions::cmaf` は同時に指定できないため、
/// どちらか一方だけを指定できるようにする。
enum StypSource<'a> {
    /// `styp` ボックスをそのまま指定する
    Explicit(&'a StypBox),

    /// CMAF の構造ブランドとして muxer に `styp` を組み立てさせる
    Cmaf,
}

/// `styp` を持たせたメディアセグメントを作り、init セグメントとともに返す
///
/// `styp` の指定方法は `source` で切り替える。
fn build_segments_with_styp(source: StypSource<'_>) -> (Vec<u8>, Vec<u8>) {
    let payload = [0u8; 16];
    let options = match source {
        StypSource::Explicit(styp) => SegmentMuxerOptions {
            styp: Some(styp.clone()),
            ..Default::default()
        },
        StypSource::Cmaf => SegmentMuxerOptions {
            cmaf: Some(CmafTrackFileBrand::Cmfc),
            ..Default::default()
        },
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[create_video_sample(payload.len())])
        .expect("styp 付き media セグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    (init_segment, media_segment)
}

/// `styp` を指定したメディアセグメントを処理すると `styp()` からその値が取得できること
///
/// init セグメントの処理前とメディアセグメントの処理前は `None` になることも確認する。
#[test]
fn styp_value_is_available_after_media_segment() {
    let styp = create_styp(*b"msdh");
    let (init_segment, media_segment) = build_segments_with_styp(StypSource::Explicit(&styp));

    let mut demuxer = Fmp4SegmentDemuxer::new();
    assert!(
        demuxer.styp().is_none(),
        "init セグメントの処理前の styp() は None であるべき"
    );
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    assert!(
        demuxer.styp().is_none(),
        "メディアセグメントの処理前の styp() は None であるべき"
    );

    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("styp 付きメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 1, "サンプルが 1 つ返る");
    assert_eq!(
        demuxer.styp(),
        Some(&styp),
        "処理したメディアセグメントの先頭の styp が取得できない"
    );
}

/// CMAF 指定で生成したメディアセグメントを処理すると、`styp()` が
/// `major_brand = cmfs` の `StypBox` を返すこと
///
/// `styp` は muxer がセグメント生成時に組み立てるため、muxer と demuxer の間で
/// `major_brand` と互換 brand が往復することも合わせて確認する。
#[test]
fn cmaf_styp_value_is_available_after_media_segment() {
    let (init_segment, media_segment) = build_segments_with_styp(StypSource::Cmaf);

    // init セグメントの `ftyp` をデコードし、`styp` の期待値を組み立てる。
    // `styp` の互換 brand は、`ftyp` の互換 brand に `major_brand` の `cmfs` を加えた集合になる
    let (ftyp_box, _) =
        FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let mut expected_compatible_brands = vec![Brand::CMFS];
    expected_compatible_brands.extend_from_slice(&ftyp_box.compatible_brands);
    let expected_styp = StypBox {
        major_brand: Brand::CMFS,
        minor_version: 0,
        compatible_brands: expected_compatible_brands,
    };

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("CMAF 指定のメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 1, "サンプルが 1 つ返る");
    assert_eq!(
        demuxer.styp(),
        Some(&expected_styp),
        "CMAF 指定で生成したメディアセグメントの styp が取得できない"
    );
}

/// `styp` を持たないメディアセグメントを処理すると `styp()` が `None` になること
///
/// 直前の成功したメディアセグメントの値をそのまま残さず、
/// `styp` を持たないセグメントを処理した後は `None` に戻ることを確認する。
#[test]
fn styp_is_none_when_segment_has_no_styp() {
    let styp = create_styp(*b"msdh");
    let (init_segment, media_segment_with_styp) =
        build_segments_with_styp(StypSource::Explicit(&styp));
    let (_, media_segment_without_styp) = build_init_and_media_segments();

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");

    demuxer
        .handle_media_segment(&media_segment_with_styp)
        .expect("styp 付きメディアセグメントの処理に失敗した");
    assert_eq!(
        demuxer.styp(),
        Some(&styp),
        "styp 付きメディアセグメントの styp が取得できない"
    );

    demuxer
        .handle_media_segment(&media_segment_without_styp)
        .expect("styp なしメディアセグメントの処理に失敗した");
    assert!(
        demuxer.styp().is_none(),
        "styp を持たないメディアセグメントを処理した後の styp() は None であるべき"
    );
}

/// 先頭以外の `styp` を無視すること
///
/// 先頭の `styp` だけを型付きで読むため、2 個目の `styp` は採用しない。
#[test]
fn second_styp_is_ignored() {
    let first_styp = create_styp(*b"msdh");
    let second_styp = create_styp(*b"cmff");
    let (init_segment, media_segment) = build_init_and_media_segments();

    // 先頭に 2 個の `styp` を置く。どちらも構文的には正しい `styp` である
    let mut data = first_styp
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
    data.extend_from_slice(
        &second_styp
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない"),
    );
    data.extend_from_slice(&media_segment);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("2 個の styp を持つメディアセグメントの処理に失敗した");
    assert_eq!(samples.len(), 1, "サンプルが 1 つ返る");
    assert_eq!(
        demuxer.styp(),
        Some(&first_styp),
        "先頭の styp ではなく 2 個目の styp が採用された"
    );
}

/// 壊れた `styp` を読み飛ばし、メディアセグメントの処理をエラーにしないこと
///
/// `compatible_brands` に 4 バイト未満の端数がある `styp` はデコードできないが、
/// 読み飛ばす対象のボックスとして扱い、`styp()` には反映しない。
#[test]
fn invalid_styp_is_skipped() {
    let (init_segment, media_segment) = build_init_and_media_segments();

    // size=19 + 種別 `styp` + major_brand + minor_version + 3 バイトの端数
    let mut invalid_styp = Vec::new();
    invalid_styp.extend_from_slice(&19u32.to_be_bytes());
    invalid_styp.extend_from_slice(b"styp");
    invalid_styp.extend_from_slice(b"msdh");
    invalid_styp.extend_from_slice(&0u32.to_be_bytes());
    invalid_styp.extend_from_slice(b"msx");

    let mut data = invalid_styp;
    data.extend_from_slice(&media_segment);

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let samples = demuxer
        .handle_media_segment(&data)
        .expect("壊れた styp を読み飛ばせずにエラーになった");
    assert_eq!(samples.len(), 1, "サンプルが 1 つ返る");
    assert!(
        demuxer.styp().is_none(),
        "デコードに失敗した壊れた styp を保持してしまった"
    );
}

/// `styp` を保持した後にメディアセグメントのデコードが失敗しても `styp()` が変わらないこと
///
/// 先頭の `styp` は保持するが、そのセグメントの後半の検証でエラーになった場合に、
/// 直前の成功した呼び出しで保持した `styp` を書き換えないことを確認する。
#[test]
fn styp_is_unchanged_when_media_segment_decoding_fails() {
    let kept_styp = create_styp(*b"msdh");
    let rejected_styp = create_styp(*b"cmff");
    let (init_segment, media_segment) = build_segments_with_styp(StypSource::Explicit(&kept_styp));

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    demuxer
        .handle_media_segment(&media_segment)
        .expect("styp 付きメディアセグメントの処理に失敗した");
    assert_eq!(
        demuxer.styp(),
        Some(&kept_styp),
        "最初のメディアセグメントの styp が取得できない"
    );

    // 正常なセグメントのサンプルサイズを `mdat` より大きくして、
    // `styp` を読んだ後のサンプルデータ範囲の検査でエラーにする
    let (_, invalid_segment) = build_init_and_media_segments();
    let invalid_segment = rewrite_media_segment_moof(&invalid_segment, |moof_box| {
        let trun_box = &mut moof_box.traf_boxes[0].trun_boxes[0];
        let TrunSamples::PerSample(samples) = &mut trun_box.samples else {
            panic!("muxer の trun はサンプル行を持つ");
        };
        samples[0].size = Some(1_000_000);
    });
    let mut data = rejected_styp
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
    data.extend_from_slice(&invalid_segment);

    let err = demuxer
        .handle_media_segment(&data)
        .expect_err("サンプルデータが mdat を超える入力がエラーにならなかった");
    match err {
        DemuxError::DecodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("sample data range exceeds mdat boundary"),
                "サンプルデータの範囲検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("DecodeError を期待したが {other:?} だった"),
    }
    assert_eq!(
        demuxer.styp(),
        Some(&kept_styp),
        "エラーを返した呼び出しで styp() の値が変わった"
    );
}
