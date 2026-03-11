//! Mux → Demux Roundtrip の Property-Based Testing
//!
//! Mp4FileMuxer で作成したデータを Mp4FileDemuxer で読み取り、
//! 元のデータと一致することを確認するテスト

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, Brand, DopsBox, FtypBox,
        Hev1Box, Hvc1Box, HvccBox, OpusBox, SampleEntry, VisualSampleEntryFields,
    },
    demux::{Input, Mp4FileDemuxer},
    mux::{
        FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, Sample, estimate_maximum_moov_box_size,
    },
};

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

/// ビデオサンプル情報を生成する Strategy
fn arb_video_sample_info() -> impl Strategy<Value = VideoSampleInfo> {
    (any::<bool>(), 1u32..100, 100usize..10000).prop_map(|(keyframe, duration, data_size)| {
        VideoSampleInfo {
            keyframe,
            duration,
            data_size,
        }
    })
}

/// オーディオサンプル情報を生成する Strategy
fn arb_audio_sample_info() -> impl Strategy<Value = AudioSampleInfo> {
    (1u32..100, 100usize..5000).prop_map(|(duration, data_size)| AudioSampleInfo {
        duration,
        data_size,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    /// ビデオのみの Mux → Demux roundtrip
    #[test]
    fn mux_demux_video_only_roundtrip(
        width in 16u16..1920,
        height in 16u16..1080,
        timescale in 1u32..90001,
        samples in prop::collection::vec(arb_video_sample_info(), 1..20)
    ) {
        // 最初のサンプルは必ず keyframe にする
        let mut samples = samples;
        if let Some(first) = samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(timescale).unwrap_or(NonZeroU32::MIN);

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
            muxer.append_sample(&sample).expect("failed to append sample");
            expected_samples.push((sample_info.keyframe, sample_info.duration, sample_info.data_size));
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert!(matches!(tracks[0].kind, TrackKind::Video));

        // サンプル数と属性を確認
        let mut actual_samples = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("failed to read sample") {
            actual_samples.push((sample.keyframe, sample.duration, sample.data_size));
        }
        prop_assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples.iter().zip(actual_samples.iter()).enumerate() {
            prop_assert_eq!(expected.0, actual.0, "keyframe mismatch at sample {}", i);
            prop_assert_eq!(expected.1, actual.1, "duration mismatch at sample {}", i);
            prop_assert_eq!(expected.2, actual.2, "data_size mismatch at sample {}", i);
        }
    }

    /// オーディオのみの Mux → Demux roundtrip
    #[test]
    fn mux_demux_audio_only_roundtrip(
        channel_count in 1u8..=8,
        timescale in 1u32..48001,
        samples in prop::collection::vec(arb_audio_sample_info(), 1..30)
    ) {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(timescale).unwrap_or(NonZeroU32::MIN);

        // サンプルを追加
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
            muxer.append_sample(&sample).expect("failed to append sample");
            expected_samples.push((sample_info.duration, sample_info.data_size));
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert!(matches!(tracks[0].kind, TrackKind::Audio));

        // サンプル数と属性を確認
        let mut actual_samples = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("failed to read sample") {
            actual_samples.push((sample.duration, sample.data_size));
        }
        prop_assert_eq!(actual_samples.len(), expected_samples.len());
        for (i, (expected, actual)) in expected_samples.iter().zip(actual_samples.iter()).enumerate() {
            prop_assert_eq!(expected.0, actual.0, "duration mismatch at sample {}", i);
            prop_assert_eq!(expected.1, actual.1, "data_size mismatch at sample {}", i);
        }
    }

    /// composition_time_offset が `ctts` 経由で roundtrip される
    #[test]
    fn mux_demux_video_composition_time_offset_roundtrip(
        width in 16u16..1920,
        height in 16u16..1080,
        durations in prop::collection::vec(1u32..3001, 1..20),
        composition_time_offsets in prop::collection::vec(prop::option::of(-3000i32..3001), 1..20),
    ) {
        prop_assume!(durations.len() == composition_time_offsets.len());

        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let timescale = NonZeroU32::new(90_000).expect("non-zero");
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
            muxer.append_sample(&sample).expect("failed to append sample");
            data_offset += 128;
        }

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");
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
                .expect("failed to read sample")
                .expect("missing sample");
            let normalized = if has_any_cto {
                Some(i64::from(expected.unwrap_or(0)))
            } else {
                None
            };
            prop_assert_eq!(sample.composition_time_offset, normalized);
        }
    }

    /// ビデオ + オーディオの Mux → Demux roundtrip
    #[test]
    fn mux_demux_video_audio_roundtrip(
        width in 16u16..1920,
        height in 16u16..1080,
        channel_count in 1u8..=8,
        video_samples in prop::collection::vec(arb_video_sample_info(), 1..10),
        audio_samples in prop::collection::vec(arb_audio_sample_info(), 1..15)
    ) {
        // 最初のビデオサンプルは必ず keyframe にする
        let mut video_samples = video_samples;
        if let Some(first) = video_samples.first_mut() {
            first.keyframe = true;
        }

        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_timescale = NonZeroU32::new(30).unwrap();
        let audio_timescale = NonZeroU32::new(48000).unwrap();

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
            muxer.append_sample(&sample).expect("failed to append video sample");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // オーディオサンプルを追加
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
            muxer.append_sample(&sample).expect("failed to append audio sample");
            data_offset += sample_info.data_size as u64;
            total_data_size += sample_info.data_size;
        }

        // ファイナライズ
        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");

        // ファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);

        // Demux
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 2);

        // サンプル数を確認
        let mut video_count = 0;
        let mut audio_count = 0;
        while let Some(sample) = demuxer.next_sample().expect("failed to read sample") {
            match sample.track.kind {
                TrackKind::Video => video_count += 1,
                TrackKind::Audio => audio_count += 1,
            }
        }
        prop_assert_eq!(video_count, video_samples.len());
        prop_assert_eq!(audio_count, audio_samples.len());
    }

    /// 使用した SampleEntry に応じて ftyp compatible brands が更新される
    #[test]
    fn compatible_brands_follow_used_sample_entries(
        codec_mask in 0u8..16,
        reserved_moov_box_size in 0usize..4096
    ) {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size,
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("failed to create muxer");
        let mut data_offset = muxer.initial_boxes_bytes().len() as u64;
        let mut total_data_size = 0usize;

        let append_video_sample = |muxer: &mut Mp4FileMuxer, data_offset: u64, sample_entry: SampleEntry| {
            let sample = Sample {
                track_kind: TrackKind::Video,
                sample_entry: Some(sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(30).expect("non-zero timescale"),
                duration: 1,
                composition_time_offset: None,
                data_offset,
                data_size: 256,
            };
            muxer.append_sample(&sample).expect("failed to append video sample");
        };

        if codec_mask == 0 {
            let sample = Sample {
                track_kind: TrackKind::Audio,
                sample_entry: Some(create_opus_sample_entry(2)),
                keyframe: false,
                timescale: NonZeroU32::new(48000).expect("non-zero timescale"),
                duration: 960,
                composition_time_offset: None,
                data_offset,
                data_size: 256,
            };
            muxer
                .append_sample(&sample)
                .expect("failed to append audio sample");
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

        prop_assert_eq!(
            data_offset,
            muxer.initial_boxes_bytes().len() as u64 + total_data_size as u64
        );

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");
        let file_data = build_file_data(&initial_bytes, finalized, total_data_size);
        let (ftyp_box, _) = FtypBox::decode(&file_data).expect("failed to decode ftyp box");

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

        prop_assert_eq!(ftyp_box.major_brand, Brand::ISOM);
        prop_assert_eq!(ftyp_box.compatible_brands, expected_brands);
    }
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// 最小構成のビデオファイル
    #[test]
    fn minimal_video_file() {
        let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(640, 480)),
            keyframe: true,
            timescale: NonZeroU32::new(30).unwrap(),
            duration: 1,
            composition_time_offset: None,
            data_offset,
            data_size: 100,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");

        let file_data = build_file_data(&initial_bytes, finalized, 100);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("failed to get tracks");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));

        let sample = demuxer.next_sample().expect("failed").expect("no sample");
        assert!(sample.keyframe);
        assert_eq!(sample.data_size, 100);
    }

    /// faststart が有効な場合の roundtrip
    #[test]
    fn faststart_enabled_roundtrip() {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("failed to create muxer");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(1920, 1080)),
            keyframe: true,
            timescale: NonZeroU32::new(30).unwrap(),
            duration: 1,
            composition_time_offset: None,
            data_offset,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("failed to finalize");
        assert!(finalized.is_faststart_enabled());

        // faststart 用のファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, 1024);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("failed to get tracks");
        assert_eq!(tracks.len(), 1);
    }
}

// ===== estimate_maximum_moov_box_size のテスト =====

mod estimate_moov_size_tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// estimate_maximum_moov_box_size は非負の値を返す
        #[test]
        fn estimate_returns_non_negative(
            track_counts in prop::collection::vec(0usize..10000, 0..10)
        ) {
            let result = estimate_maximum_moov_box_size(&track_counts);
            prop_assert!(result > 0 || track_counts.is_empty());
        }

        /// estimate_maximum_moov_box_size はサンプル数に対して単調増加
        #[test]
        fn estimate_monotonically_increasing_with_samples(
            base_count in 0usize..1000,
            additional in 1usize..1000
        ) {
            let small = estimate_maximum_moov_box_size(&[base_count]);
            let large = estimate_maximum_moov_box_size(&[base_count + additional]);
            prop_assert!(large >= small, "estimate should increase with sample count");
        }

        /// estimate_maximum_moov_box_size はトラック数に対して単調増加
        #[test]
        fn estimate_monotonically_increasing_with_tracks(
            sample_count in 0usize..1000,
            track_count in 1usize..10
        ) {
            let single_track = estimate_maximum_moov_box_size(&[sample_count]);
            let multi_track: Vec<usize> = (0..track_count).map(|_| sample_count).collect();
            let result = estimate_maximum_moov_box_size(&multi_track);
            prop_assert!(result >= single_track, "estimate should increase with track count");
        }

        /// estimate_maximum_moov_box_size の結果は実際の moov サイズより大きい
        #[test]
        fn estimate_is_upper_bound(
            video_sample_count in 1usize..50,
            audio_sample_count in 1usize..50
        ) {
            let estimated = estimate_maximum_moov_box_size(&[video_sample_count, audio_sample_count]);

            // 実際に Muxer で moov を生成してサイズを比較
            let mut muxer = Mp4FileMuxer::new().expect("failed to create muxer");
            let mut data_offset = muxer.initial_boxes_bytes().len() as u64;

            // ビデオサンプルを追加
            let mut video_entry = Some(create_avc1_sample_entry(1920, 1080));
            for _ in 0..video_sample_count {
                let sample = Sample {
                    track_kind: TrackKind::Video,
                    sample_entry: video_entry.take(),
                    keyframe: true,
                    timescale: NonZeroU32::new(30).unwrap(),
                    duration: 1,
                    composition_time_offset: None,
                    data_offset,
                    data_size: 100,
                };
                muxer.append_sample(&sample).expect("failed to append video sample");
                data_offset += 100;
            }

            // オーディオサンプルを追加
            let mut audio_entry = Some(create_opus_sample_entry(2));
            for _ in 0..audio_sample_count {
                let sample = Sample {
                    track_kind: TrackKind::Audio,
                    sample_entry: audio_entry.take(),
                    keyframe: false,
                    timescale: NonZeroU32::new(48000).unwrap(),
                    duration: 960,
                    composition_time_offset: None,
                    data_offset,
                    data_size: 50,
                };
                muxer.append_sample(&sample).expect("failed to append audio sample");
                data_offset += 50;
            }

            let finalized = muxer.finalize().expect("failed to finalize");
            let actual_moov_size = finalized.moov_box_size();

            prop_assert!(
                estimated >= actual_moov_size,
                "estimated {} should be >= actual {}",
                estimated,
                actual_moov_size
            );
        }
    }

    /// 空のトラックリストの場合
    #[test]
    fn estimate_empty_tracks() {
        let result = estimate_maximum_moov_box_size(&[]);
        // 基本オーバーヘッドのみ
        assert!(result > 0);
    }

    /// 単一トラック、サンプルなし
    #[test]
    fn estimate_single_track_no_samples() {
        let result = estimate_maximum_moov_box_size(&[0]);
        assert!(result > 0);
    }

    /// 大量のサンプルがある場合
    #[test]
    fn estimate_large_sample_count() {
        let result = estimate_maximum_moov_box_size(&[100000, 100000]);
        // 大量のサンプルでもオーバーフローしない
        assert!(result > 0);
    }
}
