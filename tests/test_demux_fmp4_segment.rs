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
//!
//! `moof` の前後にあるボックスを読み飛ばせる正常系は PBT で検証する。
//! 意図的なエラーパスは固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//!
//! 読み飛ばすトラックのデータ末尾の計算のように、`Fmp4SegmentMuxer` の出力を書き換えないと作れない
//! 正常系の入力も、`build_skipped_track_segment` で組み立てて単体テストとして置く。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    Decode, Encode, Error, ErrorKind, TrackKind, Uint,
    boxes::{
        Avc1Box, AvccBox, FtypBox, MoofBox, MoovBox, SampleEntry, SidxBox, VisualSampleEntryFields,
    },
    demux::{DemuxError, Fmp4SegmentDemuxer},
    mux::{Fmp4SegmentMuxer, Sample},
};

const VIDEO_TIMESCALE: u32 = 90_000;

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
    // ブランドは demuxer が解釈しないため任意の値でよいが、
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
/// `skipped_run_count` は読み飛ばすトラックの `traf` の `trun` の数である。1 より大きい場合は
/// サンプル 1 つずつの `trun` に分け、2 つ目以降の `data_offset` を省く（直前の `trun` の
/// データ末尾から始まる）。`mdat` の payload に読み飛ばすトラックのデータが `skipped_run_count` 個分入る
///
/// 戻り値は (init セグメント, メディアセグメント, 書き換えた `moof` のサイズ)
fn build_skipped_track_segment(
    tfhd_default_sample_size: Option<u32>,
    trex_default_sample_size: Option<u32>,
    skipped_run_count: usize,
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
    // 読み飛ばすトラックの `trun` を `skipped_run_count` 個に分ける。
    // 2 つ目以降は `data_offset` を省く（`moof` のサイズが確定してから最初の `trun` にだけ設定する）
    let base_trun_box = skipped_traf_box.trun_boxes[0].clone();
    skipped_traf_box.trun_boxes = (0..skipped_run_count)
        .map(|index| {
            let mut trun_box = base_trun_box.clone();
            trun_box.samples.truncate(1);
            trun_box.samples[0].size = None;
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
    let skipped_data_size = SKIPPED_SAMPLE_SIZE as usize * skipped_run_count;
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
/// `mdat` の payload の先頭に読み飛ばすトラックのデータがあり、その直後に対応しているトラックのサンプルがある
fn expected_sample_data_offset(moof_size: usize, skipped_run_count: usize) -> u64 {
    u64::try_from(moof_size + MDAT_HEADER_SIZE + SKIPPED_SAMPLE_SIZE as usize * skipped_run_count)
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
/// どちらからも決められない場合は `trex` の既定値が要る
#[test]
fn decode_error_skipped_track_without_trex() {
    let (init_segment, data, _moof_size) = build_skipped_track_segment(None, None, 1);

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
        panic!("DecodeError を期待したが {result:?} だった");
    };
    assert_eq!(
        error.kind,
        ErrorKind::InvalidData,
        "trex がない読み飛ばすトラックでは InvalidData を期待した"
    );
    assert_eq!(
        error.reason, "trex not found for skipped track_id=2",
        "trex がないことを示すエラー理由を期待した"
    );
    assert_eq!(
        format!("{demuxer:?}"),
        state_before_error,
        "エラーを返した後に内部状態が変わった"
    );
}

/// 読み飛ばすトラックのデータ末尾を `trex` の既定のサンプルサイズから計算すること
///
/// `default_base_is_moof = false` かつ `base_data_offset` なしの場合、2 番目以降の `traf` の基準位置は
/// 直前の `traf` のデータ末尾になる。読み飛ばすトラックのサンプルサイズが `trun` と `tfhd` の
/// どちらにもない場合は `trex` の `default_sample_size` を使う
#[test]
fn skipped_track_data_end_uses_trex_default_sample_size() {
    let (init_segment, data, moof_size) =
        build_skipped_track_segment(None, Some(SKIPPED_SAMPLE_SIZE), 1);

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
    let (init_segment, data, moof_size) =
        build_skipped_track_segment(Some(SKIPPED_SAMPLE_SIZE), None, 1);

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
    let (init_segment, data, moof_size) =
        build_skipped_track_segment(None, Some(SKIPPED_SAMPLE_SIZE), 2);

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
