//! fMP4 Mux → Demux Roundtrip の Property-Based Testing
//!
//! Fmp4SegmentMuxer で生成した初期化セグメントとメディアセグメントを
//! Fmp4SegmentDemuxer で解析し、元のデータと一致することを確認するテスト

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, FtypBox, MoofBox, MoovBox, OpusBox,
        SampleEntry, VisualSampleEntryFields,
    },
    demux::{DemuxError, Fmp4FileDemuxer, Fmp4SegmentDemuxer, Input},
    mux::{Fmp4SegmentMuxer, SegmentSample},
};

const VIDEO_TIMESCALE: u32 = 90_000;
const AUDIO_TIMESCALE: u32 = 48_000;

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

fn video_segment_sample<'a>(
    sample_entry: &SampleEntry,
    sample: &'a TestSample,
    composition_time_offset: Option<i32>,
) -> SegmentSample<'a> {
    SegmentSample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("non-zero"),
        sample_entry: Some(sample_entry.clone()),
        duration: sample.duration,
        keyframe: sample.keyframe,
        composition_time_offset,
        data: &sample.data,
    }
}

fn audio_segment_sample<'a>(
    sample_entry: &SampleEntry,
    sample: &'a TestSample,
) -> SegmentSample<'a> {
    SegmentSample {
        track_kind: TrackKind::Audio,
        timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("non-zero"),
        sample_entry: Some(sample_entry.clone()),
        duration: sample.duration,
        keyframe: sample.keyframe,
        composition_time_offset: None,
        data: &sample.data,
    }
}

fn feed_fmp4_file_demuxer(demuxer: &mut Fmp4FileDemuxer, file_data: &[u8]) {
    while let Some(required) = demuxer.required_input() {
        let start = required.position as usize;
        let end = if let Some(required_size) = required.size {
            start.saturating_add(required_size).min(file_data.len())
        } else {
            file_data.len()
        };
        let data = file_data.get(start..end).unwrap_or(&[]);
        demuxer.handle_input(Input {
            position: required.position,
            data,
        });
    }
}

fn rewrite_init_segment(init_segment: &[u8], f: impl FnOnce(&mut MoovBox)) -> Vec<u8> {
    let (ftyp_box, ftyp_box_size) =
        FtypBox::decode(init_segment).expect("failed to decode ftyp from init segment");
    let (mut moov_box, moov_box_size) = MoovBox::decode(&init_segment[ftyp_box_size..])
        .expect("failed to decode moov from init segment");
    assert_eq!(
        ftyp_box_size + moov_box_size,
        init_segment.len(),
        "init segment must contain only ftyp + moov in this test"
    );
    f(&mut moov_box);

    let mut rewritten = ftyp_box
        .encode_to_vec()
        .expect("failed to encode ftyp while rewriting init segment");
    let moov_bytes = moov_box
        .encode_to_vec()
        .expect("failed to encode moov while rewriting init segment");
    rewritten.extend_from_slice(&moov_bytes);
    rewritten
}

fn append_sample_entry_and_set_trex_default(
    init_segment: &[u8],
    sample_entry: SampleEntry,
    default_sample_description_index: u32,
) -> Vec<u8> {
    rewrite_init_segment(init_segment, move |moov_box| {
        let track_id = moov_box.trak_boxes[0].tkhd_box.track_id;
        moov_box.trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stsd_box
            .entries
            .push(sample_entry.clone());
        let trex_box = moov_box
            .mvex_box
            .as_mut()
            .expect("muxer-generated init segment must contain mvex")
            .trex_boxes
            .iter_mut()
            .find(|trex_box| trex_box.track_id == track_id)
            .expect("trex for first track must exist");
        trex_box.default_sample_description_index = default_sample_description_index;
    })
}

fn rewrite_media_segment_tfhd_sample_description_index(
    media_segment: &[u8],
    sample_description_index: Option<u32>,
) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("failed to decode moof from media segment");
    for traf_box in &mut moof_box.traf_boxes {
        traf_box.tfhd_box.sample_description_index = sample_description_index;
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("failed to encode moof while rewriting media segment");
    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

fn rewrite_media_segment_mdat_size_zero(media_segment: &[u8]) -> Vec<u8> {
    let (_moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("failed to decode moof from media segment");
    let mdat_size_offset = moof_box_size;
    assert!(
        media_segment.len() >= mdat_size_offset + 8,
        "media segment must contain an mdat header after moof"
    );

    let mut rewritten = media_segment.to_vec();
    rewritten[mdat_size_offset..mdat_size_offset + 4].copy_from_slice(&0u32.to_be_bytes());
    rewritten
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
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

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

            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// 単一音声トラックの roundtrip
    #[test]
    fn audio_only_roundtrip(
        samples in prop::collection::vec(arb_audio_sample(0), 1..10),
    ) {
        let sample_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| audio_segment_sample(&sample_entry, sample))
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

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
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// 同一トラック内で 2 番目以降のサンプルが `sample_entry: None` でも
    /// 直前に観測した sample entry を継承してメディアセグメントを生成できることを確認する
    #[test]
    fn sample_entry_can_be_omitted_after_first_sample(
        width in 64u16..1921,
        height in 64u16..1081,
        samples in prop::collection::vec(arb_video_sample(0), 2..10),
    ) {
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let mut segment_sample = video_segment_sample(&sample_entry, sample, None);
                if index > 0 {
                    segment_sample.sample_entry = None;
                }
                segment_sample
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed.len(), samples.len());
        prop_assert_eq!(demuxed[0].sample_entry, Some(&sample_entry));
        for sample in demuxed.iter().skip(1) {
            prop_assert!(sample.sample_entry.is_none());
        }

        for (orig, demuxed_sample) in samples.iter().zip(demuxed.iter()) {
            prop_assert_eq!(demuxed_sample.duration, orig.duration);
            prop_assert_eq!(demuxed_sample.keyframe, orig.keyframe);
            prop_assert_eq!(demuxed_sample.data_size, orig.data.len());

            let actual = &segment_bytes[demuxed_sample.data_offset as usize
                ..demuxed_sample.data_offset as usize + demuxed_sample.data_size];
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
        let video_sample_entry = create_avc1_sample_entry(width, height);
        let audio_sample_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("failed to create muxer");

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

        let fmp4_samples: Vec<SegmentSample> = all_samples
            .iter()
            .map(|sample| {
                if sample.track_index == 0 {
                    video_segment_sample(&video_sample_entry, sample, None)
                } else {
                    audio_segment_sample(&audio_sample_entry, sample)
                }
            })
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

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

        let demuxed_video: Vec<_> = demuxed.iter().filter(|s| s.track.track_id == 1).collect();
        prop_assert_eq!(demuxed_video.len(), video_samples.len());
        for (orig, ds) in video_samples.iter().zip(demuxed_video.iter()) {
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }

        let demuxed_audio: Vec<_> = demuxed.iter().filter(|s| s.track.track_id == 2).collect();
        prop_assert_eq!(demuxed_audio.len(), audio_samples.len());
        for (orig, ds) in audio_samples.iter().zip(demuxed_audio.iter()) {
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// composition_time_offset が roundtrip で保持されることを確認する
    #[test]
    fn composition_time_offset_roundtrip(
        samples_with_cto in prop::collection::vec(arb_video_sample_with_cto(0), 1..10),
    ) {
        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let fmp4_samples: Vec<SegmentSample> = samples_with_cto
            .iter()
            .map(|(sample, cto)| video_segment_sample(&sample_entry, sample, *cto))
            .collect();

        let segment_bytes = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

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
                Some(i64::from(expected_cto.unwrap_or(0)))
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

        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        for segment_samples in &segments {
            let fmp4_samples: Vec<SegmentSample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
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
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();

        // sidx 付きセグメントを生成する
        let segment_bytes = muxer
            .create_media_segment_with_sidx(&fmp4_samples)
            .expect("failed to create sidx segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

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

            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            prop_assert_eq!(actual, orig.data.as_slice());
        }
    }

    /// 最初のサンプルに `sample_entry` がない場合は
    /// `create_media_segment_with_sidx()` がエラーを返すことを確認する
    #[test]
    fn sidx_rejects_missing_sample_entry_on_first_sample(
        duration in 1u32..3001,
        keyframe in any::<bool>(),
        data in prop::collection::vec(any::<u8>(), 1..256),
    ) {
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");
        let sample = SegmentSample {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("non-zero"),
            sample_entry: None,
            duration,
            keyframe,
            composition_time_offset: None,
            data: &data,
        };

        let result = muxer.create_media_segment_with_sidx(&[sample]);

        match result {
            Err(shiguredo_mp4::mux::MuxError::MissingSampleEntry {
                track_kind,
            }) => {
                prop_assert_eq!(track_kind, TrackKind::Video);
            }
            other => prop_assert!(false, "unexpected result: {other:?}"),
        }
    }

    /// `trex.default_sample_description_index` が `stsd` の先頭以外を指す場合でも
    /// 対応する sample entry が使われることを確認する
    #[test]
    fn sample_entry_uses_trex_default_index(
        width1 in 64u16..1921,
        width2 in 64u16..1921,
        samples in prop::collection::vec(arb_video_sample(0), 1..5),
    ) {
        prop_assume!(width1 != width2);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let bootstrap_segment = muxer
            .create_media_segment(&[video_segment_sample(
                &alternative_sample_entry,
                &samples[0],
                None,
            )])
            .expect("failed to create bootstrap segment");
        prop_assert!(!bootstrap_segment.is_empty());
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
        let init_bytes = append_sample_entry_and_set_trex_default(&init_bytes, alternative_sample_entry.clone(), 2);

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| SegmentSample {
                sample_entry: Some(original_sample_entry.clone()),
                ..video_segment_sample(&original_sample_entry, sample, None)
            })
            .collect();
        let media_segment = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");
        let demuxed = demuxer
            .handle_media_segment(&media_segment)
            .expect("failed to handle media segment");

        prop_assert_eq!(
            demuxed[0].sample_entry,
            Some(&alternative_sample_entry),
        );
        for sample in demuxed.iter().skip(1) {
            prop_assert!(sample.sample_entry.is_none());
        }
    }

    /// `tfhd.sample_description_index` が `trex.default_sample_description_index` より優先されることを確認する
    #[test]
    fn sample_entry_prefers_tfhd_index(
        width1 in 64u16..1921,
        width2 in 64u16..1921,
        samples in prop::collection::vec(arb_video_sample(0), 1..5),
    ) {
        prop_assume!(width1 != width2);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");
        muxer
            .create_media_segment(&[video_segment_sample(
                &original_sample_entry,
                &samples[0],
                None,
            )])
            .expect("failed to create bootstrap segment");
        muxer
            .create_media_segment(&[video_segment_sample(
                &alternative_sample_entry,
                &samples[0],
                None,
            )])
            .expect("failed to create bootstrap segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
        let init_bytes = append_sample_entry_and_set_trex_default(&init_bytes, alternative_sample_entry, 2);

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let media_segment = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let media_segment =
            rewrite_media_segment_tfhd_sample_description_index(&media_segment, Some(1));

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");
        let demuxed = demuxer
            .handle_media_segment(&media_segment)
            .expect("failed to handle media segment");

        prop_assert_eq!(demuxed[0].sample_entry, Some(&original_sample_entry));
        for sample in demuxed.iter().skip(1) {
            prop_assert!(sample.sample_entry.is_none());
        }
    }

    /// sample description index が切り替わった最初のサンプルでだけ
    /// `sample_entry` が再度 `Some` になることを確認する
    #[test]
    fn sample_entry_is_emitted_only_on_change(
        width1 in 64u16..1921,
        width2 in 64u16..1921,
        first_segment_samples in prop::collection::vec(arb_video_sample(0), 2..5),
        second_segment_samples in prop::collection::vec(arb_video_sample(0), 2..5),
    ) {
        prop_assume!(width1 != width2);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let first_segment_input: Vec<SegmentSample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<SegmentSample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&alternative_sample_entry, sample, None))
            .collect();

        let first_segment = muxer
            .create_media_segment(&first_segment_input)
            .expect("failed to create first media segment");
        let second_segment = muxer
            .create_media_segment(&second_segment_input)
            .expect("failed to create second media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let first_demuxed = demuxer
            .handle_media_segment(&first_segment)
            .expect("failed to handle first media segment");
        prop_assert_eq!(
            first_demuxed[0].sample_entry,
            Some(&original_sample_entry),
        );
        for sample in first_demuxed.iter().skip(1) {
            prop_assert!(sample.sample_entry.is_none());
        }

        let second_demuxed = demuxer
            .handle_media_segment(&second_segment)
            .expect("failed to handle second media segment");
        prop_assert_eq!(
            second_demuxed[0].sample_entry,
            Some(&alternative_sample_entry),
        );
        for sample in second_demuxed.iter().skip(1) {
            prop_assert!(sample.sample_entry.is_none());
        }
    }

    /// `handle_media_segment()` が複数の `moof` + `mdat` ペアを 1 回で受け取った場合に
    /// 末尾データを黙って無視せずエラーを返すことを確認する
    #[test]
    fn rejects_multiple_moof_mdat_pairs_in_one_input(
        width in 64u16..1921,
        height in 64u16..1081,
        first_segment_samples in prop::collection::vec(arb_video_sample(0), 1..4),
        second_segment_samples in prop::collection::vec(arb_video_sample(0), 1..4),
    ) {
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let first_segment_input: Vec<SegmentSample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<SegmentSample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();

        let mut concatenated = muxer
            .create_media_segment(&first_segment_input)
            .expect("failed to create first media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
        concatenated.extend_from_slice(
            &muxer
                .create_media_segment(&second_segment_input)
                .expect("failed to create second media segment"),
        );

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");

        let result = demuxer.handle_media_segment(&concatenated);
        prop_assert!(matches!(result, Err(DemuxError::DecodeError(_))));
    }

    /// Fmp4FileDemuxer でも sample entry の切り替わりが反映されることを確認する
    #[test]
    fn fmp4_file_demuxer_propagates_sample_entry_changes(
        width1 in 64u16..1921,
        width2 in 64u16..1921,
        first_segment_samples in prop::collection::vec(arb_video_sample(0), 2..5),
        second_segment_samples in prop::collection::vec(arb_video_sample(0), 2..5),
    ) {
        prop_assume!(width1 != width2);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let first_segment_input: Vec<SegmentSample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<SegmentSample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&alternative_sample_entry, sample, None))
            .collect();

        let first_segment = muxer
            .create_media_segment(&first_segment_input)
            .expect("failed to create first media segment");
        let second_segment = muxer
            .create_media_segment(&second_segment_input)
            .expect("failed to create second media segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");

        let mut file_data = init_bytes;
        file_data.extend_from_slice(&first_segment);
        file_data.extend_from_slice(&second_segment);

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        let mut sample_entry_flags = Vec::new();
        loop {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break Some(sample),
                    Ok(None) => break None,
                    Err(DemuxError::InputRequired(_)) => feed_fmp4_file_demuxer(&mut demuxer, &file_data),
                    Err(error) => panic!("next_sample error: {error}"),
                }
            };

            let Some(sample) = sample else {
                break;
            };
            let sample_entry = sample.sample_entry.cloned();
            sample_entry_flags.push(sample_entry);
        }

        prop_assert_eq!(sample_entry_flags.len(), first_segment_samples.len() + second_segment_samples.len());
        prop_assert_eq!(sample_entry_flags[0].as_ref(), Some(&original_sample_entry));
        for sample_entry in sample_entry_flags.iter().take(first_segment_samples.len()).skip(1) {
            prop_assert!(sample_entry.is_none());
        }
        prop_assert_eq!(
            sample_entry_flags[first_segment_samples.len()].as_ref(),
            Some(&alternative_sample_entry),
        );
        for sample_entry in sample_entry_flags.iter().skip(first_segment_samples.len() + 1) {
            prop_assert!(sample_entry.is_none());
        }
    }

    /// 範囲外の sample description index はエラーになることを確認する
    #[test]
    fn invalid_sample_description_index_is_rejected(
        width1 in 64u16..1921,
        width2 in 64u16..1921,
        samples in prop::collection::vec(arb_video_sample(0), 1..5),
    ) {
        prop_assume!(width1 != width2);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");
        muxer
            .create_media_segment(&[video_segment_sample(
                &create_avc1_sample_entry(width2, 240),
                &samples[0],
                None,
            )])
            .expect("failed to create bootstrap segment");
        let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
        let init_bytes = append_sample_entry_and_set_trex_default(
            &init_bytes,
            create_avc1_sample_entry(width2, 240),
            1,
        );

        let fmp4_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let media_segment = muxer
            .create_media_segment(&fmp4_samples)
            .expect("failed to create media segment");
        let media_segment =
            rewrite_media_segment_tfhd_sample_description_index(&media_segment, Some(3));

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("failed to handle init segment");
        let result = demuxer.handle_media_segment(&media_segment);

        prop_assert!(matches!(
            result,
            Err(DemuxError::DecodeError(_))
        ));
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
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        // 全セグメントをひとつのバイト列に連結して完全な fMP4 ファイルを組み立てる
        let mut file_data = Vec::new();
        let mut all_samples: Vec<TestSample> = Vec::new();

        for segment_samples in &segments {
            let fmp4_samples: Vec<SegmentSample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let segment_bytes = muxer
                .create_media_segment(&fmp4_samples)
                .expect("failed to create media segment");
            all_samples.extend_from_slice(segment_samples);
            file_data.extend_from_slice(&segment_bytes);
        }
        let mut init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
        init_bytes.extend_from_slice(&file_data);
        let file_data = init_bytes;

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        // トラック情報の確認
        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert_eq!(tracks[0].kind, TrackKind::Video);
        prop_assert_eq!(tracks[0].timescale.get(), 90000);

        // サンプルを順番に取り出して元データと照合する
        let mut expected_decode_time: u64 = 0;
        for (i, orig) in all_samples.iter().enumerate() {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break sample,
                    Ok(None) => panic!("unexpected end of samples"),
                    Err(DemuxError::InputRequired(_)) => {
                        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
                    }
                    Err(error) => panic!("next_sample error: {error}"),
                }
            };

            prop_assert_eq!(sample.track.track_id, 1);
            prop_assert_eq!(sample.timestamp, expected_decode_time);
            prop_assert_eq!(sample.duration, orig.duration);
            prop_assert_eq!(sample.keyframe, orig.keyframe);
            prop_assert_eq!(
                &file_data[sample.data_offset as usize..sample.data_offset as usize + sample.data_size],
                orig.data.as_slice(),
            );
            prop_assert_eq!(sample.sample_entry.is_some(), i == 0);

            expected_decode_time += orig.duration as u64;
        }

        // 全サンプルを読み終えたら None が返ることを確認する
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
        let last = demuxer.next_sample().expect("next_sample error");
        prop_assert!(last.is_none(), "expected no more samples, got {:?}", last);
    }

    /// `mdat size=0` の media segment を含む fMP4 ファイルでも
    /// `Fmp4FileDemuxer` が末尾までの `mdat` として処理できることを確認する
    #[test]
    fn fmp4_file_demuxer_accepts_mdat_size_zero(
        width in 64u16..1921,
        height in 64u16..1081,
        samples in prop::collection::vec(arb_video_sample(0), 1..10),
    ) {
        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let segment_samples: Vec<SegmentSample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let media_segment = muxer
            .create_media_segment(&segment_samples)
            .expect("failed to create media segment");
        let media_segment = rewrite_media_segment_mdat_size_zero(&media_segment);

        let mut file_data = muxer.init_segment_bytes().expect("failed to build init segment");
        file_data.extend_from_slice(&media_segment);

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        let tracks = demuxer.tracks().expect("failed to get tracks");
        prop_assert_eq!(tracks.len(), 1);
        prop_assert_eq!(tracks[0].kind, TrackKind::Video);

        let mut expected_decode_time = 0u64;
        for (i, expected) in samples.iter().enumerate() {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break sample,
                    Ok(None) => panic!("unexpected end of samples"),
                    Err(DemuxError::InputRequired(_)) => {
                        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
                    }
                    Err(error) => panic!("next_sample error: {error}"),
                }
            };

            prop_assert_eq!(sample.track.track_id, 1);
            prop_assert_eq!(sample.timestamp, expected_decode_time);
            prop_assert_eq!(sample.duration, expected.duration);
            prop_assert_eq!(sample.keyframe, expected.keyframe);
            prop_assert_eq!(
                &file_data[sample.data_offset as usize..sample.data_offset as usize + sample.data_size],
                expected.data.as_slice(),
            );
            prop_assert_eq!(sample.sample_entry.is_some(), i == 0);
            expected_decode_time += expected.duration as u64;
        }

        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
        let last = demuxer.next_sample().expect("next_sample error");
        prop_assert!(last.is_none(), "expected no more samples, got {:?}", last);
    }

    /// timestamp が複数セグメントにわたって正しく累積されることを確認する
    #[test]
    fn timestamp_accumulation(
        samples_per_segment in prop::collection::vec(
            prop::collection::vec(arb_video_sample(0), 1..5),
            2..5,
        ),
    ) {
        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new failed");

        let mut expected_decode_time: u64 = 0;
        let mut demuxer = Fmp4SegmentDemuxer::new();
        let mut initialized = false;

        for segment_samples in &samples_per_segment {
            let fmp4_samples: Vec<SegmentSample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();

            let segment_bytes = muxer
                .create_media_segment(&fmp4_samples)
                .expect("failed to create media segment");
            if !initialized {
                let init_bytes = muxer.init_segment_bytes().expect("failed to build init segment");
                demuxer
                    .handle_init_segment(&init_bytes)
                    .expect("failed to handle init segment");
                initialized = true;
            }

            let demuxed = demuxer
                .handle_media_segment(&segment_bytes)
                .expect("failed to handle media segment");

            prop_assert_eq!(demuxed[0].timestamp, expected_decode_time);

            expected_decode_time +=
                segment_samples.iter().map(|s| s.duration as u64).sum::<u64>();
        }
    }
}
