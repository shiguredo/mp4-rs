//! fMP4 Mux → Demux Roundtrip の Property-Based Testing
//!
//! Fmp4SegmentMuxer で生成した初期化セグメントとメディアセグメントを
//! Fmp4SegmentDemuxer で解析し、元のデータと一致することを確認するテスト

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry,
        VisualSampleEntryFields,
    },
    demux_file_fmp4::Fmp4FileDemuxer,
    demux_fmp4_segment::Fmp4SegmentDemuxer,
    mux_fmp4_segment::{Fmp4SegmentMuxer, Fmp4SegmentSample, Fmp4SegmentTrackConfig},
};

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

fn create_opus_sample_entry() -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: 2,
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(48000u16, 0),
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

/// サンプルデータを表す補助構造体
#[derive(Debug, Clone)]
struct TestSample {
    track_index: usize,
    duration: u32,
    keyframe: bool,
    data: Vec<u8>,
}

fn arb_video_sample(track_index: usize) -> impl Strategy<Value = TestSample> {
    (
        1u32..3001,
        any::<bool>(),
        prop::collection::vec(any::<u8>(), 1..256),
    )
        .prop_map(move |(duration, keyframe, data)| TestSample {
            track_index,
            duration,
            keyframe,
            data,
        })
}

fn arb_video_sample_with_cto(
    track_index: usize,
) -> impl Strategy<Value = (TestSample, Option<i32>)> {
    (
        arb_video_sample(track_index),
        prop::option::of(-3000i32..3001),
    )
}

fn arb_audio_sample(track_index: usize) -> impl Strategy<Value = TestSample> {
    (1u32..1921, prop::collection::vec(any::<u8>(), 1..128)).prop_map(move |(duration, data)| {
        TestSample {
            track_index,
            duration,
            keyframe: true,
            data,
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// 単一映像トラックの init + media segment roundtrip
    #[test]
    fn video_only_roundtrip(
        width in 64u16..1921,
        height in 64u16..1081,
        samples in prop::collection::vec(arb_video_sample(0), 1..10),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(width, height),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let fmp4_samples: Vec<Fmp4SegmentSample> = samples
            .iter()
            .map(|s| Fmp4SegmentSample {
                track_index: s.track_index,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: None,
                data: &s.data,
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert_eq!(tracks[0].kind, TrackKind::Video);
        prop_assert_eq!(tracks[0].timescale.get(), 90000);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            prop_assert_eq!(ds.duration, orig.duration);
            prop_assert_eq!(ds.keyframe, orig.keyframe);
            prop_assert_eq!(ds.data_size, orig.data.len());

            let actual = &segment_bytes[ds.data_offset..ds.data_offset + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// 単一音声トラックの roundtrip
    #[test]
    fn audio_only_roundtrip(
        samples in prop::collection::vec(arb_audio_sample(0), 1..10),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Audio,
            timescale: NonZeroU32::new(48000).expect("non-zero"),
            sample_entry: create_opus_sample_entry(),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let fmp4_samples: Vec<Fmp4SegmentSample> = samples
            .iter()
            .map(|s| Fmp4SegmentSample {
                track_index: s.track_index,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: None,
                data: &s.data,
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert_eq!(tracks[0].kind, TrackKind::Audio);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            prop_assert_eq!(ds.duration, orig.duration);
            let actual = &segment_bytes[ds.data_offset..ds.data_offset + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// 映像＋音声の 2 トラック roundtrip
    #[test]
    fn video_audio_roundtrip(
        width in 64u16..1921,
        height in 64u16..1081,
        video_samples in prop::collection::vec(arb_video_sample(0), 1..5),
        audio_samples in prop::collection::vec(arb_audio_sample(1), 1..5),
    ) {
        let tracks = vec![
            Fmp4SegmentTrackConfig {
                track_kind: TrackKind::Video,
                timescale: NonZeroU32::new(90000).expect("non-zero"),
                sample_entry: create_avc1_sample_entry(width, height),
            },
            Fmp4SegmentTrackConfig {
                track_kind: TrackKind::Audio,
                timescale: NonZeroU32::new(48000).expect("non-zero"),
                sample_entry: create_opus_sample_entry(),
            },
        ];

        let mut muxer = Fmp4SegmentMuxer::new(tracks).expect("failed to create muxer");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let mut all_samples: Vec<TestSample> = Vec::new();
        let max_len = video_samples.len().max(audio_samples.len());
        for i in 0..max_len {
            if let Some(s) = video_samples.get(i) {
                all_samples.push(s.clone());
            }
            if let Some(s) = audio_samples.get(i) {
                all_samples.push(s.clone());
            }
        }

        let fmp4_samples: Vec<Fmp4SegmentSample> = all_samples
            .iter()
            .map(|s| Fmp4SegmentSample {
                track_index: s.track_index,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: None,
                data: &s.data,
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let demux_tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(demux_tracks.len(), 2);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed.len(), video_samples.len() + audio_samples.len());

        let demuxed_video: Vec<_> = demuxed.iter().filter(|s| s.track_id == 1).collect();
        prop_assert_eq!(demuxed_video.len(), video_samples.len());
        for (orig, ds) in video_samples.iter().zip(demuxed_video.iter()) {
            let actual = &segment_bytes[ds.data_offset..ds.data_offset + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }

        let demuxed_audio: Vec<_> = demuxed.iter().filter(|s| s.track_id == 2).collect();
        prop_assert_eq!(demuxed_audio.len(), audio_samples.len());
        for (orig, ds) in audio_samples.iter().zip(demuxed_audio.iter()) {
            let actual = &segment_bytes[ds.data_offset..ds.data_offset + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// composition_time_offset が roundtrip で保持されることを確認する
    #[test]
    fn composition_time_offset_roundtrip(
        samples_with_cto in prop::collection::vec(arb_video_sample_with_cto(0), 1..10),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(320, 240),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let fmp4_samples: Vec<Fmp4SegmentSample> = samples_with_cto
            .iter()
            .map(|(s, cto)| Fmp4SegmentSample {
                track_index: s.track_index,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: *cto,
                data: &s.data,
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed.len(), samples_with_cto.len());

        // いずれかのサンプルに CTO がある場合、muxer は全サンプルを Some(x) に正規化する
        let has_any_cto = samples_with_cto.iter().any(|(_, c)| c.is_some());

        for ((_, expected_cto), ds) in samples_with_cto.iter().zip(demuxed.iter()) {
            let normalized = if has_any_cto {
                Some(expected_cto.unwrap_or(0))
            } else {
                None
            };
            prop_assert_eq!(ds.composition_time_offset, normalized);
        }
    }

    /// mfra_bytes が正しいバイト列を生成することを確認する
    #[test]
    fn mfra_bytes_roundtrip(
        segments in prop::collection::vec(
            prop::collection::vec(arb_video_sample(0), 1..5),
            1..5,
        ),
    ) {
        use shiguredo_mp4::boxes::MfraBox;
        use shiguredo_mp4::Decode;

        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(320, 240),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        muxer.init_segment_bytes().expect("failed to build init segment");

        for segment_samples in &segments {
            let fmp4_samples: Vec<Fmp4SegmentSample> = segment_samples
                .iter()
                .map(|s| Fmp4SegmentSample {
                    track_index: 0,
                    duration: s.duration,
                    keyframe: s.keyframe,
                    composition_time_offset: None,
                    data: &s.data,
                })
                .collect();
            muxer.create_media_segment(&fmp4_samples).expect("failed to create segment");
        }

        let mfra = muxer.mfra_bytes().expect("failed to build mfra");

        // mfra が valid な MP4 ボックスとしてデコードできること
        let (mfra_box, decoded_size) = MfraBox::decode(&mfra).expect("failed to decode mfra");
        prop_assert_eq!(decoded_size, mfra.len());

        // tfra のエントリ数はセグメント数と一致すること
        prop_assert_eq!(mfra_box.tfra_boxes.len(), 1);
        prop_assert_eq!(mfra_box.tfra_boxes[0].entries.len(), segments.len());

        // mfro.size が mfra 全体のサイズと一致すること
        prop_assert_eq!(mfra_box.mfro_box.size, mfra.len() as u32);
    }

    /// sidx 付きセグメントが正しく demux できることを確認する
    #[test]
    fn sidx_roundtrip(
        width in 64u16..1921,
        height in 64u16..1081,
        samples in prop::collection::vec(arb_video_sample(0), 1..5),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(width, height),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let fmp4_samples: Vec<Fmp4SegmentSample> = samples
            .iter()
            .map(|s| Fmp4SegmentSample {
                track_index: s.track_index,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: None,
                data: &s.data,
            })
            .collect();

        // sidx 付きセグメントを生成する
        let segment_bytes = muxer
            .create_media_segment_with_sidx(&fmp4_samples)
            .expect("failed to create sidx segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        // sidx は自動的にスキップされて正常に demux できる
        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle sidx segment");

        prop_assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            prop_assert_eq!(ds.duration, orig.duration);
            prop_assert_eq!(ds.keyframe, orig.keyframe);
            prop_assert_eq!(ds.data_size, orig.data.len());

            let actual = &segment_bytes[ds.data_offset..ds.data_offset + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// Fmp4FileDemuxer が mux したファイルを正しく demux できることを確認する
    #[test]
    fn fmp4_file_demuxer_roundtrip(
        width in 64u16..1921,
        height in 64u16..1081,
        segments in prop::collection::vec(
            prop::collection::vec(arb_video_sample(0), 1..5),
            1..4,
        ),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: std::num::NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(width, height),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        // 全セグメントをひとつのバイト列に連結して完全な fMP4 ファイルを組み立てる
        let mut file_data = init_bytes;
        let mut all_samples: Vec<TestSample> = Vec::new();

        for segment_samples in &segments {
            let fmp4_samples: Vec<Fmp4SegmentSample> = segment_samples
                .iter()
                .map(|s| Fmp4SegmentSample {
                    track_index: 0,
                    duration: s.duration,
                    keyframe: s.keyframe,
                    composition_time_offset: None,
                    data: &s.data,
                })
                .collect();
            let segment_bytes = muxer
                .create_media_segment(&fmp4_samples)
                .expect("failed to create media segment");
            file_data.extend_from_slice(&segment_bytes);
            all_samples.extend_from_slice(segment_samples);
        }

        let mut demuxer =
            Fmp4FileDemuxer::new(file_data).expect("Fmp4FileDemuxer::new failed");

        // トラック情報の確認
        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert_eq!(tracks[0].kind, TrackKind::Video);
        prop_assert_eq!(tracks[0].timescale.get(), 90000);

        // サンプルを順番に取り出して元データと照合する
        let mut expected_decode_time: u64 = 0;
        for orig in &all_samples {
            let sample = demuxer
                .next_sample()
                .expect("next_sample error")
                .expect("unexpected end of samples");

            prop_assert_eq!(sample.track_id, 1);
            prop_assert_eq!(sample.base_media_decode_time, expected_decode_time);
            prop_assert_eq!(sample.duration, orig.duration);
            prop_assert_eq!(sample.keyframe, orig.keyframe);
            prop_assert_eq!(sample.data, orig.data.as_slice());

            expected_decode_time += orig.duration as u64;
        }

        // 全サンプルを読み終えたら None が返ることを確認する
        let last = demuxer.next_sample().expect("next_sample error");
        prop_assert!(last.is_none(), "expected no more samples, got {:?}", last);
    }

    /// decode_time が複数セグメントにわたって正しく累積されることを確認する
    #[test]
    fn decode_time_accumulation(
        samples_per_segment in prop::collection::vec(
            prop::collection::vec(arb_video_sample(0), 1..5),
            2..5,
        ),
    ) {
        let track_config = Fmp4SegmentTrackConfig {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(90000).expect("non-zero"),
            sample_entry: create_avc1_sample_entry(320, 240),
        };

        let mut muxer = Fmp4SegmentMuxer::new(vec![track_config]).expect("Fmp4SegmentMuxer::new failed");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let mut expected_decode_time: u64 = 0;

        for segment_samples in &samples_per_segment {
            let fmp4_samples: Vec<Fmp4SegmentSample> = segment_samples
                .iter()
                .map(|s| Fmp4SegmentSample {
                    track_index: 0,
                    duration: s.duration,
                    keyframe: s.keyframe,
                    composition_time_offset: None,
                    data: &s.data,
                })
                .collect();

            let segment_bytes = muxer
                .create_media_segment(&fmp4_samples)
                .expect("failed to create media segment");

            let demuxed = demuxer
                .handle_media_segment(&segment_bytes)
                .expect("failed to handle media segment");

            prop_assert_eq!(demuxed[0].base_media_decode_time, expected_decode_time);

            expected_decode_time +=
                segment_samples.iter().map(|s| s.duration as u64).sum::<u64>();
        }
    }
}
