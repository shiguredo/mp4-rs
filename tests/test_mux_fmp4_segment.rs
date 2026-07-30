//! `Fmp4SegmentMuxer` の公開 API 契約の単体テスト
//!
//! 対象:
//! - 主要エラーパス（`EmptyTracks` / `EmptySamples` / `MixedSampleEntries`）
//! - `create_media_segment_metadata_with_sidx` の `earliest_presentation_time` が
//!   `composition_time_offset` を反映すること（固定入力で PTS 最小値を検証する）
//!
//! 意図的なエラーパスと境界値は固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_segment_mux_demux.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    Decode, TrackKind, Uint,
    boxes::{Avc1Box, AvccBox, SampleEntry, SidxBox, VisualSampleEntryFields},
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
    video_sample_with_timing(sample_entry, 3000, None, data_offset, data_size)
}

/// duration / CTO を指定できる映像サンプルを組み立てる
fn video_sample_with_timing(
    sample_entry: SampleEntry,
    duration: u32,
    composition_time_offset: Option<i64>,
    data_offset: u64,
    data_size: usize,
) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration,
        keyframe: true,
        composition_time_offset,
        data_offset,
        data_size,
    }
}

/// セグメント先頭メタデータから `SidxBox` をデコードする
fn decode_sidx(segment: &[u8]) -> SidxBox {
    let (sidx, _) = SidxBox::decode(segment).expect("sidx のデコードに失敗した");
    sidx
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
/// そこの空スライス検査に到達する経路を検証する。
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

/// 空のサンプル列で `create_media_segment_metadata_with_sidx` すると `EmptySamples` になること
///
/// `create_media_segment_metadata_with_sidx` は自身の入口に独立した空スライス検査を持ち、
/// そこで早期リターンする経路を検証する。内側の `build_media_segment_bytes` にある
/// 同名検査に依存せずに、`samples[0]` へのアクセス手前で防御されていることを担保する。
#[test]
fn empty_samples_on_create_media_segment_with_sidx() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let result = muxer.create_media_segment_metadata_with_sidx(&[]);
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

/// CTO が全て `None` のとき、sidx の EPT はセグメント先頭の累積 DTS と一致すること
///
/// 第 1 セグメント（DTS=0）と第 2 セグメント（先行 duration 累積後）の両方を確認する。
#[test]
fn sidx_ept_matches_decode_time_when_cto_is_none() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let first_duration = 3000u32;

    let first_segment = muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry.clone(),
            first_duration,
            None,
            0,
            16,
        )])
        .expect("第 1 セグメントの生成に失敗した");
    assert_eq!(
        decode_sidx(&first_segment).earliest_presentation_time,
        0,
        "第 1 セグメントの EPT は 0 であるべき"
    );

    let second_segment = muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry, 1000, None, 0, 16,
        )])
        .expect("第 2 セグメントの生成に失敗した");
    assert_eq!(
        decode_sidx(&second_segment).earliest_presentation_time,
        u64::from(first_duration),
        "第 2 セグメントの EPT は先行 duration の累積と一致するべき"
    );
}

/// PTS 最小値が先頭サンプル以外のとき、その値を EPT にすること
///
/// `decode_time=0`, `(dur=100, cto=80)`, `(dur=100, cto=-50)` → PTS `[80, 50]` → EPT `50`。
/// CTO を無視して DTS だけ使う実装だと 0 になり、先頭だけ見る実装だと 80 になる。
#[test]
fn sidx_ept_uses_minimum_pts_not_first_sample() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let first_size = 16usize;
    let samples = [
        video_sample_with_timing(entry.clone(), 100, Some(80), 0, first_size),
        video_sample_with_timing(entry, 100, Some(-50), first_size as u64, first_size),
    ];

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("セグメントの生成に失敗した");
    assert_eq!(
        decode_sidx(&segment).earliest_presentation_time,
        50,
        "2 サンプル目の PTS が最小なので EPT は 50 であるべき"
    );
}

/// 負 CTO により EPT がセグメント先頭 DTS より小さくなること
///
/// 第 1 セグメントで DTS を 1000 まで進め、第 2 セグメントで `CTO=-50` の 1 サンプルを渡す。
/// 真の EPT は `1000 - 50 = 950`。CTO を無視すると 1000 のままになる。
#[test]
fn sidx_ept_reflects_negative_cto_below_decode_time() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);

    muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry.clone(),
            1000,
            None,
            0,
            16,
        )])
        .expect("第 1 セグメントの生成に失敗した");

    let second_segment = muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry,
            100,
            Some(-50),
            0,
            16,
        )])
        .expect("第 2 セグメントの生成に失敗した");
    assert_eq!(
        decode_sidx(&second_segment).earliest_presentation_time,
        950,
        "負 CTO を反映した EPT は 950 であるべき"
    );
}

/// いずれかのサンプルの PTS が負になると `MuxError::Overflow` になること
///
/// `decode_time=0` で `CTO=-1` のサンプルは PTS=-1 となり、`u64` の EPT に収まらない。
#[test]
fn sidx_ept_rejects_negative_pts() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let result = muxer.create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
        entry,
        100,
        Some(-1),
        0,
        16,
    )]);
    assert!(
        matches!(result, Err(MuxError::Overflow)),
        "負 PTS では Overflow を期待したが {:?} だった",
        result
    );
}
