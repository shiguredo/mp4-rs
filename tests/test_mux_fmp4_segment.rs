//! `Fmp4SegmentMuxer` の主要エラーパス（公開 API 契約）の単体テスト
//!
//! 対象は次の 3 バリアント:
//! - `MuxError::EmptyTracks`（トラック未観測での `init_segment_bytes`）
//! - `MuxError::EmptySamples`（空サンプル列での `create_media_segment_metadata`）
//! - `MuxError::MixedSampleEntries`（同一セグメント・同一トラックでの sample entry 混在）
//!
//! 意図的なエラーパスは固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_segment_mux_demux.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind, Uint,
    boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields},
    mux::{Fmp4SegmentMuxer, MuxError, Sample},
};

const VIDEO_TIMESCALE: u32 = 90_000;

/// 指定解像度の `SampleEntry::Avc1` を組み立てる
///
/// `MixedSampleEntries` の検証では、同一トラック内で異なる sample entry が
/// 必要になるため、幅を引数で変えられるようにしている。
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

/// 映像トラック用の `Sample` を 1 件組み立てる
fn video_sample(sample_entry: SampleEntry, data_offset: u64, data_size: usize) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロである"),
        sample_entry: Some(sample_entry),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size,
    }
}

/// サンプル未投入の muxer で `init_segment_bytes` すると `EmptyTracks` になること
#[test]
fn empty_tracks_on_init_segment_bytes() {
    let muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let result = muxer.init_segment_bytes();
    assert!(
        matches!(result, Err(MuxError::EmptyTracks)),
        "EmptyTracks を期待したが {:?} だった",
        result
    );
}

/// 空のサンプル列で `create_media_segment_metadata` すると `EmptySamples` になること
///
/// `create_media_segment_metadata` は内部で `build_media_segment_bytes` を経由し、
/// そこで空スライス検査に到達する。`create_media_segment_metadata_with_sidx` は
/// 自身の入口に独立した空検査を持つが、それは本テストの対象外。
#[test]
fn empty_samples_on_create_media_segment() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let result = muxer.create_media_segment_metadata(&[]);
    assert!(
        matches!(result, Err(MuxError::EmptySamples)),
        "EmptySamples を期待したが {:?} だった",
        result
    );
}

/// 同一セグメント・同一トラックで異なる sample entry を渡すと `MixedSampleEntries` になること
#[test]
fn mixed_sample_entries_in_segment() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

    // 幅だけ異なる 2 つの Avc1 エントリを用意し、同一 Video トラックへ連続配置する。
    // `data_offset` / `data_size` を連続に取っているのは、`resolve_segment_tracks` 内で
    // sample entry 一致比較（`MixedSampleEntries` の判定源）よりも後段にある
    // データ非連続検査（`sample data for the same track must be contiguous ...`）へ
    // 誤って横滑りしないようにするため。
    let first_entry = create_avc1_sample_entry(320, 240);
    let second_entry = create_avc1_sample_entry(640, 480);
    let first_size = 16usize;
    let second_size = 32usize;
    let samples = [
        video_sample(first_entry, 0, first_size),
        video_sample(second_entry, first_size as u64, second_size),
    ];

    let result = muxer.create_media_segment_metadata(&samples);
    assert!(
        matches!(
            result,
            Err(MuxError::MixedSampleEntries {
                track_kind: TrackKind::Video
            })
        ),
        "MixedSampleEntries(Video) を期待したが {:?} だった",
        result
    );
}
