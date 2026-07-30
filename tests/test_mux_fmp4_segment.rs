//! `Fmp4SegmentMuxer` の公開 API 契約の単体テスト
//!
//! 対象:
//! - 主要エラーパス（`EmptyTracks` / `EmptySamples` / `MixedSampleEntries`）
//! - `create_media_segment_metadata_with_sidx` の `earliest_presentation_time` の
//!   値・境界・`Overflow` 契約
//!   - CTO=None 時に旧挙動と等価であること（後方互換性の回帰防止）
//!   - 複数サンプルで PTS 最小値が採用されること
//!   - セグメント跨ぎで負 CTO により先頭 DTS を下回るケース
//!   - 負 PTS で `MuxError::Overflow` を返すこと
//!
//! 意図的なエラーパスと境界値は固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_segment_mux_demux.rs` が担う。

use std::num::{NonZeroU16, NonZeroU32};

use shiguredo_mp4::{
    Decode, FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry, SidxBox,
        VisualSampleEntryFields,
    },
    mux::{Fmp4SegmentMuxer, MuxError, Sample},
};

const VIDEO_TIMESCALE: u32 = 90_000;
const AUDIO_TIMESCALE: u32 = 48_000;

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

/// 最小限の Opus SampleEntry を組み立てる
///
/// マルチトラック混在テストで Audio サンプルに紐付けるためだけに使う。
/// 値は `pbt/tests/prop_container_boxes.rs` の `minimal_opus_box` と揃える。
fn create_opus_sample_entry() -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: NonZeroU16::new(1).expect("data_reference_index は非ゼロ"),
            channelcount: 2,
            samplesize: 16,
            samplerate: FixedPointNumber::new(48000, 0),
        },
        dops_box: DopsBox {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        },
        unknown_boxes: vec![],
    })
}

/// duration / CTO を指定できる音声サンプルを組み立てる
fn audio_sample_with_timing(
    sample_entry: SampleEntry,
    duration: u32,
    composition_time_offset: Option<i64>,
    data_offset: u64,
    data_size: usize,
) -> Sample {
    Sample {
        track_kind: TrackKind::Audio,
        timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("タイムスケールは非ゼロ"),
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

/// `mdat` ボックスサイズ計算が `u64` を超えるとき `MuxError::Overflow` になること
///
/// 先頭トラックは `data_offset = 0` から連続配置される必要がある。
/// `data_size` を `u64::MAX - 7` にすると `payload_end = u64::MAX - 7` となり、
/// `BoxHeader::MIN_SIZE (8) + payload` がオーバーフローする。
/// `build_moof` の `u32::try_from(data_size)` より前にサイズ計算があるため、
/// `data_size > u32::MAX` でもこの経路に到達できる。
///
/// 64-bit 専用: 32-bit の `usize` ではこの `data_size` を表現できない。
#[cfg(target_pointer_width = "64")]
#[test]
fn mdat_box_size_overflow_returns_overflow() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    // 8 + (u64::MAX - 7) が u64 を超える境界
    let data_size = usize::try_from(u64::MAX - 7).expect("64-bit では usize に収まる");
    let samples = [video_sample_with_timing(entry, 3000, None, 0, data_size)];

    let result = muxer.create_media_segment_metadata(&samples);
    assert!(
        matches!(result, Err(MuxError::Overflow)),
        "Overflow を期待したが {:?} だった",
        result
    );
}

/// 拡張サイズ（16 バイトヘッダー）再計算が `u64` を超えるとき `MuxError::Overflow` になること
///
/// `data_size = u64::MAX - 15` では `8 + payload` は成功して U64 分岐に入り、
/// `16 + payload` の再計算だけがオーバーフローする。
/// `mdat_box_size_overflow_returns_overflow` が踏まない第 2 系統の `checked_add` を固定する。
///
/// 64-bit 専用: 32-bit の `usize` ではこの `data_size` を表現できない。
#[cfg(target_pointer_width = "64")]
#[test]
fn mdat_extended_box_size_overflow_returns_overflow() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    // 8 + (u64::MAX - 15) は成功し、16 + (u64::MAX - 15) がオーバーフローする
    let data_size = usize::try_from(u64::MAX - 15).expect("64-bit では usize に収まる");
    let samples = [video_sample_with_timing(entry, 3000, None, 0, data_size)];

    let result = muxer.create_media_segment_metadata(&samples);
    assert!(
        matches!(result, Err(MuxError::Overflow)),
        "Overflow を期待したが {:?} だった",
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
        video_sample_with_timing(first_entry, 3000, None, 0, first_size),
        video_sample_with_timing(second_entry, 3000, None, first_size as u64, second_size),
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
/// 修正前実装（`EPT = track.decode_time`）でも通過するテストであり、
/// 「CTO=None 時に旧挙動と等価であり後方互換性が壊れない」ことの回帰防止を目的とする。
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

/// 参照トラック以外（Audio）のサンプルの CTO / duration が EPT の計算に混入しないこと
///
/// `samples[0].track_kind` である Video だけを filter して DTS / PTS を計算する契約を検証する。
/// もし filter が壊れて Audio まで走査すると、Audio の (`dur` 大 + `CTO=-大`) が
/// dts 累積と min PTS を汚染し EPT が変わる（極端な負値の場合は `Overflow` になる）。
///
/// resolve_segment_tracks は「同一トラック内での data_offset 連続配置」を要求するため、
/// Video 2 サンプルを前半に、Audio 1 サンプルを後半に置いている。
#[test]
fn sidx_ept_ignores_non_reference_track_samples() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let video_entry = create_avc1_sample_entry(320, 240);
    let audio_entry = create_opus_sample_entry();
    let video_size = 16usize;
    let audio_size = 8usize;
    let samples = [
        video_sample_with_timing(video_entry.clone(), 100, None, 0, video_size),
        video_sample_with_timing(video_entry, 100, None, video_size as u64, video_size),
        audio_sample_with_timing(
            audio_entry,
            999_999_999,
            Some(-999_999_999),
            (video_size * 2) as u64,
            audio_size,
        ),
    ];

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("セグメントの生成に失敗した");
    // Video 側 PTS は [0, 100]、Audio は filter で除外されるので EPT は 0
    assert_eq!(
        decode_sidx(&segment).earliest_presentation_time,
        0,
        "非参照トラックの CTO が EPT に混入していない場合、EPT は 0 であるべき"
    );
}

/// 第 2 セグメントで複数サンプルを渡したとき、`decode_time` 起点の DTS 累積 + 各サンプルの CTO が
/// すべて PTS に反映されること
///
/// 第 1 セグメントで `decode_time` を 1000 まで進める。
/// 第 2 セグメントに `[(dur=100, cto=+50), (dur=100, cto=-70)]` を渡すと:
///   PTS_0 = 1000 + 50 = 1050
///   PTS_1 = (1000 + 100) - 70 = 1030 (2 サンプル目に dts 累積が反映される)
/// EPT は最小値の 1030。単一サンプルテストでは通らないサンプル間の dts 累積経路と、
/// セグメント跨ぎの decode_time 起点を同時に踏むケース。
#[test]
fn sidx_ept_across_segments_with_multiple_samples() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);

    // 第 1 セグメント: decode_time を 1000 まで進める
    muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry.clone(),
            1000,
            None,
            0,
            16,
        )])
        .expect("第 1 セグメントの生成に失敗した");

    let sample_size = 16usize;
    let samples = [
        video_sample_with_timing(entry.clone(), 100, Some(50), 0, sample_size),
        video_sample_with_timing(entry, 100, Some(-70), sample_size as u64, sample_size),
    ];
    let second_segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("第 2 セグメントの生成に失敗した");
    assert_eq!(
        decode_sidx(&second_segment).earliest_presentation_time,
        1030,
        "第 2 サンプルの PTS=(1000+100)-70=1030 が最小になるため EPT は 1030 であるべき"
    );
}
