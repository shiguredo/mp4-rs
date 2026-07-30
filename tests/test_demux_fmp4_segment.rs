//! `Fmp4SegmentDemuxer` の `DemuxError::InvalidState` 経路の単体テスト
//!
//! 対象は次の 3 経路:
//! - 二重 `handle_init_segment`
//! - init 前の `tracks`
//! - init 前の `handle_media_segment`（正当な `moof` + `mdat` を渡した場合）
//!
//! `handle_media_segment` は構文解析が成功したあとに初めて `InvalidState` を返す。
//! 空・不正バイト列では `DecodeError` になるため、別の `Fmp4SegmentMuxer` で
//! 正当なセグメントを組み立ててから未初期化 demuxer に渡す。
//!
//! 意図的なエラーパスは固定入力で契約を検証するため、PBT ではなく単体テストとして置く。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind, Uint,
    boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields},
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

/// 正当な init セグメントとメディアセグメント（`moof` + `mdat` payload 付き）を組み立てる
///
/// demux 側の `InvalidState` 検証では、構文的に正しいバイト列が必要なため、
/// 別 muxer の公開 API だけで生成する。
fn build_init_and_media_segments() -> (Vec<u8>, Vec<u8>) {
    let sample_entry = create_avc1_sample_entry(320, 240);
    // payload 長は demux 経路の検証（`InvalidState` への到達）には無関係の任意値。
    // `mdat` header は payload サイズに応じて 8 / 16 バイトを選ぶため、
    // `u32::MAX` を超えない範囲であれば境界の選択に影響しない。
    let payload = [0u8; 16];
    let sample = Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset: 0,
        data_size: payload.len(),
    };

    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&[sample])
        .expect("media セグメントの作成に失敗した");
    media_segment.extend_from_slice(&payload);

    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    (init_segment, media_segment)
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
