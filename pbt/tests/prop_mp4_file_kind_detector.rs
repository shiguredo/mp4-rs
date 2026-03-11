//! MP4 / fMP4 のファイル種別判定に関する Property-Based Testing

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    TrackKind, Uint,
    boxes::{Avc1Box, AvccBox, SampleEntry, VisualSampleEntryFields},
    demux::{DemuxError, Input, Mp4FileKind, Mp4FileKindDetector},
    mux::{Fmp4SegmentMuxer, Mp4FileMuxer, Mp4FileMuxerOptions, Sample},
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

fn build_file_data(
    initial_bytes: &[u8],
    offset_and_bytes_pairs: &[(u64, Vec<u8>)],
    sample_data_size: usize,
) -> Vec<u8> {
    let total_size = initial_bytes.len()
        + sample_data_size
        + 1024
        + offset_and_bytes_pairs
            .iter()
            .map(|(offset, bytes)| *offset as usize + bytes.len())
            .max()
            .unwrap_or(0);
    let mut file_data = vec![0u8; total_size];
    file_data[..initial_bytes.len()].copy_from_slice(initial_bytes);

    let mut max_end = initial_bytes.len() + sample_data_size;
    for (offset, bytes) in offset_and_bytes_pairs {
        let offset = *offset as usize;
        file_data[offset..offset + bytes.len()].copy_from_slice(bytes);
        let end = offset + bytes.len();
        if end > max_end {
            max_end = end;
        }
    }
    file_data.truncate(max_end);
    file_data
}

fn build_regular_mp4_file_data(
    width: u16,
    height: u16,
    sample_sizes: &[usize],
    faststart: bool,
) -> (Vec<u8>, usize) {
    let options = Mp4FileMuxerOptions {
        reserved_moov_box_size: if faststart { 8192 } else { 0 },
        ..Mp4FileMuxerOptions::default()
    };
    let mut muxer = Mp4FileMuxer::with_options(options).expect("failed to create muxer");
    let initial_bytes = muxer.initial_boxes_bytes().to_vec();
    let mut data_offset = initial_bytes.len() as u64;
    let mut sample_entry = Some(create_avc1_sample_entry(width, height));
    let mut total_data_size = 0usize;

    for (index, data_size) in sample_sizes.iter().copied().enumerate() {
        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: sample_entry.take(),
            keyframe: index == 0,
            timescale: NonZeroU32::new(90_000).expect("non-zero"),
            duration: 3_000,
            composition_time_offset: None,
            data_offset,
            data_size,
        };
        muxer
            .append_sample(&sample)
            .expect("failed to append sample");
        data_offset += data_size as u64;
        total_data_size += data_size;
    }

    let finalized = muxer.finalize().expect("failed to finalize");
    let offset_and_bytes_pairs: Vec<_> = finalized
        .offset_and_bytes_pairs()
        .map(|(offset, bytes)| (offset, bytes.to_vec()))
        .collect();
    let file_data = build_file_data(&initial_bytes, &offset_and_bytes_pairs, total_data_size);
    let moov_offset = initial_bytes.len() + total_data_size;
    (file_data, moov_offset)
}

fn build_fragmented_mp4_file_data(width: u16, height: u16, sample_sizes: &[usize]) -> Vec<u8> {
    let mut muxer = Fmp4SegmentMuxer::new().expect("failed to create Fmp4SegmentMuxer");
    let sample_entry = create_avc1_sample_entry(width, height);

    let sample_payloads: Vec<Vec<u8>> = sample_sizes.iter().map(|size| vec![0u8; *size]).collect();
    let mut payload_offset = 0u64;
    let segment_samples: Vec<Sample> = sample_payloads
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let sample = Sample {
                track_kind: TrackKind::Video,
                timescale: NonZeroU32::new(90_000).expect("non-zero"),
                sample_entry: Some(sample_entry.clone()),
                duration: 3_000,
                keyframe: index == 0,
                composition_time_offset: None,
                data_offset: payload_offset,
                data_size: payload.len(),
            };
            payload_offset += payload.len() as u64;
            sample
        })
        .collect();
    let media_segment_metadata = muxer
        .create_media_segment_metadata(&segment_samples)
        .expect("failed to build media segment");
    let mut media_segment = media_segment_metadata;
    for payload in &sample_payloads {
        media_segment.extend_from_slice(payload);
    }
    let mut file_data = muxer
        .init_segment_bytes()
        .expect("failed to build init segment");
    file_data.extend_from_slice(&media_segment);
    file_data
}

fn feed_detector(detector: &mut Mp4FileKindDetector, file_data: &[u8], extra_sizes: &[usize]) {
    let mut step = 0usize;
    while let Some(required) = detector.required_input() {
        let start = required.position as usize;
        let extra = extra_sizes.get(step).copied().unwrap_or(0);
        let data = if start > file_data.len() {
            &[]
        } else {
            let end = required
                .size
                .map(|size| start.saturating_add(size.saturating_add(extra)))
                .unwrap_or(file_data.len())
                .min(file_data.len());
            file_data.get(start..end).unwrap_or(&[])
        };
        detector.handle_input(Input {
            position: required.position,
            data,
        });
        step += 1;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn detect_regular_mp4(
        width in 64u16..1921,
        height in 64u16..1081,
        faststart in any::<bool>(),
        sample_sizes in prop::collection::vec(1usize..2048, 1..8),
        extra_sizes in prop::collection::vec(0usize..256, 0..16),
    ) {
        let (file_data, _moov_offset) =
            build_regular_mp4_file_data(width, height, &sample_sizes, faststart);
        let mut detector = Mp4FileKindDetector::new();
        feed_detector(&mut detector, &file_data, &extra_sizes);

        prop_assert_eq!(
            detector.file_kind().expect("file_kind failed"),
            Some(Mp4FileKind::Mp4)
        );
        prop_assert!(detector.required_input().is_none());
    }

    #[test]
    fn detect_fragmented_mp4(
        width in 64u16..1921,
        height in 64u16..1081,
        sample_sizes in prop::collection::vec(1usize..1024, 1..8),
        extra_sizes in prop::collection::vec(0usize..256, 0..8),
    ) {
        let file_data = build_fragmented_mp4_file_data(width, height, &sample_sizes);
        let mut detector = Mp4FileKindDetector::new();
        feed_detector(&mut detector, &file_data, &extra_sizes);

        prop_assert_eq!(
            detector.file_kind().expect("file_kind failed"),
            Some(Mp4FileKind::FragmentedMp4)
        );
        prop_assert!(detector.required_input().is_none());
    }

    #[test]
    fn non_faststart_mp4_stays_unknown_before_moov(
        width in 64u16..1921,
        height in 64u16..1081,
        sample_sizes in prop::collection::vec(1usize..2048, 1..8),
    ) {
        let (file_data, moov_offset) =
            build_regular_mp4_file_data(width, height, &sample_sizes, false);
        let mut detector = Mp4FileKindDetector::new();

        while let Some(required) = detector.required_input() {
            if required.position as usize >= moov_offset {
                break;
            }

            let start = required.position as usize;
            let end = start
                .saturating_add(required.size.expect("bug: detector must require sized input before moov"))
                .min(file_data.len());
            detector.handle_input(Input {
                position: required.position,
                data: &file_data[start..end],
            });
            prop_assert_eq!(detector.file_kind().expect("file_kind failed"), None);
        }

        feed_detector(&mut detector, &file_data, &[]);
        prop_assert_eq!(
            detector.file_kind().expect("file_kind failed"),
            Some(Mp4FileKind::Mp4)
        );
    }

    #[test]
    fn eof_without_moov_returns_error(
        width in 64u16..1921,
        height in 64u16..1081,
        sample_sizes in prop::collection::vec(1usize..2048, 1..8),
    ) {
        let (file_data, moov_offset) =
            build_regular_mp4_file_data(width, height, &sample_sizes, false);
        let truncated = &file_data[..moov_offset];
        let mut detector = Mp4FileKindDetector::new();
        feed_detector(&mut detector, truncated, &[]);

        let err = detector.file_kind().expect_err("EOF without moov must fail");
        prop_assert!(matches!(err, DemuxError::DecodeError(_)));
    }
}
