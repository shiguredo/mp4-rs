//! `Fmp4SegmentMuxer` の公開 API 契約の単体テスト
//!
//! 対象:
//! - 主要エラーパス（`EmptyTracks` / `EmptySamples` / `MixedSampleEntries`）
//! - `mdat` ボックスサイズ計算の `Overflow` 契約（通常ヘッダー経路と拡張サイズ経路）
//! - `create_media_segment_metadata_with_sidx` の `earliest_presentation_time` の
//!   値・境界・`Overflow` 契約
//!   - CTO=None 時に旧挙動と等価であること（後方互換性の回帰防止）
//!   - 複数サンプルで PTS 最小値が採用されること
//!   - セグメント跨ぎで負 CTO により先頭 DTS を下回るケース
//!   - 負 PTS で `MuxError::Overflow` を返すこと
//! - `create_media_segment_metadata_with_sidx` の `starts_with_sap` / `sap_type` が
//!   EPT サンプルの `keyframe` と整合すること
//!   - EPT が B フレームのとき `starts_with_sap=false` / `sap_type=0`
//!   - EPT が I フレームのとき `starts_with_sap=true` / `sap_type=1`
//!   - 非参照トラックの `keyframe` に引きずられないこと
//!   - PTS 同値時は samples[] 内で先出現のサンプルの `keyframe` が採られること
//!
//! - CENC (DRM) の `pssh` / `encv` / `enca` を含む init セグメントの生成
//!   - `create_media_segment_metadata` が `Encv` / `Enca` のサンプルエントリーを受け付けること
//!   - `SegmentMuxerOptions::pssh_boxes` の `pssh` が `moov` にそのまま出力されること
//!   - `stsd` に `sinf` / `schm` / `schi` / `tenc` 付きの `encv` / `enca` が入ること
//!   - `encv` の `frma` に応じて `ftyp` に `hev1` ブランドが立つこと
//!
//! 意図的なエラーパスと境界値は固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_segment_mux_demux.rs` が担う。

use std::num::{NonZeroU16, NonZeroU32};

use shiguredo_mp4::{
    Decode, FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, Brand, DopsBox, EncaBox, EncvBox, FrmaBox,
        FtypBox, MoofBox, MoovBox, OpusBox, PsshBox, SampleEntry, SchiBox, SchmBox, SidxBox,
        SinfBox, TencBox, VisualSampleEntryFields,
    },
    mux::{Fmp4SegmentMuxer, MuxError, Sample, SegmentMuxerOptions},
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

/// EPT サンプルが B フレームのとき `starts_with_sap=false` / `sap_type=0` になること
///
/// `decode_time=0`, duration=100 固定で:
///   samples[0]: I, keyframe=true,  CTO=+50  → PTS=50
///   samples[1]: B, keyframe=false, CTO=-40  → PTS=60
///   samples[2]: B, keyframe=false, CTO=-180 → PTS=20 (最小 = EPT)
///
/// samples[0] の keyframe を採ると虚偽の `starts_with_sap=true` になるため、
/// EPT を採ったサンプル（samples[2]）の keyframe を使う契約を固定する。
#[test]
fn sidx_starts_with_sap_false_when_ept_sample_is_b_frame() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let size = 16usize;
    let samples = [
        video_sample_with_timing(entry.clone(), 100, Some(50), 0, size),
        Sample {
            keyframe: false,
            ..video_sample_with_timing(entry.clone(), 100, Some(-40), size as u64, size)
        },
        Sample {
            keyframe: false,
            ..video_sample_with_timing(entry, 100, Some(-180), (size * 2) as u64, size)
        },
    ];

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("セグメントの生成に失敗した");
    let sidx = decode_sidx(&segment);
    assert_eq!(
        sidx.earliest_presentation_time, 20,
        "EPT は samples[2] の PTS=20 であるべき"
    );
    let reference = &sidx.references[0];
    assert!(
        !reference.starts_with_sap,
        "EPT サンプルが B フレームなら starts_with_sap は false であるべき"
    );
    assert_eq!(
        reference.sap_type, 0,
        "EPT サンプルが B フレームなら sap_type は 0 であるべき"
    );
    assert_eq!(
        reference.sap_delta_time, 0,
        "sap_delta_time は現行の 0 のまま変更されないべき"
    );
}

/// EPT サンプルが I フレームのとき `starts_with_sap=true` / `sap_type=1` になること
///
/// 従来ケース（EPT サンプル == samples[0] == I フレーム）の回帰防止。
/// CTO なしの単一キーフレームでは EPT = DTS = 0 となり、そのサンプル自身が SAP。
#[test]
fn sidx_starts_with_sap_true_when_ept_sample_is_i_frame() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&[video_sample_with_timing(
            entry, 100, None, 0, 16,
        )])
        .expect("セグメントの生成に失敗した");
    let sidx = decode_sidx(&segment);
    assert_eq!(
        sidx.earliest_presentation_time, 0,
        "CTO なしの単一サンプルでは EPT は 0 であるべき"
    );
    let reference = &sidx.references[0];
    assert!(
        reference.starts_with_sap,
        "EPT サンプルが I フレームなら starts_with_sap は true であるべき"
    );
    assert_eq!(
        reference.sap_type, 1,
        "EPT サンプルが I フレームなら sap_type は 1 であるべき"
    );
    assert_eq!(
        reference.sap_delta_time, 0,
        "sap_delta_time は現行の 0 のまま変更されないべき"
    );
}

/// 非参照トラック（Audio）の `keyframe` が `starts_with_sap` に混入しないこと
///
/// Video を `samples[0]` にして参照トラックとし、Video 群を前半・Audio 群を後半に
/// `data_offset` 連続で配置する（`sidx_ept_ignores_non_reference_track_samples` と同じ配置パターン）。
/// Audio は全て `keyframe=true` でも、Video の EPT サンプルが B フレームなら
/// `starts_with_sap=false` になることを固定する。
#[test]
fn sidx_starts_with_sap_ignores_non_reference_track_keyframes() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let video_entry = create_avc1_sample_entry(320, 240);
    let audio_entry = create_opus_sample_entry();
    let video_size = 16usize;
    let audio_size = 8usize;
    // Video: I (PTS=50) → B (PTS=20 = EPT)。Audio は全て keyframe=true。
    let samples = [
        video_sample_with_timing(video_entry.clone(), 100, Some(50), 0, video_size),
        Sample {
            keyframe: false,
            ..video_sample_with_timing(video_entry, 100, Some(-80), video_size as u64, video_size)
        },
        audio_sample_with_timing(audio_entry, 100, None, (video_size * 2) as u64, audio_size),
    ];

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("セグメントの生成に失敗した");
    let sidx = decode_sidx(&segment);
    assert_eq!(
        sidx.earliest_presentation_time, 20,
        "Video の EPT は samples[1] の PTS=20 であるべき"
    );
    let reference = &sidx.references[0];
    assert!(
        !reference.starts_with_sap,
        "非参照トラックの keyframe=true に引きずられず starts_with_sap は false であるべき"
    );
    assert_eq!(
        reference.sap_type, 0,
        "非参照トラックの keyframe に引きずられず sap_type は 0 であるべき"
    );
    assert_eq!(
        reference.sap_delta_time, 0,
        "sap_delta_time は現行の 0 のまま変更されないべき"
    );
}

/// PTS 同値時は samples[] 内で先出現のサンプルの `keyframe` が採られる（先勝ち）
///
/// `decode_time=0`, duration=100 固定で:
///   samples[0]: I, keyframe=true,  CTO=+50 → PTS=50
///   samples[1]: B, keyframe=false, CTO=-50 → PTS=50 (同値)
///
/// 実装は `compute_earliest_presentation_time` の `pts >= current` で更新をスキップするため、
/// 先出現の samples[0] の keyframe=true が採られる契約。将来 `>` に書き換わると samples[1] の
/// keyframe=false が採られて `starts_with_sap` が反転する脆弱性への回帰防止テスト。
#[test]
fn sidx_starts_with_sap_ties_prefer_first_occurrence() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let size = 16usize;
    let samples = [
        video_sample_with_timing(entry.clone(), 100, Some(50), 0, size),
        Sample {
            keyframe: false,
            ..video_sample_with_timing(entry, 100, Some(-50), size as u64, size)
        },
    ];

    let segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("セグメントの生成に失敗した");
    let sidx = decode_sidx(&segment);
    assert_eq!(
        sidx.earliest_presentation_time, 50,
        "PTS 同値時の EPT は 50 であるべき"
    );
    let reference = &sidx.references[0];
    assert!(
        reference.starts_with_sap,
        "PTS 同値時は先出現サンプルの keyframe=true が採られ starts_with_sap は true であるべき"
    );
    assert_eq!(
        reference.sap_type, 1,
        "PTS 同値時は先出現サンプルの keyframe=true が採られ sap_type は 1 であるべき"
    );
    assert_eq!(
        reference.sap_delta_time, 0,
        "sap_delta_time は現行の 0 のまま変更されないべき"
    );
}

// ===== CENC (DRM) の初期化データのテスト =====

/// DRM テスト用の映像トラックの KID
const VIDEO_KID: [u8; 16] = [0x11; 16];

/// DRM テスト用の音声トラックの KID
const AUDIO_KID: [u8; 16] = [0x22; 16];

/// `encv` 用の有効な `sinf` を組み立てる
///
/// CMSF 4.2 が要求する `schm` と `schi` / `tenc` をすべて含める。
/// `frma` は `hev1` にして、init セグメントの `ftyp` に `hev1` ブランドが立つことを
/// 確認できるようにする。`cbcs` はパターン暗号化を使うスキームなので、
/// `tenc` はパターン暗号化パラメーターを持つ version 1 にする。
fn create_encv_sinf_box() -> SinfBox {
    SinfBox {
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
    }
}

/// `frma=hev1` の `EncvBox` を持つ `SampleEntry::Encv` を組み立てる
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
        sinf_box: create_encv_sinf_box(),
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
/// 複数の `pssh` がそのままの順序で保持されることを確認できるようにする。
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
fn drm_video_sample(sample_entry: SampleEntry, data_offset: u64, data_size: usize) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 3000,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size,
    }
}

/// DRM サンプルエントリーを持つ音声サンプルを組み立てる
fn drm_audio_sample(sample_entry: SampleEntry, data_offset: u64, data_size: usize) -> Sample {
    Sample {
        track_kind: TrackKind::Audio,
        timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration: 960,
        keyframe: true,
        composition_time_offset: None,
        data_offset,
        data_size,
    }
}

/// `create_media_segment_metadata` が `Encv` / `Enca` のサンプルエントリーを受け付けること
///
/// DRM で保護された 2 トラック分のサンプルを渡し、メディアセグメントの `moof` に
/// 2 つの `traf` が作られることを確認する。サンプルエントリーの内容の検証は
/// init セグメント側のテストで行う。
#[test]
fn create_media_segment_metadata_accepts_drm_sample_entries() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let video_size = 16usize;
    let samples = [
        drm_video_sample(create_encv_sample_entry(320, 240), 0, video_size),
        drm_audio_sample(create_enca_sample_entry(), video_size as u64, 8),
    ];

    let media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("Encv / Enca のサンプルエントリーを受け付けなかった");
    let (moof_box, _moof_size) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    assert_eq!(
        moof_box.traf_boxes.len(),
        2,
        "Encv / Enca の 2 トラック分の traf が作られるべき"
    );
}

/// `pssh` と `Encv` / `Enca` を含む init セグメントを生成できること
///
/// 次の 3 点を確認する。
/// - `SegmentMuxerOptions::pssh_boxes` に渡した `pssh` が `moov` にそのまま出力されること
/// - `stsd` の `encv` / `enca` に `sinf` / `schm` / `schi` / `tenc` が型付きで入ること
/// - `encv` の `frma=hev1` に応じて `ftyp` に `hev1` ブランドが立つこと
#[test]
fn init_segment_contains_pssh_and_drm_sample_entries() {
    let pssh_boxes = create_pssh_boxes();
    let options = SegmentMuxerOptions {
        pssh_boxes: pssh_boxes.clone(),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");

    let video_entry = create_encv_sample_entry(320, 240);
    let audio_entry = create_enca_sample_entry();
    let video_size = 16usize;
    let samples = [
        drm_video_sample(video_entry, 0, video_size),
        drm_audio_sample(audio_entry, video_size as u64, 8),
    ];
    muxer
        .create_media_segment_metadata(&samples)
        .expect("Encv / Enca のサンプルエントリーを受け付けなかった");

    let init_segment = muxer
        .init_segment_bytes()
        .expect("DRM の init セグメントの作成に失敗した");

    // init セグメントは ftyp + moov のみで、moov が pssh を保持している
    let (ftyp_box, ftyp_size) =
        FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let (moov_box, moov_size) = MoovBox::decode(&init_segment[ftyp_size..])
        .expect("init セグメントからの moov デコードに失敗した");
    assert_eq!(
        ftyp_size + moov_size,
        init_segment.len(),
        "init セグメントは ftyp + moov のみを含む"
    );
    assert_eq!(
        moov_box.pssh_boxes, pssh_boxes,
        "moov に出力された pssh が SegmentMuxerOptions::pssh_boxes と一致しない"
    );
    assert_eq!(
        moov_box.trak_boxes.len(),
        2,
        "DRM の映像・音声の 2 トラックが moov に含まれるべき"
    );

    // 映像トラック: encv の sinf / tenc と、tkhd に反映された解像度を確認する
    let video_trak = &moov_box.trak_boxes[0];
    let video_entries = &video_trak.mdia_box.minf_box.stbl_box.stsd_box.entries;
    assert_eq!(
        video_entries.len(),
        1,
        "映像トラックの stsd は 1 エントリー"
    );
    let SampleEntry::Encv(encv_box) = &video_entries[0] else {
        panic!("映像トラックの stsd が Encv でない: {:?}", video_entries[0]);
    };
    assert_eq!(
        encv_box.sinf_box.frma_box.data_format, *b"hev1",
        "encv の frma が hev1 でない"
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
    assert_eq!(
        video_trak.tkhd_box.width,
        FixedPointNumber::new(320, 0),
        "encv の幅が tkhd に反映されていない"
    );
    assert_eq!(
        video_trak.tkhd_box.height,
        FixedPointNumber::new(240, 0),
        "encv の高さが tkhd に反映されていない"
    );

    // 音声トラック: enca の sinf / tenc を確認する
    let audio_trak = &moov_box.trak_boxes[1];
    let audio_entries = &audio_trak.mdia_box.minf_box.stbl_box.stsd_box.entries;
    assert_eq!(
        audio_entries.len(),
        1,
        "音声トラックの stsd は 1 エントリー"
    );
    let SampleEntry::Enca(enca_box) = &audio_entries[0] else {
        panic!("音声トラックの stsd が Enca でない: {:?}", audio_entries[0]);
    };
    assert_eq!(
        enca_box.sinf_box.frma_box.data_format, *b"mp4a",
        "enca の frma が mp4a でない"
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
    assert_eq!(
        tenc_box.default_skip_byte_block, None,
        "cenc の tenc はパターン暗号化パラメーターを持たないべき"
    );

    // encv の frma=hev1 に対応するブランドだけが立つ
    assert!(
        ftyp_box.compatible_brands.contains(&Brand::HEV1),
        "encv の frma=hev1 に対応する HEV1 ブランドが ftyp にない: {:?}",
        ftyp_box.compatible_brands
    );
    assert!(
        !ftyp_box.compatible_brands.contains(&Brand::HVC1),
        "hvc1 の encv がないのに HVC1 ブランドが ftyp にある: {:?}",
        ftyp_box.compatible_brands
    );
    assert!(
        !ftyp_box.compatible_brands.contains(&Brand::AV01),
        "av01 の encv がないのに AV01 ブランドが ftyp にある: {:?}",
        ftyp_box.compatible_brands
    );
}
