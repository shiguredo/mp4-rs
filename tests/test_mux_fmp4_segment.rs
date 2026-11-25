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
//! - `SegmentMuxerOptions::styp` を指定したときの出力
//!   - 出力順が `styp` + `moof` + `mdat`、および `styp` + `sidx` + `moof` + `mdat` になること
//!   - `styp` を指定しない場合の出力が `moof`（または `sidx`）から始まること
//!   - `sidx.referenced_size` が `moof` + `mdat` の合計で、`first_offset` が 0 になること
//!   - `mfra` の `tfra.moof_offset` が実際の `moof` 位置と一致すること
//!
//! - `SegmentMuxerOptions::cmaf` を指定したときの CMAF の構造ブランド出力
//!   - `ftyp` が `major_brand = cmfc` / `minor_version = 0` になり、互換 brand に `cmfc` を含むこと
//!   - `styp` が `major_brand = cmfs` になり、互換 brand が `ftyp` の互換 brand に
//!     `cmfs` を加えた集合になること
//!   - `cmaf` を指定しない場合の出力が `new()` で作った muxer と一致すること
//!   - 複数の `TrackKind` を観測する場合に `MuxError::EncodeError` になり、状態が変わらないこと
//!   - `cmaf` と `styp` の同時指定が `MuxError::EncodeError` になること
//!   - `Fmp4SegmentDemuxer` で読み戻した `styp()` とサンプルが入力と一致すること
//!
//! - CENC (DRM) の `pssh` / `encv` / `enca` を含む init セグメントの生成
//!   - `create_media_segment_metadata` が `Encv` / `Enca` のサンプルエントリーを受け付けること
//!   - `SegmentMuxerOptions::pssh_boxes` の `pssh` が `moov` にそのまま出力されること
//!   - `stsd` に `sinf` / `schm` / `schi` / `tenc` 付きの `encv` / `enca` が入ること
//!   - `encv` の `frma` に応じて `ftyp` に `hev1` ブランドが立つこと
//!
//! 意図的なエラーパスと境界値は固定入力で契約を検証するため、PBT ではなく単体テストとして置く。
//! 正常系のラウンドトリップは `pbt/tests/prop_fmp4_segment_mux_demux.rs` が担うが、
//! CMAF 指定時の `styp` のブランドとサンプルの往復は `cmaf` 指定に固有の契約であるため、
//! このファイルでも固定する。

use std::num::{NonZeroU16, NonZeroU32};

use shiguredo_mp4::{
    BoxHeader, BoxType, Decode, Encode, ErrorKind, FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, Brand, DopsBox, EncaBox, EncvBox, FrmaBox,
        FtypBox, MfraBox, MoofBox, MoovBox, OpusBox, PsshBox, SaioBox, SaizBox,
        SampleEncryptionInfo, SampleEntry, SbgpBox, SbgpEntry, SchiBox, SchmBox, SeigEntry,
        SencBox, SencSample, SgpdBox, SgpdEntry, SidxBox, SinfBox, StypBox, Subsample, TencBox,
        VisualSampleEntryFields,
    },
    demux::Fmp4SegmentDemuxer,
    mux::{CmafTrackFileBrand, Fmp4SegmentMuxer, MuxError, Sample, SegmentMuxerOptions},
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
        encryption_info: None,
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
        encryption_info: None,
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
        // `create_encv_sample_entry()` の `tenc` の `default_per_sample_iv_size` は 16
        encryption_info: Some(SampleEncryptionInfo {
            iv: vec![0x33; 16],
            subsamples: Vec::new(),
            kid: None,
        }),
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
        // `create_enca_sample_entry()` の `tenc` の `default_per_sample_iv_size` は 8
        encryption_info: Some(SampleEncryptionInfo {
            iv: vec![0x44; 8],
            subsamples: Vec::new(),
            kid: None,
        }),
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

// ===== サンプル補助情報 (senc / saiz / saio) の出力テスト =====

/// 指定した保護スキームと `tenc` を持つ `encv` のサンプルエントリーを組み立てる
///
/// `tenc` の内容だけを差し替えられるようにし、サンプルエントリーの共通部分を
/// 1 か所にまとめる。
fn create_encv_sample_entry_from_tenc(scheme_type: [u8; 4], tenc_box: TencBox) -> SampleEntry {
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
                tenc_box: Some(tenc_box),
                unknown_boxes: vec![],
            }),
            unknown_boxes: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// `cenc` スキームの `tenc` を指定して `encv` のサンプルエントリーを組み立てる
///
/// 選択的暗号化と constant IV の検証で、`default_is_protected` / `default_per_sample_iv_size` を
/// 差し替えられるようにしている。
fn create_encv_sample_entry_with_tenc(
    default_is_protected: u8,
    default_per_sample_iv_size: u8,
    default_constant_iv: Option<Vec<u8>>,
) -> SampleEntry {
    create_encv_sample_entry_from_tenc(
        // `cenc` はパターン暗号化を使わないスキーム
        *b"cenc",
        TencBox {
            default_is_protected,
            default_per_sample_iv_size,
            default_kid: VIDEO_KID,
            default_constant_iv,
            default_crypt_byte_block: None,
            default_skip_byte_block: None,
        },
    )
}

/// `tenc` version 1 でパターン暗号化パラメーターが 0 / 0 の `encv` のサンプルエントリーを
/// 組み立てる
///
/// `cbcs` はパターン暗号化を使うスキームなので `tenc` は version 1 でなければならない。
/// version 1 の 0 / 0 はパターン暗号化を使わないことを表す。`seig` のパターン値も
/// 0 / 0 であるため、KID の切り替えで復号結果は変わらない。
fn create_encv_sample_entry_with_zero_pattern_tenc() -> SampleEntry {
    create_encv_sample_entry_from_tenc(
        *b"cbcs",
        TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 16,
            default_kid: VIDEO_KID,
            default_constant_iv: None,
            default_crypt_byte_block: Some(Uint::new(0)),
            default_skip_byte_block: Some(Uint::new(0)),
        },
    )
}

/// 暗号化情報を指定できる映像サンプルを組み立てる
fn encrypted_video_sample(
    sample_entry: SampleEntry,
    encryption_info: Option<SampleEncryptionInfo>,
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
        encryption_info,
    }
}

/// `traf` の `unknown_boxes` から指定された種別のボックスを 1 つ取り出す
#[track_caller]
fn find_traf_unknown_box(traf_box: &shiguredo_mp4::boxes::TrafBox, box_type: [u8; 4]) -> Vec<u8> {
    traf_box
        .unknown_boxes
        .iter()
        .find(|unknown_box| unknown_box.box_type.as_bytes() == box_type)
        .unwrap_or_else(|| {
            panic!(
                "traf に {} が見つからない",
                String::from_utf8_lossy(&box_type)
            )
        })
        .encode_to_vec()
        .expect("テスト用の UnknownBox はエンコードできる")
}

/// mux が暗号化されたサンプルのある `traf` に `senc` / `saiz` / `saio` を書き出すこと
///
/// `saio` の offset が moof 先頭から `senc` 内の最初の SAI 先頭までのバイト数と一致し、
/// その位置にサンプルの IV とサブサンプルが実際に置かれていることも確認する。
#[test]
fn muxer_writes_senc_saiz_saio_for_encrypted_track() {
    let sample_entry = create_encv_sample_entry(320, 240);
    let iv16 = |value: u8| vec![value; 16];
    let samples = [
        encrypted_video_sample(
            sample_entry.clone(),
            Some(SampleEncryptionInfo {
                iv: iv16(0x11),
                subsamples: vec![Subsample {
                    bytes_of_clear_data: 5,
                    bytes_of_protected_data: 10,
                }],
                kid: None,
            }),
            0,
            16,
        ),
        encrypted_video_sample(
            sample_entry,
            Some(SampleEncryptionInfo {
                iv: iv16(0x22),
                subsamples: Vec::new(),
                kid: None,
            }),
            16,
            16,
        ),
    ];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("暗号化サンプルのメディアセグメントの作成に失敗した");

    let (moof_box, moof_size) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    assert_eq!(moof_box.traf_boxes.len(), 1);
    let traf_box = &moof_box.traf_boxes[0];

    // senc: サンプルごとの IV とサブサンプルが、入力のまま書き出されている
    let senc_bytes = find_traf_unknown_box(traf_box, *b"senc");
    let (senc_box, senc_size) =
        SencBox::decode_with_iv_size(&senc_bytes, 16).expect("senc のデコードに失敗した");
    assert_eq!(senc_size, senc_bytes.len());
    assert_eq!(
        senc_box.samples,
        vec![
            SencSample {
                iv: iv16(0x11),
                subsamples: vec![Subsample {
                    bytes_of_clear_data: 5,
                    bytes_of_protected_data: 10,
                }],
            },
            SencSample {
                iv: iv16(0x22),
                subsamples: Vec::new(),
            },
        ],
        "senc に書き出された暗号化情報が入力と一致しない"
    );
    // サブサンプルを持つサンプルがあるため、flags の 0x000002 が立つ
    assert_eq!(
        &senc_bytes[8..12],
        &[0x00, 0x00, 0x00, 0x02],
        "senc の flags にサブサンプル暗号化のビットが立っていない"
    );

    // saiz: サブサンプルフラグが立っているため、サブサンプルを持たないサンプルにも
    // subsample_count の 2 バイトが加わる
    let saiz_bytes = find_traf_unknown_box(traf_box, *b"saiz");
    let (saiz_box, _) = SaizBox::decode(&saiz_bytes).expect("saiz のデコードに失敗した");
    assert_eq!(
        saiz_box.default_sample_info_size, 0,
        "saiz の default_sample_info_size は使わない"
    );
    assert_eq!(saiz_box.sample_count, 2);
    assert_eq!(
        saiz_box.sample_info_sizes,
        vec![16 + 2 + 6, 16 + 2],
        "saiz の SAI 長が senc の内容と一致しない"
    );

    // saio: offset は moof 先頭から senc 内の最初の SAI 先頭まで
    let saio_bytes = find_traf_unknown_box(traf_box, *b"saio");
    let (saio_box, _) = SaioBox::decode(&saio_bytes).expect("saio のデコードに失敗した");
    assert_eq!(saio_box.version, 0);
    assert_eq!(saio_box.offsets.len(), 1, "saio の entry_count は 1");
    // `senc` のボックス種別はボックス先頭から 4 バイト目にあり、最初の SAI は
    // そこからさらに 12 バイト後ろ (ヘッダー 8 + FullBox 4 + sample_count 4) にある
    let senc_type_position = media_segment
        .windows(4)
        .position(|window| window == b"senc")
        .expect("media segment に senc のボックス種別が見つからない");
    let expected_sai_offset = u64::try_from(senc_type_position - 4 + 16)
        .expect("テスト用のセグメントサイズは u64 に収まる");
    assert_eq!(
        saio_box.offsets,
        vec![expected_sai_offset],
        "saio の offset が senc 内の最初の SAI 先頭を指していない"
    );
    let sai_offset = usize::try_from(expected_sai_offset).expect("u64 から usize への変換に失敗");
    assert!(
        sai_offset < moof_size,
        "saio の offset が moof の範囲外を指している"
    );
    // 先頭サンプルの SAI は IV (16 バイト) + subsample_count (2 バイト) + サブサンプル (6 バイト)
    let mut expected_sai = iv16(0x11);
    expected_sai.extend_from_slice(&1u16.to_be_bytes());
    expected_sai.extend_from_slice(&5u16.to_be_bytes());
    expected_sai.extend_from_slice(&10u32.to_be_bytes());
    assert_eq!(
        &media_segment[sai_offset..sai_offset + expected_sai.len()],
        expected_sai.as_slice(),
        "saio の offset が指す位置に最初のサンプルの SAI がない"
    );
}

/// サブサンプルを持たない暗号化サンプルでは `senc` の flags が 0 になること
#[test]
fn muxer_writes_senc_without_subsample_flag_for_full_sample_encryption() {
    let sample = encrypted_video_sample(
        create_encv_sample_entry(320, 240),
        Some(SampleEncryptionInfo {
            iv: vec![0x33; 16],
            subsamples: Vec::new(),
            kid: None,
        }),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let media_segment = muxer
        .create_media_segment_metadata(&[sample])
        .expect("暗号化サンプルのメディアセグメントの作成に失敗した");
    let (moof_box, _) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    let senc_bytes = find_traf_unknown_box(&moof_box.traf_boxes[0], *b"senc");
    assert_eq!(
        &senc_bytes[8..12],
        &[0x00, 0x00, 0x00, 0x00],
        "サブサンプルがない場合は senc の flags が 0 になる"
    );
    let saiz_bytes = find_traf_unknown_box(&moof_box.traf_boxes[0], *b"saiz");
    let (saiz_box, _) = SaizBox::decode(&saiz_bytes).expect("saiz のデコードに失敗した");
    assert_eq!(
        saiz_box.sample_info_sizes,
        vec![16],
        "サブサンプルがない場合の SAI 長は IV 長だけになる"
    );
}

/// `tenc.default_is_protected = 1` のトラックで `encryption_info` が `None` のサンプルが
/// ある場合に InvalidInput になること
#[test]
fn muxer_rejects_missing_encryption_info_for_protected_track() {
    let sample = encrypted_video_sample(create_encv_sample_entry(320, 240), None, 0, 16);
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("暗号化情報のない暗号化サンプルを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("encrypted track requires sample encryption info"),
                "暗号化情報の有無の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// `tenc.default_is_protected = 0` のトラックで `encryption_info` が `Some` のサンプルが
/// ある場合に InvalidInput になること
#[test]
fn muxer_rejects_encryption_info_for_clear_track() {
    let sample = encrypted_video_sample(
        create_encv_sample_entry_with_tenc(0, 16, None),
        Some(SampleEncryptionInfo {
            iv: vec![0x44; 16],
            subsamples: Vec::new(),
            kid: None,
        }),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("選択的暗号化のサンプルを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("clear track must not have sample encryption info"),
                "選択的暗号化の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// `tenc.default_per_sample_iv_size` と `encryption_info.iv` の長さが一致しない場合に
/// InvalidInput になること
#[test]
fn muxer_rejects_iv_size_mismatch() {
    let sample = encrypted_video_sample(
        create_encv_sample_entry(320, 240),
        Some(SampleEncryptionInfo {
            iv: vec![0x55; 8],
            subsamples: Vec::new(),
            kid: None,
        }),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("IV 長が tenc と一致しないサンプルを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("does not match tenc default_per_sample_iv_size"),
                "IV 長の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// `tenc` を持たないトラックに `encryption_info` を渡すと InvalidInput になること
#[test]
fn muxer_rejects_encryption_info_without_tenc() {
    let sample = encrypted_video_sample(
        create_avc1_sample_entry(320, 240),
        Some(SampleEncryptionInfo {
            iv: vec![0x66; 16],
            subsamples: Vec::new(),
            kid: None,
        }),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("tenc を持たないトラックの暗号化サンプルを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("requires a CENC protected sample entry"),
                "サンプルエントリーの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

// ===== 鍵ローテーション (`seig` サンプルグループ) の出力テスト =====

/// 鍵ローテーションテスト用の KID A
const ROTATED_KID_A: [u8; 16] = [0xAA; 16];

/// 鍵ローテーションテスト用の KID B
const ROTATED_KID_B: [u8; 16] = [0xBB; 16];

/// IV 16 バイトの暗号化情報を KID 付きで組み立てる
fn kid_encryption_info(iv: u8, kid: Option<[u8; 16]>) -> SampleEncryptionInfo {
    SampleEncryptionInfo {
        iv: vec![iv; 16],
        subsamples: Vec::new(),
        kid,
    }
}

/// `traf` の `unknown_boxes` に指定種別のボックスがあるかを返す
fn has_traf_unknown_box(traf_box: &shiguredo_mp4::boxes::TrafBox, box_type: [u8; 4]) -> bool {
    traf_box
        .unknown_boxes
        .iter()
        .any(|unknown_box| unknown_box.box_type.as_bytes() == box_type)
}

/// KID が切り替わるサンプルに対して `sgpd` / `sbgp` を書き出すこと
///
/// `sgpd` は version 1、`grouping_type = seig`、`default_length = 20` で KID ごとに
/// 1 エントリーを持ち、`sbgp` は version 0 で連続する同一 KID ごとに 1 エントリーを
/// 持つ。1 番目の KID の index が `0x10001`、2 番目が `0x10002` になり、既定 KID を
/// 使うサンプルは index 0 になることを、デコードした値で確認する。
#[test]
fn muxer_writes_sgpd_and_sbgp_for_kid_rotation() {
    let sample_entry = create_encv_sample_entry_with_tenc(1, 16, None);
    let samples = [
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x11, Some(ROTATED_KID_A))),
            0,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x22, Some(ROTATED_KID_A))),
            16,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x33, Some(ROTATED_KID_B))),
            32,
            16,
        ),
        encrypted_video_sample(sample_entry, Some(kid_encryption_info(0x44, None)), 48, 16),
    ];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("KID が切り替わるメディアセグメントの作成に失敗した");
    let (moof_box, _) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    assert_eq!(moof_box.traf_boxes.len(), 1, "テストは 1 traf を前提にする");
    let traf_box = &moof_box.traf_boxes[0];

    // sgpd: version 1、seig、default_length 20 で KID ごとに 1 エントリー
    let sgpd_bytes = find_traf_unknown_box(traf_box, *b"sgpd");
    let (sgpd_box, sgpd_size) = SgpdBox::decode(&sgpd_bytes).expect("sgpd のデコードに失敗した");
    assert_eq!(sgpd_size, sgpd_bytes.len());
    assert_eq!(sgpd_box.version, 1, "sgpd の version が 1 ではない");
    assert_eq!(
        sgpd_box.grouping_type, *b"seig",
        "sgpd の grouping_type が 'seig' ではない"
    );
    assert_eq!(
        sgpd_box.default_length,
        Some(20),
        "sgpd の default_length が 20 ではない"
    );
    assert_eq!(
        sgpd_box.entries,
        vec![
            SgpdEntry::Seig(SeigEntry {
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: ROTATED_KID_A,
                constant_iv: None,
            }),
            SgpdEntry::Seig(SeigEntry {
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: ROTATED_KID_B,
                constant_iv: None,
            }),
        ],
        "sgpd に書き出された KID のエントリーが入力と一致しない"
    );

    // sbgp: version 0、連続する同一 KID ごとに 1 エントリー
    let sbgp_bytes = find_traf_unknown_box(traf_box, *b"sbgp");
    let (sbgp_box, sbgp_size) = SbgpBox::decode(&sbgp_bytes).expect("sbgp のデコードに失敗した");
    assert_eq!(sbgp_size, sbgp_bytes.len());
    assert_eq!(sbgp_box.version, 0, "sbgp の version が 0 ではない");
    assert_eq!(
        sbgp_box.grouping_type, *b"seig",
        "sbgp の grouping_type が 'seig' ではない"
    );
    assert_eq!(
        sbgp_box.grouping_type_parameter, None,
        "sbgp version 0 は grouping_type_parameter を持たない"
    );
    assert_eq!(
        sbgp_box.entries,
        vec![
            SbgpEntry {
                sample_count: 2,
                group_description_index: 0x1_0001,
            },
            SbgpEntry {
                sample_count: 1,
                group_description_index: 0x1_0002,
            },
            SbgpEntry {
                sample_count: 1,
                group_description_index: 0,
            },
        ],
        "sbgp の group_description_index が KID の並びと一致しない"
    );
}

/// すべてのサンプルの実効 KID が `tenc.default_kid` と同じ場合は
/// `sgpd` / `sbgp` を書き出さないこと
///
/// `Some(tenc.default_kid)` は `None` に正規化されるため、これも既定 KID とみなす。
#[test]
fn muxer_does_not_write_sample_groups_when_all_kids_are_default() {
    let sample_entry = create_encv_sample_entry_with_tenc(1, 16, None);
    let samples = [
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x11, Some(VIDEO_KID))),
            0,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x22, None)),
            16,
            16,
        ),
        encrypted_video_sample(
            sample_entry,
            Some(kid_encryption_info(0x33, Some(VIDEO_KID))),
            32,
            16,
        ),
    ];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("既定 KID のみのメディアセグメントの作成に失敗した");
    let (moof_box, _) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    let traf_box = &moof_box.traf_boxes[0];
    assert!(
        !has_traf_unknown_box(traf_box, *b"sgpd"),
        "既定 KID のみなのに sgpd を書き出している"
    );
    assert!(
        !has_traf_unknown_box(traf_box, *b"sbgp"),
        "既定 KID のみなのに sbgp を書き出している"
    );
}

/// `tenc` にパターン暗号化パラメーターを持つトラックに `kid: Some` を渡すと
/// InvalidInput になること
///
/// `seig` のパターン値は常に 0 / 0 であり、`tenc` のパターン値を上書きしてしまう。
#[test]
fn muxer_rejects_kid_rotation_with_pattern_tenc() {
    // `create_encv_sample_entry()` の `tenc` は `cbcs` かつパターン値 (1, 9) を持つ
    let sample = encrypted_video_sample(
        create_encv_sample_entry(320, 240),
        Some(kid_encryption_info(0x11, Some(ROTATED_KID_A))),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("パターン暗号化パラメーターを持つトラックの KID 切り替えを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("pattern encryption parameters"),
                "パターン暗号化パラメーターの検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// `tenc.default_per_sample_iv_size` が 0 (constant IV) のトラックに `kid: Some` を
/// 渡すと InvalidInput になること
#[test]
fn muxer_rejects_kid_rotation_with_constant_iv() {
    let sample = encrypted_video_sample(
        create_encv_sample_entry_with_tenc(1, 0, Some(vec![0x77; 8])),
        Some(SampleEncryptionInfo {
            iv: Vec::new(),
            subsamples: Vec::new(),
            kid: Some(ROTATED_KID_A),
        }),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("constant IV のトラックの KID 切り替えを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("per-sample IV"),
                "per-sample IV の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// `tenc` を持たないトラックに `kid: Some` を渡すと InvalidInput になること
#[test]
fn muxer_rejects_kid_rotation_without_tenc() {
    let sample = encrypted_video_sample(
        create_avc1_sample_entry(320, 240),
        Some(kid_encryption_info(0x11, Some(ROTATED_KID_A))),
        0,
        16,
    );
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let err = muxer
        .create_media_segment_metadata(&[sample])
        .expect_err("tenc を持たないトラックの KID 切り替えを受け付けてしまった");
    assert!(
        matches!(err, MuxError::EncodeError(_)),
        "EncodeError を期待したが {err:?} だった"
    );
}

/// mux → demux の往復で、サンプルごとの実効 KID が一致すること
///
/// `Some(tenc.default_kid)` を入力すると `None` に正規化されることも確認する。
#[test]
fn kid_rotation_roundtrip_preserves_effective_kid() {
    let sample_entry = create_encv_sample_entry_with_tenc(1, 16, None);
    let samples = [
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x11, Some(ROTATED_KID_A))),
            0,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x22, None)),
            16,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x33, Some(VIDEO_KID))),
            32,
            16,
        ),
        encrypted_video_sample(
            sample_entry,
            Some(kid_encryption_info(0x44, Some(ROTATED_KID_B))),
            48,
            16,
        ),
    ];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("KID が切り替わるメディアセグメントの作成に失敗した");
    // demux が mdat の範囲を検査できるよう、サンプルデータ分の payload を付ける
    media_segment.extend_from_slice(&[0u8; 64]);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let decoded_samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("KID が切り替わるメディアセグメントの処理に失敗した");
    let kids: Vec<Option<[u8; 16]>> = decoded_samples
        .iter()
        .map(|sample| sample.encryption_info.as_ref().and_then(|info| info.kid))
        .collect();
    assert_eq!(
        kids,
        vec![Some(ROTATED_KID_A), None, None, Some(ROTATED_KID_B)],
        "mux → demux の往復で実効 KID が一致しない (Some(default_kid) の正規化を含む)"
    );
}

/// `tenc` version 1 でパターン暗号化パラメーターが 0 / 0 のトラックでも、KID を
/// 切り替えると `sgpd` / `sbgp` を書き出し、demux で実効 KID が一致すること
///
/// `tenc` version 1 の 0 / 0 はパターン暗号化を使わないことを表すため、`seig` の
/// 0 / 0 に上書きされても復号結果は変わらない。mux がパターン値の有無ではなく値で
/// 判定し、demux と同じ基準で KID の切り替えを受理することを確認する。
#[test]
fn muxer_writes_sample_groups_for_zero_pattern_tenc() {
    let sample_entry = create_encv_sample_entry_with_zero_pattern_tenc();
    let samples = [
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x11, Some(ROTATED_KID_A))),
            0,
            16,
        ),
        encrypted_video_sample(
            sample_entry.clone(),
            Some(kid_encryption_info(0x22, Some(ROTATED_KID_B))),
            16,
            16,
        ),
        encrypted_video_sample(sample_entry, Some(kid_encryption_info(0x33, None)), 32, 16),
    ];
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let mut media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("パターン値 0 / 0 のトラックの KID 切り替えに失敗した");
    let (moof_box, _) =
        MoofBox::decode(&media_segment).expect("メディアセグメントからの moof デコードに失敗した");
    assert_eq!(moof_box.traf_boxes.len(), 1, "テストは 1 traf を前提にする");
    let traf_box = &moof_box.traf_boxes[0];
    assert!(
        has_traf_unknown_box(traf_box, *b"sgpd"),
        "KID が切り替わるのに sgpd を書き出していない"
    );
    assert!(
        has_traf_unknown_box(traf_box, *b"sbgp"),
        "KID が切り替わるのに sbgp を書き出していない"
    );

    // demux が mdat の範囲を検査できるよう、サンプルデータ分の payload を付ける
    media_segment.extend_from_slice(&[0u8; 48]);
    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer
        .handle_init_segment(&init_segment)
        .expect("init セグメントの処理に失敗した");
    let decoded_samples = demuxer
        .handle_media_segment(&media_segment)
        .expect("KID が切り替わるメディアセグメントの処理に失敗した");
    let kids: Vec<Option<[u8; 16]>> = decoded_samples
        .iter()
        .map(|sample| sample.encryption_info.as_ref().and_then(|info| info.kid))
        .collect();
    assert_eq!(
        kids,
        vec![Some(ROTATED_KID_A), Some(ROTATED_KID_B), None],
        "パターン値 0 / 0 のトラックで mux → demux の実効 KID が一致しない"
    );
}

// ===== styp (セグメント種別ボックス) のテスト =====

/// テスト用の `styp` を組み立てる
///
/// ISO/IEC 14496-12:2022 8.16.2 の `styp` は、`ftyp` と同じく
/// major_brand / minor_version / compatible_brands を持つ。
fn create_styp() -> StypBox {
    StypBox {
        major_brand: Brand::ISO6,
        minor_version: 0,
        compatible_brands: vec![Brand::ISO6, Brand::new(*b"cmff")],
    }
}

/// メディアセグメント先頭から `moof` ボックスのバイト位置を求める
///
/// `styp` / `sidx` などの読み飛ばすボックスが前にあっても、`moof` の位置を返す。
/// 位置の計算を muxer の実装から独立させるため、`BoxHeader` を順にデコードして求める。
fn find_moof_offset(segment: &[u8]) -> usize {
    let mut offset = 0;
    loop {
        let (header, _) = BoxHeader::decode(&segment[offset..])
            .expect("メディアセグメントのボックスヘッダーのデコードに失敗した");
        if header.box_type == MoofBox::TYPE {
            return offset;
        }
        offset += usize::try_from(header.box_size.get()).expect("ボックスサイズは usize に収まる");
    }
}

/// `styp` を指定した `create_media_segment_metadata` の出力順が `styp` + `moof` + `mdat` ヘッダーになること
#[test]
fn create_media_segment_metadata_writes_styp_first() {
    let styp = create_styp();
    let styp_size = styp
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない")
        .len();
    let options = SegmentMuxerOptions {
        styp: Some(styp.clone()),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let samples = [video_sample_with_timing(entry, 3000, None, 0, 16)];

    let mut segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("styp 付きのメディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);

    // 先頭は styp ボックスである
    let (decoded_styp, decoded_styp_size) =
        StypBox::decode(&segment).expect("セグメント先頭からの styp デコードに失敗した");
    assert_eq!(
        decoded_styp_size, styp_size,
        "styp の消費バイト数がエンコード結果の長さと一致しない"
    );
    assert_eq!(
        decoded_styp, styp,
        "出力された styp が指定した値と一致しない"
    );

    // styp の直後は moof ボックスである
    let (_, moof_size) =
        MoofBox::decode(&segment[styp_size..]).expect("styp の直後からの moof デコードに失敗した");

    // moof の直後は mdat ヘッダーである
    let (mdat_header, _) = BoxHeader::decode(&segment[styp_size + moof_size..])
        .expect("moof の直後からの mdat ヘッダーデコードに失敗した");
    assert_eq!(
        mdat_header.box_type,
        BoxType::Normal(*b"mdat"),
        "moof の直後が mdat ヘッダーでない"
    );
}

/// `styp` を指定した `create_media_segment_metadata_with_sidx` の出力順が
/// `styp` + `sidx` + `moof` + `mdat` ヘッダーになること
#[test]
fn create_media_segment_metadata_with_sidx_writes_styp_first() {
    let styp = create_styp();
    let styp_size = styp
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない")
        .len();
    let options = SegmentMuxerOptions {
        styp: Some(styp.clone()),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let samples = [video_sample_with_timing(entry, 3000, None, 0, 16)];

    let mut segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("styp 付きの sidx 入りメディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);

    // 先頭は styp ボックスである
    let (decoded_styp, decoded_styp_size) =
        StypBox::decode(&segment).expect("セグメント先頭からの styp デコードに失敗した");
    assert_eq!(
        decoded_styp_size, styp_size,
        "styp の消費バイト数がエンコード結果の長さと一致しない"
    );
    assert_eq!(
        decoded_styp, styp,
        "出力された styp が指定した値と一致しない"
    );

    // styp の直後は sidx ボックスである
    let (sidx_box, sidx_size) =
        SidxBox::decode(&segment[styp_size..]).expect("styp の直後からの sidx デコードに失敗した");

    // sidx の直後は moof ボックスである
    let (_, moof_size) = MoofBox::decode(&segment[styp_size + sidx_size..])
        .expect("sidx の直後からの moof デコードに失敗した");

    // moof の直後は mdat ヘッダーである
    let (mdat_header, _) = BoxHeader::decode(&segment[styp_size + sidx_size + moof_size..])
        .expect("moof の直後からの mdat ヘッダーデコードに失敗した");
    assert_eq!(
        mdat_header.box_type,
        BoxType::Normal(*b"mdat"),
        "moof の直後が mdat ヘッダーでない"
    );

    // sidx の参照範囲は sidx より後ろ（moof + mdat）だけであり、styp を含めない
    let reference = sidx_box
        .references
        .first()
        .expect("sidx の参照が 1 つもない");
    assert_eq!(
        usize::try_from(reference.referenced_size).expect("referenced_size は usize に収まる"),
        segment.len() - (styp_size + sidx_size),
        "sidx.referenced_size が moof + mdat の合計と一致しない"
    );
    assert_eq!(
        sidx_box.first_offset, 0,
        "sidx の直後が moof であるため first_offset は 0 になる"
    );
}

/// `styp` を指定しない場合の出力が既存どおり `moof`（または `sidx`）から始まること
///
/// 既定値 (`styp: None`) では `styp` を書き出さないため、既存の出力バイト列は変わらない。
/// `styp` が先頭に混入していれば `MoofBox` / `SidxBox` のデコードが種別違いで失敗する。
#[test]
fn create_media_segment_metadata_without_styp_does_not_write_styp() {
    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let entry = create_avc1_sample_entry(320, 240);
    let samples = [video_sample_with_timing(entry, 3000, None, 0, 16)];

    let mut segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("メディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);
    let (_, moof_size) =
        MoofBox::decode(&segment).expect("styp を指定しないセグメントが moof から始まっていない");
    let (mdat_header, _) = BoxHeader::decode(&segment[moof_size..])
        .expect("moof の直後からの mdat ヘッダーデコードに失敗した");
    assert_eq!(
        mdat_header.box_type,
        BoxType::Normal(*b"mdat"),
        "moof の直後が mdat ヘッダーでない"
    );
    assert_eq!(
        segment.len(),
        moof_size + BoxHeader::MIN_SIZE + 16,
        "styp を指定しない場合の出力が moof + mdat ヘッダー + payload だけになっていない"
    );

    let mut segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("sidx 入りメディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);
    let (sidx_box, sidx_size) =
        SidxBox::decode(&segment).expect("styp を指定しないセグメントが sidx から始まっていない");
    let (_, moof_size) =
        MoofBox::decode(&segment[sidx_size..]).expect("sidx の直後からの moof デコードに失敗した");
    let (mdat_header, _) = BoxHeader::decode(&segment[sidx_size + moof_size..])
        .expect("moof の直後からの mdat ヘッダーデコードに失敗した");
    assert_eq!(
        mdat_header.box_type,
        BoxType::Normal(*b"mdat"),
        "moof の直後が mdat ヘッダーでない"
    );
    let reference = sidx_box
        .references
        .first()
        .expect("sidx の参照が 1 つもない");
    assert_eq!(
        usize::try_from(reference.referenced_size).expect("referenced_size は usize に収まる"),
        segment.len() - sidx_size,
        "sidx.referenced_size が moof + mdat の合計と一致しない"
    );
    assert_eq!(
        sidx_box.first_offset, 0,
        "sidx の直後が moof であるため first_offset は 0 になる"
    );
}

/// `styp` を指定したとき、`sidx` あり/なしのどちらでも
/// `mfra` の `tfra.moof_offset` が実際の `moof` 位置と一致すること
#[test]
fn mfra_moof_offset_matches_actual_position_with_styp() {
    let styp = create_styp();
    let styp_size = styp
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない")
        .len();
    let options = SegmentMuxerOptions {
        styp: Some(styp),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let entry = create_avc1_sample_entry(320, 240);

    // `sidx` なし、`sidx` ありの順に 2 セグメントを作り、
    // セグメント先頭からの実際の `moof` 位置とセグメント全体のサイズを記録する
    let mut segments = Vec::new();
    let mut moof_offsets = Vec::new();
    for with_sidx in [false, true] {
        let samples = [video_sample_with_timing(entry.clone(), 3000, None, 0, 16)];
        let mut segment = if with_sidx {
            muxer
                .create_media_segment_metadata_with_sidx(&samples)
                .expect("sidx 入りメディアセグメントの作成に失敗した")
        } else {
            muxer
                .create_media_segment_metadata(&samples)
                .expect("メディアセグメントの作成に失敗した")
        };
        // `mdat` payload まで含めた完全なメディアセグメントにする
        segment.extend_from_slice(&[0u8; 16]);

        // `styp` は常に先頭にある
        let (_, decoded_styp_size) =
            StypBox::decode(&segment).expect("セグメント先頭からの styp デコードに失敗した");
        assert_eq!(
            decoded_styp_size, styp_size,
            "styp の消費バイト数がエンコード結果の長さと一致しない"
        );

        let moof_offset = find_moof_offset(&segment);
        if with_sidx {
            // `styp` と `moof` の間には `sidx` がある
            let (_, sidx_size) = SidxBox::decode(&segment[styp_size..])
                .expect("styp の直後からの sidx デコードに失敗した");
            assert_eq!(
                moof_offset,
                styp_size + sidx_size,
                "sidx ありの場合の moof 位置が styp + sidx の直後になっていない"
            );
        } else {
            // `styp` の直後が `moof` である
            assert_eq!(
                moof_offset, styp_size,
                "sidx なしの場合の moof 位置が styp の直後になっていない"
            );
        }

        moof_offsets.push(moof_offset);
        segments.push(segment);
    }

    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    let mfra = muxer.mfra_bytes().expect("mfra の作成に失敗した");
    let (mfra_box, _) = MfraBox::decode(&mfra).expect("mfra のデコードに失敗した");

    // 映像トラック 1 本なので tfra は 1 つで、エントリー数はセグメント数と一致する
    assert_eq!(mfra_box.tfra_boxes.len(), 1, "tfra の数が 1 でない");
    let entries = &mfra_box.tfra_boxes[0].entries;
    assert_eq!(
        entries.len(),
        segments.len(),
        "tfra のエントリー数がセグメント数と一致しない"
    );

    // tfra.moof_offset は、init セグメントの後ろに各セグメントを並べたときの実際の moof 位置になる
    let mut segment_head = u64::try_from(init_segment.len()).expect("init サイズは u64 に収まる");
    for (index, ((entry, segment), moof_offset)) in entries
        .iter()
        .zip(segments.iter())
        .zip(moof_offsets.iter())
        .enumerate()
    {
        let expected =
            segment_head + u64::try_from(*moof_offset).expect("moof 位置は u64 に収まる");
        assert_eq!(
            entry.moof_offset, expected,
            "{index} 番目のセグメントの tfra.moof_offset が実際の moof 位置と一致しない"
        );
        segment_head += u64::try_from(segment.len()).expect("セグメントサイズは u64 に収まる");
    }
}

// ===== CMAF (構造ブランド) のテスト =====

/// CMAF の往復テスト用に、`keyframe` と CTO を指定できる映像サンプルを組み立てる
///
/// `video_sample_with_timing()` は常に `keyframe = true` を設定するため、
/// 往復で `keyframe` が保たれることを確認できるよう、このテスト専用に用意する。
fn cmaf_video_sample(
    sample_entry: SampleEntry,
    duration: u32,
    keyframe: bool,
    composition_time_offset: Option<i64>,
    data_offset: u64,
    data_size: usize,
) -> Sample {
    Sample {
        encryption_info: None,
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("タイムスケールは非ゼロ"),
        sample_entry: Some(sample_entry),
        duration,
        keyframe,
        composition_time_offset,
        data_offset,
        data_size,
    }
}

/// `cmaf: Some(Cmfc)` を指定した場合の `ftyp` が CMAF の構造ブランドになり、
/// 同じセグメントの `styp` に観測したコーデックブランドが入ること
///
/// `major_brand` が `cmfc`、`minor_version` が 0 になり、互換 brand に従来の
/// `iso5` / `iso6` / `mp41` と `cmfc`、および観測した映像のコーデックブランドが並ぶことを確認する。
/// `isom` / `avc1` は `default_base_is_moof` の制約により含めない。
/// このテストは `encv` の `frma` が `hev1` のサンプルエントリーを使うため、
/// メディアセグメントの生成時点で初めて観測するコーデックブランド (`hev1`) が
/// `styp` にも入ることを確認できる。
#[test]
fn cmaf_ftyp_uses_cmfc_brand() {
    let options = SegmentMuxerOptions {
        cmaf: Some(CmafTrackFileBrand::Cmfc),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");

    // `encv` の `frma` が `hev1` なので、映像のコーデックブランドとして `hev1` が加わる
    let samples = [drm_video_sample(create_encv_sample_entry(320, 240), 0, 16)];
    let media_segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("CMAF 指定のメディアセグメントの作成に失敗した");

    let init_segment = muxer
        .init_segment_bytes()
        .expect("CMAF 指定の init セグメントの作成に失敗した");
    let (ftyp_box, _) =
        FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");

    assert_eq!(
        ftyp_box.major_brand,
        Brand::CMFC,
        "CMAF 指定時の major_brand が cmfc でない"
    );
    assert_eq!(
        ftyp_box.minor_version, 0,
        "CMAF 指定時の minor_version が 0 でない"
    );
    assert_eq!(
        ftyp_box.compatible_brands,
        vec![
            Brand::ISO5,
            Brand::ISO6,
            Brand::MP41,
            Brand::CMFC,
            Brand::HEV1,
        ],
        "CMAF 指定時の互換 brand が期待値と一致しない"
    );
    assert!(
        !ftyp_box.compatible_brands.contains(&Brand::ISOM)
            && !ftyp_box.compatible_brands.contains(&Brand::AVC1),
        "default_base_is_moof と互換でない isom / avc1 が互換 brand に含まれている: {:?}",
        ftyp_box.compatible_brands
    );

    // `styp` の互換 brand は、その時点の `ftyp` の互換 brand に `major_brand` の `cmfs` を
    // 加えた集合になる。`styp` を `ftyp` と同じ集合で組み立てていたり、組み立て時に
    // このセグメントで初めて観測するサンプルエントリーを渡していなかったりすると、ここで落ちる
    let (styp_box, _) = StypBox::decode(&media_segment)
        .expect("メディアセグメント先頭からの styp デコードに失敗した");
    let mut expected_styp_brands = vec![Brand::CMFS];
    expected_styp_brands.extend_from_slice(&ftyp_box.compatible_brands);
    assert_eq!(
        styp_box.compatible_brands, expected_styp_brands,
        "CMAF 指定時の styp の互換 brand が ftyp の互換 brand に cmfs を加えた集合と一致しない"
    );
    assert!(
        styp_box.compatible_brands.contains(&Brand::HEV1),
        "ftyp 生成後に観測したコーデックブランド hev1 が styp の互換 brand に含まれていない: {:?}",
        styp_box.compatible_brands
    );
}

/// `cmaf: Some(Cmfc)` を指定した場合の `styp` が CMAF Segment のブランドになり、
/// 互換 brand が `ftyp` の互換 brand に `cmfs` を加えた集合になること
///
/// `sidx` なしでは `styp` の直後が `moof`、`sidx` ありでは `styp` + `sidx` + `moof` に
/// なることも確認する。`sidx.referenced_size` は `styp` を含めない。
#[test]
fn cmaf_styp_uses_cmfs_brand_matching_ftyp() {
    let options = SegmentMuxerOptions {
        cmaf: Some(CmafTrackFileBrand::Cmfc),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let samples = [video_sample_with_timing(
        create_avc1_sample_entry(320, 240),
        3000,
        None,
        0,
        16,
    )];

    // `sidx` なし: `styp` + `moof` + `mdat` ヘッダーの順になる
    let mut segment = muxer
        .create_media_segment_metadata(&samples)
        .expect("CMAF 指定のメディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);
    let (styp_box, styp_size) =
        StypBox::decode(&segment).expect("セグメント先頭からの styp デコードに失敗した");
    assert_eq!(
        styp_box.major_brand,
        Brand::CMFS,
        "CMAF 指定時の styp の major_brand が cmfs でない"
    );
    assert_eq!(
        styp_box.minor_version, 0,
        "CMAF 指定時の styp の minor_version が 0 でない"
    );
    MoofBox::decode(&segment[styp_size..]).expect("styp の直後からの moof デコードに失敗した");

    // `styp` の互換 brand は、その時点の `ftyp` の互換 brand に `major_brand` の `cmfs` を
    // 加えた集合になる
    let init_segment = muxer
        .init_segment_bytes()
        .expect("CMAF 指定の init セグメントの作成に失敗した");
    let (ftyp_box, _) =
        FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let mut expected_styp_brands = vec![Brand::CMFS];
    expected_styp_brands.extend_from_slice(&ftyp_box.compatible_brands);
    assert_eq!(
        styp_box.compatible_brands, expected_styp_brands,
        "styp の互換 brand が ftyp の互換 brand に cmfs を加えた集合と一致しない"
    );

    // `sidx` あり: `styp` + `sidx` + `moof` + `mdat` ヘッダーの順になる
    let mut segment = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("CMAF 指定の sidx 入りメディアセグメントの作成に失敗した");
    // `mdat` payload まで含めた完全なメディアセグメントにする
    segment.extend_from_slice(&[0u8; 16]);
    let (styp_box, styp_size) =
        StypBox::decode(&segment).expect("セグメント先頭からの styp デコードに失敗した");
    assert_eq!(
        styp_box.compatible_brands, expected_styp_brands,
        "sidx ありの styp の互換 brand が ftyp の互換 brand に cmfs を加えた集合と一致しない"
    );
    let (sidx_box, sidx_size) =
        SidxBox::decode(&segment[styp_size..]).expect("styp の直後からの sidx デコードに失敗した");
    MoofBox::decode(&segment[styp_size + sidx_size..])
        .expect("sidx の直後からの moof デコードに失敗した");
    let reference = sidx_box
        .references
        .first()
        .expect("sidx の参照が 1 つもない");
    assert_eq!(
        usize::try_from(reference.referenced_size).expect("referenced_size は usize に収まる"),
        segment.len() - (styp_size + sidx_size),
        "sidx.referenced_size が moof + mdat の合計と一致しない"
    );
}

/// `cmaf` を指定しない場合の出力が既存と一致すること
///
/// `new()` で作った muxer と、`cmaf: None` を明示した muxer の出力バイト列を比較する。
/// 併せて `ftyp` の `major_brand` が従来の `iso5` のままで、`styp` が先頭に混入しないことも確認する。
#[test]
fn cmaf_none_keeps_output_unchanged() {
    let samples = [video_sample_with_timing(
        create_avc1_sample_entry(320, 240),
        3000,
        None,
        0,
        16,
    )];
    let mut default_muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
    let options = SegmentMuxerOptions {
        cmaf: None,
        ..Default::default()
    };
    let mut explicit_muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");

    // メディアセグメント (sidx なし) のバイト列が一致する
    let default_segment = default_muxer
        .create_media_segment_metadata(&samples)
        .expect("メディアセグメントの作成に失敗した");
    let explicit_segment = explicit_muxer
        .create_media_segment_metadata(&samples)
        .expect("メディアセグメントの作成に失敗した");
    assert_eq!(
        default_segment, explicit_segment,
        "cmaf を明示しない場合のメディアセグメントが new() の出力と変わった"
    );

    // メディアセグメント (sidx あり) のバイト列が一致する
    let default_sidx_segment = default_muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("sidx 入りメディアセグメントの作成に失敗した");
    let explicit_sidx_segment = explicit_muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect("sidx 入りメディアセグメントの作成に失敗した");
    assert_eq!(
        default_sidx_segment, explicit_sidx_segment,
        "cmaf を明示しない場合の sidx 入りメディアセグメントが new() の出力と変わった"
    );

    // init セグメントのバイト列が一致し、major_brand も従来どおりになる
    let default_init = default_muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    let explicit_init = explicit_muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");
    assert_eq!(
        default_init, explicit_init,
        "cmaf を明示しない場合の init セグメントが new() の出力と変わった"
    );
    let (ftyp_box, _) =
        FtypBox::decode(&explicit_init).expect("init セグメントからの ftyp デコードに失敗した");
    assert_eq!(
        ftyp_box.major_brand,
        Brand::ISO5,
        "cmaf 未指定時の major_brand が iso5 でない"
    );

    // `styp` が先頭に混入していない
    MoofBox::decode(&explicit_segment)
        .expect("cmaf 未指定時のメディアセグメントが moof から始まっていない");
}

/// `cmaf: Some(Cmfc)` を指定して複数の `TrackKind` を観測しようとすると
/// `MuxError::EncodeError` になり、状態が変わらないこと
///
/// 別々のセグメントで 2 種類目の `TrackKind` を観測する場合を確認する。
/// エラー後に同じ映像トラックのセグメントを作れることと、init セグメントが変わらないことで
/// 内部状態が保たれていることを確かめる。
#[test]
fn cmaf_rejects_second_track_kind() {
    let options = SegmentMuxerOptions {
        cmaf: Some(CmafTrackFileBrand::Cmfc),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let video_samples = [video_sample_with_timing(
        create_avc1_sample_entry(320, 240),
        3000,
        None,
        0,
        16,
    )];
    muxer
        .create_media_segment_metadata(&video_samples)
        .expect("CMAF 指定の映像セグメントの作成に失敗した");
    let init_before = muxer
        .init_segment_bytes()
        .expect("CMAF 指定の init セグメントの作成に失敗した");

    // 音声トラックを観測すると、観測済みの TrackKind が 2 種類になる
    let audio_samples = [audio_sample_with_timing(
        create_opus_sample_entry(),
        960,
        None,
        0,
        8,
    )];
    let err = muxer
        .create_media_segment_metadata(&audio_samples)
        .expect_err("CMAF 指定で 2 種類目の TrackKind を受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("cmaf requires a single media track kind"),
                "複数 TrackKind の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }

    // エラーを返した呼び出しで状態が変わっていない
    assert_eq!(
        muxer
            .init_segment_bytes()
            .expect("エラー後の init セグメントの作成に失敗した"),
        init_before,
        "エラーを返した呼び出しで init セグメントが変わった"
    );
    muxer
        .create_media_segment_metadata(&video_samples)
        .expect("エラーを返した呼び出しで muxer の状態が変わった");
}

/// `cmaf: Some(Cmfc)` を指定して 1 つの `moof` に複数の `traf` を書くことになる場合は
/// `MuxError::EncodeError` になり、状態が変わらないこと
///
/// 1 つのセグメントに映像と音声を混在させる場合を確認する。
/// エラー後に `init_segment_bytes()` が `EmptyTracks` を返すことで、
/// トラックが 1 本も登録されていないことを確かめる。
#[test]
fn cmaf_rejects_multiple_trafs_in_one_segment() {
    let options = SegmentMuxerOptions {
        cmaf: Some(CmafTrackFileBrand::Cmfc),
        ..Default::default()
    };
    let mut muxer =
        Fmp4SegmentMuxer::with_options(options).expect("Fmp4SegmentMuxer::with_options に失敗した");
    let video_size = 16usize;
    let samples = [
        video_sample_with_timing(
            create_avc1_sample_entry(320, 240),
            3000,
            None,
            0,
            video_size,
        ),
        audio_sample_with_timing(create_opus_sample_entry(), 960, None, video_size as u64, 8),
    ];
    let err = muxer
        .create_media_segment_metadata(&samples)
        .expect_err("CMAF 指定で複数の traf を書くセグメントを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("cmaf requires one traf per moof"),
                "traf 数の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }

    // `sidx` 付きの生成経路でも同じ検査が働く
    let err = muxer
        .create_media_segment_metadata_with_sidx(&samples)
        .expect_err("CMAF 指定で複数の traf を書く sidx 入りセグメントを受け付けてしまった");
    match err {
        MuxError::EncodeError(error) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error.reason.contains("cmaf requires one traf per moof"),
                "traf 数の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        other => panic!("EncodeError を期待したが {other:?} だった"),
    }

    // エラーを返した呼び出しでトラックが登録されていない
    assert!(
        matches!(muxer.init_segment_bytes(), Err(MuxError::EmptyTracks)),
        "エラーを返した呼び出しでトラックが登録された"
    );
}

/// `cmaf` と `styp` を同時に指定すると `MuxError::EncodeError` になること
///
/// どちらを優先するかを黙って決めないよう、muxer の生成時点で拒否する。
#[test]
fn cmaf_and_styp_cannot_both_be_specified() {
    let options = SegmentMuxerOptions {
        cmaf: Some(CmafTrackFileBrand::Cmfc),
        styp: Some(create_styp()),
        ..Default::default()
    };
    match Fmp4SegmentMuxer::with_options(options) {
        Err(MuxError::EncodeError(error)) => {
            assert_eq!(
                error.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                error.reason,
            );
            assert!(
                error
                    .reason
                    .contains("cmaf and styp cannot both be specified"),
                "cmaf と styp の同時指定の検査とは別のエラーである (reason={})",
                error.reason,
            );
        }
        Ok(_) => panic!("cmaf と styp を同時に指定した muxer を生成できてしまった"),
        Err(other) => panic!("EncodeError を期待したが {other:?} だった"),
    }
}

/// CMAF 指定で生成したセグメントを `Fmp4SegmentDemuxer` で読み戻し、
/// `styp()` とサンプルが入力と一致すること
///
/// `composition_time_offset` は、muxer が 1 つでも `Some` があると全サンプルを `Some` に
/// 正規化するため、全 `Some` と全 `None` の 2 通りで往復を確認する。
/// `data_offset` の数値は muxer と demuxer で基準が異なるため比較せず、
/// `data_offset` が指すバイト列を payload と比較する。
#[test]
fn cmaf_segment_roundtrips_through_segment_demuxer() {
    let payloads: [&[u8]; 2] = [b"first", b"second"];
    let sample_entry = create_avc1_sample_entry(320, 240);

    for with_composition_time_offset in [false, true] {
        let composition_time_offset = with_composition_time_offset.then_some(3000i64);
        let samples = [
            cmaf_video_sample(
                sample_entry.clone(),
                3000,
                true,
                composition_time_offset,
                0,
                payloads[0].len(),
            ),
            cmaf_video_sample(
                sample_entry.clone(),
                3000,
                false,
                composition_time_offset,
                payloads[0].len() as u64,
                payloads[1].len(),
            ),
        ];
        let options = SegmentMuxerOptions {
            cmaf: Some(CmafTrackFileBrand::Cmfc),
            ..Default::default()
        };
        let mut muxer = Fmp4SegmentMuxer::with_options(options)
            .expect("Fmp4SegmentMuxer::with_options に失敗した");

        // メディアセグメントに payload を連結して完全なセグメントにする
        let mut media_segment = muxer
            .create_media_segment_metadata(&samples)
            .expect("CMAF 指定のメディアセグメントの作成に失敗した");
        media_segment.extend_from_slice(payloads[0]);
        media_segment.extend_from_slice(payloads[1]);
        let init_segment = muxer
            .init_segment_bytes()
            .expect("CMAF 指定の init セグメントの作成に失敗した");
        let (ftyp_box, _) =
            FtypBox::decode(&init_segment).expect("init セグメントからの ftyp デコードに失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_segment)
            .expect("CMAF 指定の init セグメントの処理に失敗した");
        let demuxed_samples = demuxer
            .handle_media_segment(&media_segment)
            .expect("CMAF 指定のメディアセグメントの処理に失敗した");

        assert_eq!(
            demuxed_samples.len(),
            samples.len(),
            "読み戻したサンプル数が入力と一致しない"
        );
        for (index, (demuxed, sample)) in demuxed_samples.iter().zip(samples.iter()).enumerate() {
            assert_eq!(
                demuxed.data_size, sample.data_size,
                "{index} 番目のサンプルの data_size が一致しない"
            );
            assert_eq!(
                demuxed.keyframe, sample.keyframe,
                "{index} 番目のサンプルの keyframe が一致しない"
            );
            assert_eq!(
                demuxed.composition_time_offset, sample.composition_time_offset,
                "{index} 番目のサンプルの composition_time_offset が一致しない"
            );

            // `data_offset` が指すバイト列が入力の payload と一致する
            let start =
                usize::try_from(demuxed.data_offset).expect("data_offset は usize に収まる");
            let end = start
                .checked_add(demuxed.data_size)
                .expect("data_offset + data_size が usize を超えない");
            assert_eq!(
                &media_segment[start..end],
                payloads[index],
                "{index} 番目のサンプルの data_offset が指すバイト列が payload と一致しない"
            );
        }

        // 先頭の `styp` が CMAF Segment のブランドで、互換 brand が `ftyp` の互換 brand に
        // `cmfs` を加えた集合である。
        // `handle_media_segment()` が返すサンプルは demuxer を借用しているため、
        // サンプルの比較が終わった後に `styp()` を呼ぶ
        let styp_box = demuxer
            .styp()
            .expect("CMAF 指定で生成したセグメントの styp を取得できない");
        assert_eq!(
            styp_box.major_brand,
            Brand::CMFS,
            "読み戻した styp の major_brand が cmfs でない"
        );
        assert_eq!(
            styp_box.minor_version, 0,
            "読み戻した styp の minor_version が 0 でない"
        );
        let mut expected_styp_brands = vec![Brand::CMFS];
        expected_styp_brands.extend_from_slice(&ftyp_box.compatible_brands);
        assert_eq!(
            styp_box.compatible_brands, expected_styp_brands,
            "読み戻した styp の互換 brand が ftyp の互換 brand に cmfs を加えた集合と一致しない"
        );
    }
}
