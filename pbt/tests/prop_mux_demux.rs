//! Mux → Demux Roundtrip の Property-Based Testing
//!
//! Mp4FileMuxer で作成したデータを Mp4FileDemuxer で読み取り、
//! 元のデータと一致することを確認するテスト

use std::num::NonZeroU32;

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, TrackKind, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, Brand, DopsBox, FtypBox,
        HdlrBox, Hev1Box, Hvc1Box, HvccBox, MoovBox, OpusBox, SampleEntry, StppBox,
        VisualSampleEntryFields,
    },
    demux::{Input, Mp4FileDemuxer},
    mux::{
        FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, Sample, estimate_maximum_moov_box_size,
    },
};

mod helpers;

/// noprop の `sample_usize_in` で長さを引いてから要素を生成するベクタサンプラー
fn sample_vec<T>(
    ctx: &mut TestCaseContext,
    range: std::ops::Range<usize>,
    mut elem: impl FnMut(&mut TestCaseContext) -> T,
) -> Vec<T> {
    let len = noprop::sample_usize_in(ctx, range);
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(elem(ctx));
    }
    result
}

/// テスト用の H.264 SampleEntry を作成
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
            avc_profile_indication: 66, // Baseline Profile
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

/// テスト用の Hev1 SampleEntry を作成
fn create_hev1_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Hev1(Hev1Box {
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
        hvcc_box: create_hvcc_box(),
        unknown_boxes: vec![],
    })
}

/// テスト用の Hvc1 SampleEntry を作成
fn create_hvc1_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Hvc1(Hvc1Box {
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
        hvcc_box: create_hvcc_box(),
        unknown_boxes: vec![],
    })
}

/// テスト用の AV1 SampleEntry を作成
fn create_av01_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Av01(Av01Box {
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
        av1c_box: Av1cBox {
            seq_profile: Uint::new(0),
            seq_level_idx_0: Uint::new(0),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(0),
            twelve_bit: Uint::new(0),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        },
        unknown_boxes: vec![],
    })
}

fn create_hvcc_box() -> HvccBox {
    HvccBox {
        general_profile_space: Uint::new(0),
        general_tier_flag: Uint::new(0),
        general_profile_idc: Uint::new(1),
        general_profile_compatibility_flags: 0,
        general_constraint_indicator_flags: Uint::new(0),
        general_level_idc: 93,
        min_spatial_segmentation_idc: Uint::new(0),
        parallelism_type: Uint::new(0),
        chroma_format_idc: Uint::new(1),
        bit_depth_luma_minus8: Uint::new(0),
        bit_depth_chroma_minus8: Uint::new(0),
        avg_frame_rate: 0,
        constant_frame_rate: Uint::new(0),
        num_temporal_layers: Uint::new(1),
        temporal_id_nested: Uint::new(0),
        length_size_minus_one: Uint::new(3),
        nalu_arrays: vec![],
    }
}

/// テスト用の Opus SampleEntry を作成
fn create_opus_sample_entry(channel_count: u8) -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: channel_count as u16,
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(48000u16, 0),
        },
        dops_box: DopsBox {
            output_channel_count: channel_count,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        },
        unknown_boxes: vec![],
    })
}

/// テスト用の Stpp（TTML）SampleEntry を作成
fn create_stpp_sample_entry() -> SampleEntry {
    SampleEntry::Stpp(StppBox {
        data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
        namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
        schema_location: Utf8String::EMPTY,
        auxiliary_mime_types: Utf8String::EMPTY,
        unknown_boxes: vec![],
    })
}

/// FinalizedBoxes からファイルデータを構築する
///
/// non-faststart の場合: ftyp | mdat_header | mdat_data | moov
fn build_file_data(
    initial_bytes: &[u8],
    finalized: &FinalizedBoxes,
    sample_data_size: usize,
) -> Vec<u8> {
    // 全体のサイズを計算（十分なサイズを確保）
    let total_size = initial_bytes.len() + sample_data_size + finalized.moov_box_size() + 1024;
    let mut file_data = vec![0u8; total_size];

    // initial bytes をコピー
    file_data[..initial_bytes.len()].copy_from_slice(initial_bytes);

    // offset_and_bytes_pairs() で各ボックスを書き込む
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let offset = offset as usize;
        file_data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    // 実際のファイルサイズにトリミング
    // moov の終端を見つける
    let mut max_end = initial_bytes.len() + sample_data_size;
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let end = offset as usize + bytes.len();
        if end > max_end {
            max_end = end;
        }
    }
    file_data.truncate(max_end);
    file_data
}

/// FinalizedBoxes からファイルデータを構築する（ギャップ領域を含む）
///
/// `regions` はファイル上のデータ配置位置とサイズのリスト。
/// サンプルデータとギャップの区別はせず、全領域の末尾位置からファイルサイズを決定する。
fn build_hybrid_file_data(
    initial_bytes: &[u8],
    finalized: &FinalizedBoxes,
    regions: &[(u64, usize)],
) -> Vec<u8> {
    // データ領域の末尾を計算
    let data_end = regions
        .iter()
        .map(|(offset, size)| *offset as usize + size)
        .max()
        .unwrap_or(initial_bytes.len());

    let total_size = data_end + finalized.moov_box_size() + 1024;
    let mut file_data = vec![0u8; total_size];

    // initial bytes をコピー
    file_data[..initial_bytes.len()].copy_from_slice(initial_bytes);

    // offset_and_bytes_pairs() で各ボックスを書き込む
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let offset = offset as usize;
        file_data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    // 実際のファイルサイズにトリミング
    let mut max_end = data_end;
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let end = offset as usize + bytes.len();
        if end > max_end {
            max_end = end;
        }
    }
    file_data.truncate(max_end);
    file_data
}

/// ビデオサンプル情報
#[derive(Debug, Clone)]
struct VideoSampleInfo {
    keyframe: bool,
    duration: u32,
    data_size: usize,
}

/// オーディオサンプル情報
#[derive(Debug, Clone)]
struct AudioSampleInfo {
    duration: u32,
    data_size: usize,
}

/// ビデオサンプル情報を生成する
fn arb_video_sample_info(ctx: &mut TestCaseContext) -> VideoSampleInfo {
    let keyframe = noprop::sample_bool(ctx);
    let duration = noprop::sample_u64_in(ctx, 1..100) as u32;
    let data_size = noprop::sample_usize_in(ctx, 100..10000);
    VideoSampleInfo {
        keyframe,
        duration,
        data_size,
    }
}

/// オーディオサンプル情報を生成する
fn arb_audio_sample_info(ctx: &mut TestCaseContext) -> AudioSampleInfo {
    let duration = noprop::sample_u64_in(ctx, 1..100) as u32;
    let data_size = noprop::sample_usize_in(ctx, 100..5000);
    AudioSampleInfo {
        duration,
        data_size,
    }
}

/// 字幕サンプル情報
#[derive(Debug, Clone)]
struct SubtitleSampleInfo {
    duration: u32,
    data_size: usize,
}

/// 字幕サンプル情報を生成する
///
/// keyframe は Sample 型の doc コメントの推奨に従い true 固定として本構造体には持たせない。
/// duration / data_size は音声サンプルと同型で、値域は字幕想定に合わせて狭める
fn arb_subtitle_sample_info(ctx: &mut TestCaseContext) -> SubtitleSampleInfo {
    let duration = noprop::sample_u64_in(ctx, 1..100) as u32;
    let data_size = noprop::sample_usize_in(ctx, 100..2000);
    SubtitleSampleInfo {
        duration,
        data_size,
    }
}

/// moov ボックスの尺に関する不変条件を検証する
///
/// `expected` は (ハンドラー種別, 入力した timescale, 入力したサンプルの尺の合計) を
/// トラックごとに並べたもの。`mdhd` には入力した値がそのまま入り、
/// `tkhd` はそれを `mvhd` の `timescale` 単位へ切り上げ換算した値になる。
///
/// demuxer は尺として `mdhd` しか読まないため、`tkhd` の単位の誤りは
/// mux → demux のラウンドトリップでは検出できない。そのため moov ボックスを直接検証する
fn assert_moov_duration_invariants(moov_box: &MoovBox, expected: &[([u8; 4], NonZeroU32, u64)]) {
    assert_eq!(
        moov_box.trak_boxes.len(),
        expected.len(),
        "出力された trak の数が想定と一致しない"
    );

    let movie_timescale = moov_box.mvhd_box.timescale.get() as u128;
    for trak_box in &moov_box.trak_boxes {
        // moov を直接見ているため demuxer の `TrackKind` が使えず、ハンドラー種別で判別する
        let handler_type = trak_box.mdia_box.hdlr_box.handler_type;
        let (_, expected_timescale, expected_duration) = expected
            .iter()
            .find(|(h, _, _)| *h == handler_type)
            .expect("想定していないハンドラー種別の trak が出力された");

        let mdhd_box = &trak_box.mdia_box.mdhd_box;
        assert_eq!(
            mdhd_box.timescale, *expected_timescale,
            "mdhd の timescale が入力の timescale と一致しない"
        );
        assert_eq!(
            mdhd_box.duration, *expected_duration,
            "mdhd の duration が入力サンプルの尺の合計と一致しない"
        );

        let expected_tkhd_duration = u64::try_from(
            (mdhd_box.duration as u128 * movie_timescale)
                .div_ceil(mdhd_box.timescale.get() as u128),
        )
        .expect("この生成範囲では換算結果は必ず u64 に収まる");
        assert_eq!(
            trak_box.tkhd_box.duration, expected_tkhd_duration,
            "tkhd の duration が mvhd の timescale 単位になっていない"
        );
    }
}

/// このファイルの主要 PBT ブロックの共通ケース数（旧 `with_cases(20)` を維持）
const CASES_MAIN: usize = 20;

/// Options で指定した language / name が mux → demux で復元される
#[test]
fn track_metadata_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let video_meta = helpers::arb_track_metadata(ctx);
        let audio_meta = helpers::arb_track_metadata(ctx);
        let subtitle_meta = helpers::arb_track_metadata(ctx);

        let options = Mp4FileMuxerOptions {
            video_track: video_meta.clone(),
            audio_track: audio_meta.clone(),
            subtitle_track: subtitle_meta.clone(),
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let mut total_data_size = 0usize;

        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(320, 240)),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
            duration: 1,
            composition_time_offset: None,
            data_offset,
            data_size: 128,
        };
        muxer
            .append_sample(&video_sample)
            .expect("video sample の追加に失敗した");
        data_offset += 128;
        total_data_size += 128;

        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry(2)),
            keyframe: true,
            timescale: NonZeroU32::new(48000).expect("timescale は非ゼロである"),
            duration: 960,
            composition_time_offset: None,
            data_offset,
            data_size: 64,
        };
        muxer
            .append_sample(&audio_sample)
            .expect("audio sample の追加に失敗した");
        data_offset += 64;
        total_data_size += 64;

        let subtitle_sample = Sample {
            track_kind: TrackKind::Subtitle,
            sample_entry: Some(create_stpp_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::new(1000).expect("timescale は非ゼロである"),
            duration: 1000,
            composition_time_offset: None,
            data_offset,
            data_size: 32,
        };
        muxer
            .append_sample(&subtitle_sample)
            .expect("subtitle sample の追加に失敗した");
        total_data_size += 32;

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        // demux 相当として、生成した moov を一度バイト列に戻してから再デコードする
        let moov_bytes = finalized
            .moov_box()
            .encode_to_vec()
            .expect("moov のエンコードに失敗した");
        let (decoded_moov, decoded_size) =
            MoovBox::decode(&moov_bytes).expect("エンコードした moov はデコードできる");
        assert_eq!(
            decoded_size,
            moov_bytes.len(),
            "moov の decode サイズがバイト列長と一致しない"
        );
        assert_eq!(decoded_moov.trak_boxes.len(), 3);
        helpers::assert_track_metadata(&decoded_moov.trak_boxes[0], &video_meta);
        helpers::assert_track_metadata(&decoded_moov.trak_boxes[1], &audio_meta);
        helpers::assert_track_metadata(&decoded_moov.trak_boxes[2], &subtitle_meta);

        // ファイルとしても demux できることを確認する
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });
        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 3);
        Ok(())
    })?;
    Ok(())
}

/// ビデオのみの Mux → Demux roundtrip
#[test]
fn mux_demux_video_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let timescale = noprop::sample_u64_in(ctx, 1..90001) as u32;
        let mut samples = sample_vec(ctx, 1..20, arb_video_sample_info);

        // 最初のサンプルは必ず keyframe にする
        if let Some(first) = samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ");

        // サンプルを追加
        let mut sample_entry = Some(create_avc1_sample_entry(width, height));
        let mut expected_samples = Vec::new();
        let mut total_data_size = 0usize;
        for sample_info in &samples {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: sample_entry.take(),
                keyframe: sample_info.keyframe,
                timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("sample の追加に失敗した");
            expected_samples.push((
                sample_info.keyframe,
                sample_info.duration,
                sample_info.data_size,
            ));
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));

        // サンプル数と属性を確認
        let mut actual_samples = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            actual_samples.push((sample.keyframe, sample.duration, sample.data_size));
        }
        assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples
            .iter()
            .zip(actual_samples.iter())
            .enumerate()
        {
            assert_eq!(expected.0, actual.0, "sample {i} で keyframe が一致しない");
            assert_eq!(expected.1, actual.1, "sample {i} で duration が一致しない");
            assert_eq!(expected.2, actual.2, "sample {i} で data_size が一致しない");
        }
        Ok(())
    })?;
    Ok(())
}

/// オーディオのみの Mux → Demux roundtrip
#[test]
fn mux_demux_audio_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let channel_count = noprop::sample_u64_in(ctx, 1..=8) as u8;
        let timescale = noprop::sample_u64_in(ctx, 1..48001) as u32;
        let samples = sample_vec(ctx, 1..30, arb_audio_sample_info);

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ");

        // サンプルを追加
        // 正規は keyframe = true だが、全 false でも空 stss を出さず省略する契約を検証するため false を固定する
        let mut sample_entry = Some(create_opus_sample_entry(channel_count));
        let mut expected_samples = Vec::new();
        let mut total_data_size = 0usize;
        for sample_info in &samples {
            let sample = Sample {
                track_kind: TrackKind::Audio,
                sample_entry: sample_entry.take(),
                keyframe: false,
                timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("sample の追加に失敗した");
            expected_samples.push((sample_info.duration, sample_info.data_size));
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");
        assert!(
            finalized.moov_box().trak_boxes[0]
                .mdia_box
                .minf_box
                .stbl_box
                .stss_box
                .is_none(),
            "全非キーフレームの音声トラックで空の stss が出力された"
        );

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Audio));

        // サンプル数と属性を確認
        // 入力は keyframe = false だが、stss 省略により demux ではすべて同期サンプルになる
        let mut actual_samples = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            assert!(
                sample.keyframe,
                "音声サンプルが同期サンプルとして復元されていない"
            );
            actual_samples.push((sample.duration, sample.data_size));
        }
        assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples
            .iter()
            .zip(actual_samples.iter())
            .enumerate()
        {
            assert_eq!(expected.0, actual.0, "sample {i} で duration が一致しない");
            assert_eq!(expected.1, actual.1, "sample {i} で data_size が一致しない");
        }
        Ok(())
    })?;
    Ok(())
}

/// 字幕のみの Mux → Demux roundtrip
#[test]
fn mux_demux_subtitle_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let timescale = noprop::sample_u64_in(ctx, 1..10001) as u32;
        let samples = sample_vec(ctx, 1..15, arb_subtitle_sample_info);

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ");

        // サンプルを追加
        let mut sample_entry = Some(create_stpp_sample_entry());
        let mut expected_samples = Vec::new();
        let mut total_data_size = 0usize;
        for sample_info in &samples {
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: sample_entry.take(),
                keyframe: true,
                timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("subtitle sample の追加に失敗した");
            expected_samples.push((sample_info.duration, sample_info.data_size));
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Subtitle));
        // 投入した timescale が mdhd 経由でそのまま復元される
        assert_eq!(tracks[0].timescale, timescale);

        // サンプル数と属性を確認
        let mut actual_samples = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            assert!(
                matches!(sample.track.kind, TrackKind::Subtitle),
                "字幕以外のトラックは本テストの対象外"
            );
            // 字幕サンプルはすべて keyframe = true で投入しているので stss は生成されない
            assert!(
                sample.keyframe,
                "字幕サンプルが同期サンプルとして復元されていない"
            );
            actual_samples.push((sample.duration, sample.data_size));
        }
        assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples
            .iter()
            .zip(actual_samples.iter())
            .enumerate()
        {
            assert_eq!(expected.0, actual.0, "sample {i} で duration が一致しない");
            assert_eq!(expected.1, actual.1, "sample {i} で data_size が一致しない");
        }
        Ok(())
    })?;
    Ok(())
}

/// composition_time_offset が `ctts` 経由で roundtrip される
#[test]
fn mux_demux_video_composition_time_offset_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        // 旧 proptest 版は独立した 2 本の vec を引いた後に長さ一致を prop_assume で
        // 要求していたが、noprop では長さを一度だけ引いて両方の生成に使うことで
        // valid-by-construction に振り、ケース拒否を回避する
        let n = noprop::sample_usize_in(ctx, 1..20);
        let mut durations: Vec<u32> = Vec::new();
        for _ in 0..n {
            durations.push(noprop::sample_u64_in(ctx, 1..3001) as u32);
        }
        let mut composition_time_offsets: Vec<Option<i64>> = Vec::new();
        for _ in 0..n {
            composition_time_offsets.push(if noprop::sample_bool(ctx) {
                Some(noprop::sample_u64_in(ctx, 0..6001) as i64 - 3000)
            } else {
                None
            });
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(90_000).expect("非ゼロである");
        let mut sample_entry = Some(create_avc1_sample_entry(width, height));

        for ((duration, composition_time_offset), index) in durations
            .iter()
            .zip(composition_time_offsets.iter())
            .zip(0..)
        {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: sample_entry.take(),
                keyframe: index == 0,
                timescale,
                duration: *duration,
                composition_time_offset: *composition_time_offset,
                data_offset,
                data_size: 128,
            };
            muxer
                .append_sample(&sample)
                .expect("sample の追加に失敗した");
            data_offset += 128;
        }

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");
        let file_data = build_file_data(&initial_bytes, finalized, durations.len() * 128);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let has_any_cto = composition_time_offsets.iter().any(Option::is_some);
        for expected in &composition_time_offsets {
            let sample = demuxer
                .next_sample()
                .expect("sample の読み取りに失敗した")
                .expect("sample が欠落している");
            let normalized = if has_any_cto {
                Some(expected.unwrap_or(0))
            } else {
                None
            };
            assert_eq!(sample.composition_time_offset, normalized);
        }
        Ok(())
    })?;
    Ok(())
}

/// 負の composition_time_offset を含む場合、ctts version 1 の表現範囲を超える正値はエラーになる
#[test]
fn mux_video_composition_time_offset_out_of_i32_range_for_ctts_v1() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let duration_a = noprop::sample_u64_in(ctx, 1..3001) as u32;
        let duration_b = noprop::sample_u64_in(ctx, 1..3001) as u32;
        // (i32::MAX + 1)..=u32::MAX を i64 で表現。u64 として引いてから i64 に落とす
        let too_large_positive_cto =
            noprop::sample_u64_in(ctx, (i32::MAX as u64 + 1)..=(u32::MAX as u64)) as i64;

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let initial_data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(90_000).expect("非ゼロである");
        let sample_entry = create_avc1_sample_entry(width, height);

        muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Video,
                sample_entry: Some(sample_entry.clone()),
                keyframe: true,
                timescale,
                duration: duration_a,
                composition_time_offset: Some(-1),
                data_offset: initial_data_offset,
                data_size: 128,
            })
            .expect("sample の追加に失敗した");

        muxer
            .append_sample(&Sample {
                track_kind: TrackKind::Video,
                sample_entry: None,
                keyframe: false,
                timescale,
                duration: duration_b,
                composition_time_offset: Some(too_large_positive_cto),
                data_offset: initial_data_offset + 128,
                data_size: 128,
            })
            .expect("sample の追加に失敗した");

        let result = muxer.finalize();
        assert!(result.is_err());
        Ok(())
    })?;
    Ok(())
}

/// ビデオ + オーディオの Mux → Demux roundtrip
#[test]
fn mux_demux_video_audio_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let channel_count = noprop::sample_u64_in(ctx, 1..=8) as u8;
        let mut video_samples = sample_vec(ctx, 1..10, arb_video_sample_info);
        let audio_samples = sample_vec(ctx, 1..15, arb_audio_sample_info);

        // 最初のビデオサンプルは必ず keyframe にする
        if let Some(first) = video_samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_timescale = NonZeroU32::new(30).expect("30 は非ゼロ");
        let audio_timescale = NonZeroU32::new(48000).expect("48000 は非ゼロ");

        let mut total_data_size = 0usize;

        // ビデオサンプルを追加
        let mut video_sample_entry = Some(create_avc1_sample_entry(width, height));
        for sample_info in &video_samples {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: video_sample_entry.take(),
                keyframe: sample_info.keyframe,
                timescale: video_timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("video sample の追加に失敗した");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // オーディオサンプルを追加
        // 正規は keyframe = true だが、全 false でも空 stss を出さず省略する契約を検証するため false を固定する
        let mut audio_sample_entry = Some(create_opus_sample_entry(channel_count));
        for sample_info in &audio_samples {
            let sample = Sample {
                track_kind: TrackKind::Audio,
                sample_entry: audio_sample_entry.take(),
                keyframe: false,
                timescale: audio_timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("audio sample の追加に失敗した");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        // 音声トラック（2 本目）の stss が省略されていること
        assert!(
            finalized.moov_box().trak_boxes[1]
                .mdia_box
                .minf_box
                .stbl_box
                .stss_box
                .is_none(),
            "全非キーフレームの音声トラックで空の stss が出力された"
        );

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 2);

        // サンプル数を確認
        // 音声は入力 keyframe = false だが、stss 省略により demux では同期サンプルになる
        let mut video_count = 0;
        let mut audio_count = 0;
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            match sample.track.kind {
                TrackKind::Video => video_count += 1,
                TrackKind::Audio => {
                    assert!(
                        sample.keyframe,
                        "音声サンプルが同期サンプルとして復元されていない"
                    );
                    audio_count += 1;
                }
                // このテストは Audio / Video 系トラックのみを扱う。字幕が現れたらテスト条件外
                TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外"),
            }
        }
        assert_eq!(video_count, video_samples.len());
        assert_eq!(audio_count, audio_samples.len());
        Ok(())
    })?;
    Ok(())
}

/// 映像 + 音声 + 字幕の 3 トラック Mux → Demux roundtrip
#[test]
fn mux_demux_video_audio_subtitle_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let channel_count = noprop::sample_u64_in(ctx, 1..=8) as u8;
        let mut video_samples = sample_vec(ctx, 1..10, arb_video_sample_info);
        let audio_samples = sample_vec(ctx, 1..15, arb_audio_sample_info);
        let subtitle_samples = sample_vec(ctx, 1..10, arb_subtitle_sample_info);

        // 最初の映像サンプルは必ず keyframe にする
        if let Some(first) = video_samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_timescale = NonZeroU32::new(30).expect("30 は非ゼロ");
        let audio_timescale = NonZeroU32::new(48000).expect("48000 は非ゼロ");
        let subtitle_timescale = NonZeroU32::new(1000).expect("1000 は非ゼロ");

        let mut total_data_size = 0usize;

        // サンプル追加順は 映像 → 音声 → 字幕
        let mut video_sample_entry = Some(create_avc1_sample_entry(width, height));
        for sample_info in &video_samples {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: video_sample_entry.take(),
                keyframe: sample_info.keyframe,
                timescale: video_timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("video sample の追加に失敗した");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        let mut audio_sample_entry = Some(create_opus_sample_entry(channel_count));
        for sample_info in &audio_samples {
            let sample = Sample {
                track_kind: TrackKind::Audio,
                sample_entry: audio_sample_entry.take(),
                // 正規は true だが、全 false でも空 stss を出さず省略する契約を検証するため false を固定する
                keyframe: false,
                timescale: audio_timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("audio sample の追加に失敗した");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        let mut subtitle_sample_entry = Some(create_stpp_sample_entry());
        for sample_info in &subtitle_samples {
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: subtitle_sample_entry.take(),
                keyframe: true,
                timescale: subtitle_timescale,
                duration: sample_info.duration,
                composition_time_offset: None,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("subtitle sample の追加に失敗した");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");
        assert!(
            finalized.moov_box().trak_boxes[1]
                .mdia_box
                .minf_box
                .stbl_box
                .stss_box
                .is_none(),
            "全非キーフレームの音声トラックで空の stss が出力された"
        );

        // 3 トラックとも `timescale` が異なるため、少なくとも 2 つは換算を経る。
        // 特に音声（48000）は正規化した尺が映像（30）に届かず `mvhd` に採用されないので、
        // 換算結果が 1 未満になり切り上げが効くケースをほぼ確実に通る
        assert_moov_duration_invariants(
            finalized.moov_box(),
            &[
                (
                    HdlrBox::HANDLER_TYPE_VIDE,
                    video_timescale,
                    video_samples.iter().map(|s| s.duration as u64).sum(),
                ),
                (
                    HdlrBox::HANDLER_TYPE_SOUN,
                    audio_timescale,
                    audio_samples.iter().map(|s| s.duration as u64).sum(),
                ),
                (
                    HdlrBox::HANDLER_TYPE_SUBT,
                    subtitle_timescale,
                    subtitle_samples.iter().map(|s| s.duration as u64).sum(),
                ),
            ],
        );

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 3);

        // trak は append_sample() の呼び出し順（映像 → 音声 → 字幕）で並び、
        // track_id もその順に 1 から振られる
        assert!(matches!(tracks[0].kind, TrackKind::Video));
        assert!(matches!(tracks[1].kind, TrackKind::Audio));
        assert!(matches!(tracks[2].kind, TrackKind::Subtitle));
        assert_eq!(tracks[0].track_id, 1);
        assert_eq!(tracks[1].track_id, 2);
        assert_eq!(tracks[2].track_id, 3);

        // トラックごとに別々の timescale が取り違えられずに復元される
        assert_eq!(tracks[0].timescale, video_timescale);
        assert_eq!(tracks[1].timescale, audio_timescale);
        assert_eq!(tracks[2].timescale, subtitle_timescale);

        // サンプル数を確認
        // 音声は入力 keyframe = false だが、stss 省略により demux では同期サンプルになる
        let mut video_count = 0;
        let mut audio_count = 0;
        let mut subtitle_count = 0;
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            match sample.track.kind {
                TrackKind::Video => video_count += 1,
                TrackKind::Audio => {
                    assert!(
                        sample.keyframe,
                        "音声サンプルが同期サンプルとして復元されていない"
                    );
                    audio_count += 1;
                }
                TrackKind::Subtitle => {
                    assert!(
                        sample.keyframe,
                        "字幕サンプルが同期サンプルとして復元されていない"
                    );
                    subtitle_count += 1;
                }
            }
        }
        assert_eq!(video_count, video_samples.len());
        assert_eq!(audio_count, audio_samples.len());
        assert_eq!(subtitle_count, subtitle_samples.len());
        Ok(())
    })?;
    Ok(())
}

/// 使用した SampleEntry に応じて ftyp compatible brands が更新される
#[test]
fn compatible_brands_follow_used_sample_entries() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let codec_mask = noprop::sample_u64_in(ctx, 0..16) as u8;
        let reserved_moov_box_size = noprop::sample_usize_in(ctx, 0..4096);

        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size,
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let mut total_data_size = 0usize;

        let append_video_sample =
            |muxer: &mut Mp4FileMuxer, data_offset: u64, sample_entry: SampleEntry| {
                let sample = Sample {
                    track_kind: TrackKind::Video,
                    sample_entry: Some(sample_entry),
                    keyframe: true,
                    timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
                    duration: 1,
                    composition_time_offset: None,
                    data_offset,
                    data_size: 256,
                };
                muxer
                    .append_sample(&sample)
                    .expect("video sample の追加に失敗した");
            };

        if codec_mask == 0 {
            let sample = Sample {
                track_kind: TrackKind::Audio,
                sample_entry: Some(create_opus_sample_entry(2)),
                keyframe: false,
                timescale: NonZeroU32::new(48000).expect("timescale は非ゼロである"),
                duration: 960,
                composition_time_offset: None,
                data_offset,
                data_size: 256,
            };
            muxer
                .append_sample(&sample)
                .expect("audio sample の追加に失敗した");
            data_offset += 256;
            total_data_size += 256;
        } else {
            if (codec_mask & 0b0001) != 0 {
                append_video_sample(&mut muxer, data_offset, create_avc1_sample_entry(1280, 720));
                data_offset += 256;
                total_data_size += 256;
            }
            if (codec_mask & 0b0010) != 0 {
                append_video_sample(&mut muxer, data_offset, create_hev1_sample_entry(1280, 720));
                data_offset += 256;
                total_data_size += 256;
            }
            if (codec_mask & 0b0100) != 0 {
                append_video_sample(&mut muxer, data_offset, create_hvc1_sample_entry(1280, 720));
                data_offset += 256;
                total_data_size += 256;
            }
            if (codec_mask & 0b1000) != 0 {
                append_video_sample(&mut muxer, data_offset, create_av01_sample_entry(1280, 720));
                data_offset += 256;
                total_data_size += 256;
            }
        }

        assert_eq!(
            data_offset,
            muxer.initial_boxes_bytes().len() as u64 + total_data_size as u64
        );

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);
        let (ftyp_box, _) = FtypBox::decode(&file_data).expect("ftyp ボックスのデコードに失敗した");

        let mut expected_brands = vec![Brand::ISOM, Brand::ISO2, Brand::MP41];
        if (codec_mask & 0b0001) != 0 {
            expected_brands.push(Brand::AVC1);
        }
        if (codec_mask & 0b0010) != 0 {
            expected_brands.push(Brand::HEV1);
        }
        if (codec_mask & 0b0100) != 0 {
            expected_brands.push(Brand::HVC1);
        }
        if (codec_mask & 0b1000) != 0 {
            expected_brands.push(Brand::AV01);
        }

        assert_eq!(ftyp_box.major_brand, Brand::ISOM);
        assert_eq!(ftyp_box.compatible_brands, expected_brands);
        Ok(())
    })?;
    Ok(())
}

/// advance_position を使用したビデオのみの Mux → Demux roundtrip
///
/// サンプル間にランダムなギャップ（非サンプルデータ）を挿入し、
/// advance_position で位置を進めた上で正しく roundtrip することを検証する。
#[test]
fn mux_demux_video_with_advance_position_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let timescale = noprop::sample_u64_in(ctx, 1..90001) as u32;
        let mut samples = sample_vec(ctx, 1..20, arb_video_sample_info);
        let composition_time_offsets = sample_vec(ctx, 1..20, |ctx| {
            if noprop::sample_bool(ctx) {
                Some(noprop::sample_u64_in(ctx, 0..6001) as i64 - 3000)
            } else {
                None
            }
        });
        let gaps = sample_vec(ctx, 1..20, |ctx| noprop::sample_u64_in(ctx, 0..256));

        if let Some(first) = samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let initial_len = muxer.initial_boxes_bytes().len() as u64;
        let mut data_offset = initial_len;
        let timescale = NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ");

        let mut sample_entry = Some(create_avc1_sample_entry(width, height));
        let mut expected_samples = Vec::new();
        let mut expected_ctos: Vec<Option<i64>> = Vec::new();

        let mut regions: Vec<(u64, usize)> = Vec::new();

        for (i, sample_info) in samples.iter().enumerate() {
            let gap = gaps.get(i).copied().unwrap_or(0);
            if gap > 0 {
                regions.push((data_offset, gap as usize));
                muxer
                    .advance_position(gap)
                    .expect("position の前進に失敗した");
                data_offset += gap;
            }

            let cto = composition_time_offsets.get(i).copied().flatten();
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: sample_entry.take(),
                keyframe: sample_info.keyframe,
                timescale,
                duration: sample_info.duration,
                composition_time_offset: cto,
                data_offset,
                data_size: sample_info.data_size,
            };
            muxer
                .append_sample(&sample)
                .expect("sample の追加に失敗した");
            expected_samples.push((
                sample_info.keyframe,
                sample_info.duration,
                sample_info.data_size,
            ));
            expected_ctos.push(cto);

            regions.push((data_offset, sample_info.data_size));
            data_offset += sample_info.data_size as u64;
        }

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        let file_data = build_hybrid_file_data(&initial_bytes, finalized, &regions);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));

        let has_any_cto = expected_ctos.iter().any(Option::is_some);
        let mut actual_samples = Vec::new();
        let mut actual_ctos = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            actual_samples.push((sample.keyframe, sample.duration, sample.data_size));
            actual_ctos.push(sample.composition_time_offset);
        }
        assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples
            .iter()
            .zip(actual_samples.iter())
            .enumerate()
        {
            assert_eq!(expected.0, actual.0, "sample {i} で keyframe が一致しない");
            assert_eq!(expected.1, actual.1, "sample {i} で duration が一致しない");
            assert_eq!(expected.2, actual.2, "sample {i} で data_size が一致しない");
        }
        for (i, (expected, actual)) in expected_ctos.iter().zip(actual_ctos.iter()).enumerate() {
            let normalized = if has_any_cto {
                Some(expected.unwrap_or(0))
            } else {
                None
            };
            assert_eq!(
                normalized, *actual,
                "sample {i} で composition_time_offset が一致しない"
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// advance_position を使用したビデオ + オーディオの Mux → Demux roundtrip
///
/// あわせて moov ボックスの `mvhd` / `tkhd` / `mdhd` の尺の整合も検証する
#[test]
fn mux_demux_video_audio_with_advance_position_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_MAIN, |ctx| {
        let width = noprop::sample_u64_in(ctx, 16..1920) as u16;
        let height = noprop::sample_u64_in(ctx, 16..1080) as u16;
        let channel_count = noprop::sample_u64_in(ctx, 1..=8) as u8;
        let video_timescale = noprop::sample_u64_in(ctx, 1..90001) as u32;
        let audio_timescale = noprop::sample_u64_in(ctx, 1..48001) as u32;
        let mut video_samples = sample_vec(ctx, 1..10, arb_video_sample_info);
        let audio_samples = sample_vec(ctx, 1..10, arb_audio_sample_info);
        let video_ctos = sample_vec(ctx, 1..10, |ctx| {
            if noprop::sample_bool(ctx) {
                Some(noprop::sample_u64_in(ctx, 0..6001) as i64 - 3000)
            } else {
                None
            }
        });
        let gaps = sample_vec(ctx, 1..20, |ctx| noprop::sample_u64_in(ctx, 0..256));

        if let Some(first) = video_samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_timescale =
            NonZeroU32::new(video_timescale).expect("サンプル値域が 1 以上なので非ゼロ");
        let audio_timescale =
            NonZeroU32::new(audio_timescale).expect("サンプル値域が 1 以上なので非ゼロ");

        let mut video_entry = Some(create_avc1_sample_entry(width, height));
        let mut audio_entry = Some(create_opus_sample_entry(channel_count));
        let mut expected_video = Vec::new();
        let mut expected_video_ctos: Vec<Option<i64>> = Vec::new();
        let mut expected_audio = Vec::new();
        let mut regions: Vec<(u64, usize)> = Vec::new();
        let mut gap_idx = 0;

        // ビデオとオーディオを交互に追加し、間にギャップを挿入する
        let max_len = video_samples.len().max(audio_samples.len());
        for i in 0..max_len {
            if let Some(vs) = video_samples.get(i) {
                let gap = gaps.get(gap_idx).copied().unwrap_or(0);
                gap_idx += 1;
                if gap > 0 {
                    regions.push((data_offset, gap as usize));
                    muxer
                        .advance_position(gap)
                        .expect("position の前進に失敗した");
                    data_offset += gap;
                }

                let cto = video_ctos.get(i).copied().flatten();
                let sample = Sample {
                    track_kind: TrackKind::Video,
                    sample_entry: video_entry.take(),
                    keyframe: vs.keyframe,
                    timescale: video_timescale,
                    duration: vs.duration,
                    composition_time_offset: cto,
                    data_offset,
                    data_size: vs.data_size,
                };
                muxer
                    .append_sample(&sample)
                    .expect("video sample の追加に失敗した");
                expected_video.push((vs.keyframe, vs.duration, vs.data_size));
                expected_video_ctos.push(cto);
                regions.push((data_offset, vs.data_size));
                data_offset += vs.data_size as u64;
            }

            if let Some(aus) = audio_samples.get(i) {
                let gap = gaps.get(gap_idx).copied().unwrap_or(0);
                gap_idx += 1;
                if gap > 0 {
                    regions.push((data_offset, gap as usize));
                    muxer
                        .advance_position(gap)
                        .expect("position の前進に失敗した");
                    data_offset += gap;
                }

                let sample = Sample {
                    track_kind: TrackKind::Audio,
                    sample_entry: audio_entry.take(),
                    // 正規は true だが、全 false でも空 stss を出さず省略する契約を検証するため false を固定する
                    keyframe: false,
                    timescale: audio_timescale,
                    duration: aus.duration,
                    composition_time_offset: None,
                    data_offset,
                    data_size: aus.data_size,
                };
                muxer
                    .append_sample(&sample)
                    .expect("audio sample の追加に失敗した");
                expected_audio.push((aus.duration, aus.data_size));
                regions.push((data_offset, aus.data_size));
                data_offset += aus.data_size as u64;
            }
        }

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        // 映像が先に append されるため音声は 2 本目。全 false 入力でも stss は省略される
        assert!(
            finalized.moov_box().trak_boxes[1]
                .mdia_box
                .minf_box
                .stbl_box
                .stss_box
                .is_none(),
            "全非キーフレームの音声トラックで空の stss が出力された"
        );

        // 音声と映像の `timescale` を独立に生成するため、両者が食い違う入力が普通に現れる
        // （`expected_video` は (keyframe, duration, data_size)、`expected_audio` は (duration, data_size)）
        assert_moov_duration_invariants(
            finalized.moov_box(),
            &[
                (
                    HdlrBox::HANDLER_TYPE_VIDE,
                    video_timescale,
                    expected_video.iter().map(|s| s.1 as u64).sum(),
                ),
                (
                    HdlrBox::HANDLER_TYPE_SOUN,
                    audio_timescale,
                    expected_audio.iter().map(|s| s.0 as u64).sum(),
                ),
            ],
        );

        let file_data = build_hybrid_file_data(&initial_bytes, finalized, &regions);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 2);

        let has_any_video_cto = expected_video_ctos.iter().any(Option::is_some);
        let mut actual_video = Vec::new();
        let mut actual_video_ctos = Vec::new();
        let mut actual_audio = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("sample の読み取りに失敗した")
        {
            match sample.track.kind {
                TrackKind::Video => {
                    actual_video.push((sample.keyframe, sample.duration, sample.data_size));
                    actual_video_ctos.push(sample.composition_time_offset);
                }
                TrackKind::Audio => {
                    // 入力は keyframe = false だが、stss 省略により demux では同期サンプルになる
                    assert!(
                        sample.keyframe,
                        "音声サンプルが同期サンプルとして復元されていない"
                    );
                    actual_audio.push((sample.duration, sample.data_size));
                }
                // このテストは Audio / Video 系トラックのみを扱う。字幕が現れたらテスト条件外
                TrackKind::Subtitle => unreachable!("字幕トラックは本テストの対象外"),
            }
        }
        assert_eq!(actual_video.len(), expected_video.len());
        assert_eq!(actual_audio.len(), expected_audio.len());
        for (i, (expected, actual)) in expected_video.iter().zip(actual_video.iter()).enumerate() {
            assert_eq!(
                expected.0, actual.0,
                "video の keyframe が {i} で一致しない"
            );
            assert_eq!(
                expected.1, actual.1,
                "video の duration が {i} で一致しない"
            );
            assert_eq!(
                expected.2, actual.2,
                "video の data_size が {i} で一致しない"
            );
        }
        for (i, (expected, actual)) in expected_video_ctos
            .iter()
            .zip(actual_video_ctos.iter())
            .enumerate()
        {
            let normalized = if has_any_video_cto {
                Some(expected.unwrap_or(0))
            } else {
                None
            };
            assert_eq!(
                normalized, *actual,
                "video の composition_time_offset が {i} で一致しない"
            );
        }
        for (i, (expected, actual)) in expected_audio.iter().zip(actual_audio.iter()).enumerate() {
            assert_eq!(
                expected.0, actual.0,
                "audio の duration が {i} で一致しない"
            );
            assert_eq!(
                expected.1, actual.1,
                "audio の data_size が {i} で一致しない"
            );
        }
        Ok(())
    })?;
    Ok(())
}

// ===== estimate_maximum_moov_box_size のテスト =====

mod estimate_moov_size_tests {
    use super::*;

    /// このモジュールの PBT ケース数（旧 `with_cases(100)` を維持）
    const CASES: usize = 100;

    /// estimate_maximum_moov_box_size は非負の値を返す
    #[test]
    fn estimate_returns_non_negative() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let track_counts = sample_vec(ctx, 0..10, |ctx| noprop::sample_usize_in(ctx, 0..10000));
            let result = estimate_maximum_moov_box_size(&track_counts);
            assert!(result > 0 || track_counts.is_empty());
            Ok(())
        })?;
        Ok(())
    }

    /// estimate_maximum_moov_box_size はサンプル数に対して単調増加
    #[test]
    fn estimate_monotonically_increasing_with_samples() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let base_count = noprop::sample_usize_in(ctx, 0..1000);
            let additional = noprop::sample_usize_in(ctx, 1..1000);
            let small = estimate_maximum_moov_box_size(&[base_count]);
            let large = estimate_maximum_moov_box_size(&[base_count + additional]);
            assert!(large >= small, "estimate は sample 数に応じて増加する");
            Ok(())
        })?;
        Ok(())
    }

    /// estimate_maximum_moov_box_size はトラック数に対して単調増加
    #[test]
    fn estimate_monotonically_increasing_with_tracks() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let sample_count = noprop::sample_usize_in(ctx, 0..1000);
            let track_count = noprop::sample_usize_in(ctx, 1..10);
            let single_track = estimate_maximum_moov_box_size(&[sample_count]);
            let multi_track: Vec<usize> = (0..track_count).map(|_| sample_count).collect();
            let result = estimate_maximum_moov_box_size(&multi_track);
            assert!(
                result >= single_track,
                "estimate は track 数に応じて増加する"
            );
            Ok(())
        })?;
        Ok(())
    }

    /// estimate_maximum_moov_box_size の結果は実際の moov サイズより大きい
    #[test]
    fn estimate_is_upper_bound() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let video_sample_count = noprop::sample_usize_in(ctx, 1..50);
            let audio_sample_count = noprop::sample_usize_in(ctx, 1..50);
            let estimated =
                estimate_maximum_moov_box_size(&[video_sample_count, audio_sample_count]);

            // 実際に Muxer で moov を生成してサイズを比較
            let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
            let mut data_offset = muxer.initial_boxes_bytes().len() as u64;

            // ビデオサンプルを追加
            let mut video_entry = Some(create_avc1_sample_entry(1920, 1080));
            for _ in 0..video_sample_count {
                let sample = Sample {
                    track_kind: TrackKind::Video,
                    sample_entry: video_entry.take(),
                    keyframe: true,
                    timescale: NonZeroU32::new(30).expect("30 は非ゼロ"),
                    duration: 1,
                    composition_time_offset: None,
                    data_offset,
                    data_size: 100,
                };
                muxer
                    .append_sample(&sample)
                    .expect("video sample の追加に失敗した");
                data_offset += 100;
            }

            // オーディオサンプルを追加
            let mut audio_entry = Some(create_opus_sample_entry(2));
            for _ in 0..audio_sample_count {
                let sample = Sample {
                    track_kind: TrackKind::Audio,
                    sample_entry: audio_entry.take(),
                    keyframe: false,
                    timescale: NonZeroU32::new(48000).expect("48000 は非ゼロ"),
                    duration: 960,
                    composition_time_offset: None,
                    data_offset,
                    data_size: 50,
                };
                muxer
                    .append_sample(&sample)
                    .expect("audio sample の追加に失敗した");
                data_offset += 50;
            }

            let finalized = muxer.finalize().expect("finalize に失敗した");
            let actual_moov_size = finalized.moov_box_size();

            assert!(
                estimated >= actual_moov_size,
                "推定値 {estimated} は実測値 {actual_moov_size} 以上である"
            );
            Ok(())
        })?;
        Ok(())
    }
}
