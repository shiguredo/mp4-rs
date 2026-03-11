//! Demuxer の Property-Based Testing
//!
//! 破損した MP4 データで無限ループが発生する問題を再現・検出するテスト

use proptest::prelude::*;
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer, RequiredInput, Sample};

/// テスト用の簡易 MP4 風データ
const TEST_MP4_H264: &[u8] = &[
    0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0x00, 0x00, 0x00, 0x00,
    b'i', b's', b'o', b'm', b'i', b's', b'o', b'2', 0x00, 0x00, 0x00, 0x08, b'm', b'o', b'o', b'v',
    0x00, 0x00, 0x00, 0x10, b'm', b'd', b'a', b't', 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05,
];
const TEST_MP4_AAC: &[u8] = &[
    0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2', 0x00, 0x00, 0x00, 0x00,
    b'm', b'p', b'4', b'2', b'i', b's', b'o', b'm', 0x00, 0x00, 0x00, 0x08, b'm', b'o', b'o', b'v',
    0x00, 0x00, 0x00, 0x10, b'm', b'd', b'a', b't', 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80,
];
const TEST_MP4_AAC_FILE: &[u8] = include_bytes!("../../tests/testdata/beep-aac-audio.mp4");
const TEST_MP4_H264_FILE: &[u8] = include_bytes!("../../tests/testdata/black-h264-video.mp4");

/// 破損の種類
#[derive(Debug, Clone, Copy)]
enum CorruptionType {
    /// 単一バイトを変更
    SingleByte { position: usize, value: u8 },
    /// 複数バイトをゼロで埋める
    ZeroFill { start: usize, len: usize },
    /// 複数バイトをランダム値で埋める
    RandomFill { start: usize, values: [u8; 8] },
    /// バイトを削除（切り詰め）
    Truncate { new_len: usize },
}

/// MP4 データを破損させる
fn corrupt_mp4(data: &[u8], corruption: CorruptionType) -> Vec<u8> {
    let mut corrupted = data.to_vec();

    match corruption {
        CorruptionType::SingleByte { position, value } => {
            if position < corrupted.len() {
                corrupted[position] = value;
            }
        }
        CorruptionType::ZeroFill { start, len } => {
            let end = (start + len).min(corrupted.len());
            if start < corrupted.len() {
                for byte in &mut corrupted[start..end] {
                    *byte = 0;
                }
            }
        }
        CorruptionType::RandomFill { start, values } => {
            for (i, &v) in values.iter().enumerate() {
                if start + i < corrupted.len() {
                    corrupted[start + i] = v;
                }
            }
        }
        CorruptionType::Truncate { new_len } => {
            corrupted.truncate(new_len);
        }
    }

    corrupted
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SampleDigest {
    track_id: u32,
    timestamp: u64,
    duration: u32,
    data_offset: u64,
    data_size: usize,
    keyframe: bool,
    sample_entry_present: bool,
}

fn sample_to_digest(sample: &Sample<'_>) -> SampleDigest {
    SampleDigest {
        track_id: sample.track.track_id,
        timestamp: sample.timestamp,
        duration: sample.duration,
        data_offset: sample.data_offset,
        data_size: sample.data_size,
        keyframe: sample.keyframe,
        sample_entry_present: sample.sample_entry.is_some(),
    }
}

fn ticks_to_duration(ticks: u64, timescale: u32) -> std::time::Duration {
    let timescale = u64::from(timescale);
    let secs = ticks / timescale;
    let rem = ticks % timescale;
    let nanos = rem * 1_000_000_000 / timescale;
    std::time::Duration::new(secs, nanos as u32)
}

fn duration_to_ticks(duration: std::time::Duration, timescale: u32) -> u64 {
    let timescale = u64::from(timescale);
    let secs_part = duration.as_secs() * timescale;
    let nanos_part = u64::from(duration.subsec_nanos()) * timescale / 1_000_000_000;
    secs_part + nanos_part
}

/// 破損タイプを生成する Strategy
fn arb_corruption(data_len: usize) -> impl Strategy<Value = CorruptionType> {
    prop_oneof![
        // 単一バイト変更
        (0..data_len, any::<u8>())
            .prop_map(|(position, value)| CorruptionType::SingleByte { position, value }),
        // ゼロ埋め
        (0..data_len, 1usize..=64).prop_map(|(start, len)| CorruptionType::ZeroFill { start, len }),
        // ランダム埋め
        (0..data_len, any::<[u8; 8]>())
            .prop_map(|(start, values)| CorruptionType::RandomFill { start, values }),
        // 切り詰め（最低 8 バイトは残す）
        (8usize..data_len).prop_map(|new_len| CorruptionType::Truncate { new_len }),
    ]
}

/// Demuxer が無限ループに陥らないことを確認する
///
/// 同じ RequiredInput が連続して返された場合は無限ループとみなす
fn demux_with_loop_detection(data: &[u8], max_iterations: usize) -> Result<(), String> {
    let mut demuxer = Mp4FileDemuxer::new();
    let mut last_required: Option<RequiredInput> = None;
    let mut same_request_count = 0;
    const MAX_SAME_REQUESTS: usize = 3;

    for iteration in 0..max_iterations {
        // 必要な入力を確認
        let required = demuxer.required_input();

        if let Some(req) = required {
            // 同じリクエストが繰り返されているかチェック
            if last_required == Some(req) {
                same_request_count += 1;
                if same_request_count >= MAX_SAME_REQUESTS {
                    return Err(format!(
                        "無限ループ検出: 同じ入力要求が {} 回繰り返された (position={}, size={:?}) at iteration {}",
                        same_request_count, req.position, req.size, iteration
                    ));
                }
            } else {
                same_request_count = 0;
                last_required = Some(req);
            }

            // データを提供
            let input = Input { position: 0, data };
            demuxer.handle_input(input);
        } else {
            // 初期化完了またはエラー
            break;
        }
    }

    // tracks() を呼んでエラーをチェック
    match demuxer.tracks() {
        Ok(_) => {
            // サンプルも読んでみる
            let mut sample_iterations = 0;
            loop {
                match demuxer.next_sample() {
                    Ok(Some(_)) => {
                        sample_iterations += 1;
                        if sample_iterations > max_iterations {
                            return Err("サンプル読み取りで反復回数超過".to_string());
                        }
                    }
                    Ok(None) => break,
                    Err(_) => break, // エラーは許容（破損データなので）
                }
            }
            Ok(())
        }
        Err(_) => Ok(()), // エラーは許容（破損データなので）
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// 破損した H264 MP4 で無限ループが発生しないことを確認
    #[test]
    fn corrupted_h264_mp4_no_infinite_loop(corruption in arb_corruption(TEST_MP4_H264.len())) {
        let corrupted = corrupt_mp4(TEST_MP4_H264, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// 破損した AAC MP4 で無限ループが発生しないことを確認
    #[test]
    fn corrupted_aac_mp4_no_infinite_loop(corruption in arb_corruption(TEST_MP4_AAC.len())) {
        let corrupted = corrupt_mp4(TEST_MP4_AAC, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// 複数箇所を破損させた場合も無限ループが発生しないことを確認
    #[test]
    fn multi_corrupted_mp4_no_infinite_loop(
        corruptions in prop::collection::vec(arb_corruption(TEST_MP4_H264.len()), 1..5)
    ) {
        let mut data = TEST_MP4_H264.to_vec();
        for corruption in corruptions {
            data = corrupt_mp4(&data, corruption);
        }
        let result = demux_with_loop_detection(&data, 1000);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// ランダムバイト列で無限ループが発生しないことを確認
    #[test]
    fn random_bytes_no_infinite_loop(data in prop::collection::vec(any::<u8>(), 0..1024)) {
        let result = demux_with_loop_detection(&data, 100);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// ボックスヘッダー付近の破損で無限ループが発生しないことを確認
    #[test]
    fn header_corruption_no_infinite_loop(
        offset in 0usize..32,
        value in any::<u8>()
    ) {
        let corruption = CorruptionType::SingleByte { position: offset, value };
        let corrupted = corrupt_mp4(TEST_MP4_H264, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// サイズフィールドを極端な値に破損させた場合も無限ループが発生しないことを確認
    #[test]
    fn extreme_size_corruption_no_infinite_loop(
        size_bytes in prop::array::uniform4(any::<u8>())
    ) {
        // 最初の 4 バイト（ftyp のサイズフィールド）を破損
        let mut corrupted = TEST_MP4_H264.to_vec();
        if corrupted.len() >= 4 {
            corrupted[0..4].copy_from_slice(&size_bytes);
        }
        let result = demux_with_loop_detection(&corrupted, 1000);
        prop_assert!(result.is_ok(), "Error: {:?}", result.err());
    }

    /// prev_sample() が next_sample() と往復できることを確認
    #[test]
    fn prev_sample_roundtrip(
        file_choice in 0u8..2,
        skip_samples in 0usize..20,
        max_samples in 1usize..200
    ) {
        let data = if file_choice == 0 {
            TEST_MP4_AAC_FILE
        } else {
            TEST_MP4_H264_FILE
        };
        let input = Input {
            position: 0,
            data,
        };
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(input);
        let _ = demuxer.tracks().expect("failed to get tracks");

        let mut skipped = 0usize;
        while skipped < skip_samples {
            match demuxer.next_sample().expect("failed to read next sample") {
                Some(_) => {
                    skipped += 1;
                }
                None => break,
            }
        }

        let mut forward = Vec::new();
        while let Some(sample) = demuxer.next_sample().expect("failed to read next sample") {
            forward.push(sample_to_digest(&sample));
            if forward.len() >= max_samples {
                break;
            }
        }
        prop_assume!(!forward.is_empty());

        let mut backward = Vec::new();
        for _ in 0..forward.len() {
            let sample = demuxer.prev_sample().expect("failed to read previous sample");
            prop_assert!(sample.is_some());
            backward.push(sample_to_digest(sample.as_ref().expect("missing sample")));
        }
        backward.reverse();
        prop_assert_eq!(backward.as_slice(), forward.as_slice());

        let mut forward_again = Vec::new();
        for _ in 0..forward.len() {
            let sample = demuxer.next_sample().expect("failed to read next sample");
            prop_assert!(sample.is_some());
            forward_again.push(sample_to_digest(sample.as_ref().expect("missing sample")));
        }
        prop_assert_eq!(forward_again.as_slice(), forward.as_slice());
    }

    /// seek() 後に next_sample() が指定時刻を含むサンプルを返すことを確認
    #[test]
    fn seek_returns_sample_containing_position(
        file_choice in 0u8..2,
        seek_ticks_offset in 0u64..5_000u64
    ) {
        let data = if file_choice == 0 {
            TEST_MP4_AAC_FILE
        } else {
            TEST_MP4_H264_FILE
        };
        let input = Input {
            position: 0,
            data,
        };
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(input);
        let tracks = demuxer.tracks().expect("failed to get tracks").to_vec();
        prop_assume!(!tracks.is_empty());

        let max_duration = tracks
            .iter()
            .map(|track| ticks_to_duration(track.duration, track.timescale.get()))
            .max()
            .expect("bug");
        let offset_duration = std::time::Duration::from_millis(seek_ticks_offset);
        let seek_duration = max_duration / 2 + offset_duration;

        demuxer.seek(seek_duration).expect("failed to seek");
        let sample = demuxer.next_sample().expect("failed to read sample");

        let any_track_has_sample = tracks.iter().any(|track| {
            let track_seek_ticks = duration_to_ticks(seek_duration, track.timescale.get());
            track_seek_ticks < track.duration
        });

        if let Some(sample) = sample {
            let sample_seek_ticks = duration_to_ticks(seek_duration, sample.track.timescale.get());
            prop_assert!(sample.timestamp <= sample_seek_ticks);
            prop_assert!(sample_seek_ticks < sample.timestamp + u64::from(sample.duration));
        } else {
            prop_assert!(!any_track_has_sample);
        }
    }
}

mod boundary_tests {
    use super::*;

    proptest! {
        #[test]
        fn fixed_boundary_inputs_no_infinite_loop(case in prop::sample::select(vec![
            Vec::new(),
            vec![0; 8],
            TEST_MP4_H264[..32.min(TEST_MP4_H264.len())].to_vec(),
            vec![0xFF; 256],
            vec![0x00; 256],
        ])) {
            let result = demux_with_loop_detection(&case, 100);
            prop_assert!(result.is_ok(), "Error: {:?}", result.err());
        }
    }
}

// ===== RequiredInput のテスト =====

mod required_input_tests {
    use proptest::prelude::*;
    use shiguredo_mp4::demux::{Input, RequiredInput};

    fn reference_is_satisfied_by(
        required: RequiredInput,
        input_position: u64,
        input_len: usize,
    ) -> bool {
        let Some(offset) = required.position.checked_sub(input_position) else {
            return false;
        };

        if offset > input_len as u64 {
            return false;
        }

        let Some(required_size) = required.size else {
            return true;
        };

        let Some(end) = offset.checked_add(required_size as u64) else {
            return false;
        };

        end <= input_len as u64
    }

    proptest! {
        #[test]
        fn is_satisfied_by_matches_reference_implementation(
            required_position in any::<u64>(),
            required_size in prop::option::of(0usize..2048),
            input_position in any::<u64>(),
            input_len in 0usize..2048,
        ) {
            let data = vec![0u8; input_len];
            let required = RequiredInput {
                position: required_position,
                size: required_size,
            };
            let input = Input {
                position: input_position,
                data: &data,
            };

            prop_assert_eq!(
                required.is_satisfied_by(input),
                reference_is_satisfied_by(required, input_position, input_len),
            );
        }

        #[test]
        fn to_input_sets_position_and_data(
            position in any::<u64>(),
            size in prop::option::of(0usize..2048),
            data in prop::collection::vec(any::<u8>(), 0..2048),
        ) {
            let required = RequiredInput { position, size };
            let input = required.to_input(&data);

            prop_assert_eq!(input.position, position);
            prop_assert_eq!(input.data, data.as_slice());
        }
    }
}

// ===== DemuxError のテスト =====

mod demux_error_tests {
    use proptest::prelude::*;
    use shiguredo_mp4::{
        aux::SampleTableAccessorError,
        demux::{DemuxError, RequiredInput},
    };

    proptest! {
        #[test]
        fn demux_error_from_sample_table_error(chunk_count in any::<u32>()) {
            let error = SampleTableAccessorError::ChunksExistButNoSamples { chunk_count };
            let demux_error: DemuxError = error.into();
            let debug_str = format!("{:?}", demux_error);

            prop_assert!(debug_str.contains(&chunk_count.to_string()));
        }

        #[test]
        fn demux_error_input_required(
            position in any::<u64>(),
            size in prop::option::of(0usize..2048),
        ) {
            let required = RequiredInput { position, size };
            let demux_error = DemuxError::InputRequired(required);
            let debug_str = format!("{:?}", demux_error);

            prop_assert!(debug_str.contains(&position.to_string()));
        }
    }
}

mod handle_input_validation_tests {
    use proptest::prelude::*;
    use shiguredo_mp4::demux::{DemuxError, Input, Mp4FileDemuxer};

    const TEST_MP4_AAC_FILE: &[u8] = include_bytes!("../../tests/testdata/beep-aac-audio.mp4");

    proptest! {
        #[test]
        fn wrong_position_input_is_rejected(
            wrong_position in 1u64..2048,
        ) {
            let mut demuxer = Mp4FileDemuxer::new();
            let start = usize::try_from(wrong_position)
                .ok()
                .map(|position| position.min(TEST_MP4_AAC_FILE.len()))
                .expect("usize conversion must succeed on supported targets");
            let input = Input {
                position: wrong_position,
                data: &TEST_MP4_AAC_FILE[start..],
            };

            demuxer.handle_input(input);

            prop_assert!(matches!(
                demuxer.tracks(),
                Err(DemuxError::DecodeError(_))
            ));
        }

        #[test]
        fn insufficient_initial_input_is_rejected(
            input_len in 0usize..8,
        ) {
            let mut demuxer = Mp4FileDemuxer::new();
            let input = Input {
                position: 0,
                data: &TEST_MP4_AAC_FILE[..input_len],
            };

            demuxer.handle_input(input);

            prop_assert!(matches!(
                demuxer.tracks(),
                Err(DemuxError::DecodeError(_))
            ));
        }
    }
}
