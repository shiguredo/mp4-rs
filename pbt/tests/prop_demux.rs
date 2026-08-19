//! Demuxer の Property-Based Testing
//!
//! 破損した MP4 データで無限ループが発生する問題を再現・検出するテスト

use noprop::TestCaseContext;
use shiguredo_mp4::demux::{Input, Mp4FileDemuxer, RequiredInput, Sample};

/// このファイルの PBT ケース数（旧 `with_cases(200)` を維持）
const CASES: usize = 200;

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

/// 破損タイプを生成する
///
/// 旧 `prop_oneof!` の 4 分岐を `sample_weighted_index` で等確率選択する。
fn arb_corruption(ctx: &mut TestCaseContext, data_len: usize) -> CorruptionType {
    match noprop::sample_weighted_index(ctx, &[1, 1, 1, 1]) {
        0 => {
            let position = noprop::sample_usize_in(ctx, 0..data_len);
            let value = noprop::sample_u8(ctx);
            CorruptionType::SingleByte { position, value }
        }
        1 => {
            let start = noprop::sample_usize_in(ctx, 0..data_len);
            let len = noprop::sample_usize_in(ctx, 1..=64);
            CorruptionType::ZeroFill { start, len }
        }
        2 => {
            let start = noprop::sample_usize_in(ctx, 0..data_len);
            let values = noprop::sample_bytes::<8>(ctx);
            CorruptionType::RandomFill { start, values }
        }
        _ => {
            // 切り詰め（最低 8 バイトは残す）
            let new_len = noprop::sample_usize_in(ctx, 8..data_len);
            CorruptionType::Truncate { new_len }
        }
    }
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

/// 破損した H264 MP4 で無限ループが発生しないことを確認
#[test]
fn corrupted_h264_mp4_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let corruption = arb_corruption(ctx, TEST_MP4_H264.len());
        let corrupted = corrupt_mp4(TEST_MP4_H264, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// 破損した AAC MP4 で無限ループが発生しないことを確認
#[test]
fn corrupted_aac_mp4_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let corruption = arb_corruption(ctx, TEST_MP4_AAC.len());
        let corrupted = corrupt_mp4(TEST_MP4_AAC, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// 複数箇所を破損させた場合も無限ループが発生しないことを確認
#[test]
fn multi_corrupted_mp4_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let corruptions = sample_vec(ctx, 1..5, |ctx| arb_corruption(ctx, TEST_MP4_H264.len()));
        let mut data = TEST_MP4_H264.to_vec();
        for corruption in corruptions {
            data = corrupt_mp4(&data, corruption);
        }
        let result = demux_with_loop_detection(&data, 1000);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// ランダムバイト列で無限ループが発生しないことを確認
#[test]
fn random_bytes_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let len = noprop::sample_usize_in(ctx, 0..1024);
        let data = noprop::sample_bytes_vec(ctx, len);
        let result = demux_with_loop_detection(&data, 100);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// ボックスヘッダー付近の破損で無限ループが発生しないことを確認
#[test]
fn header_corruption_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let offset = noprop::sample_usize_in(ctx, 0..32);
        let value = noprop::sample_u8(ctx);
        let corruption = CorruptionType::SingleByte {
            position: offset,
            value,
        };
        let corrupted = corrupt_mp4(TEST_MP4_H264, corruption);
        let result = demux_with_loop_detection(&corrupted, 1000);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// サイズフィールドを極端な値に破損させた場合も無限ループが発生しないことを確認
#[test]
fn extreme_size_corruption_no_infinite_loop() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let size_bytes = noprop::sample_bytes::<4>(ctx);
        // 最初の 4 バイト（ftyp のサイズフィールド）を破損
        let mut corrupted = TEST_MP4_H264.to_vec();
        if corrupted.len() >= 4 {
            corrupted[0..4].copy_from_slice(&size_bytes);
        }
        let result = demux_with_loop_detection(&corrupted, 1000);
        assert!(result.is_ok(), "エラー: {:?}", result.err());
        Ok(())
    })?;
    Ok(())
}

/// prev_sample() が next_sample() と往復できることを確認
#[test]
fn prev_sample_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let file_choice = noprop::sample_u64_in(ctx, 0..2) as u8;
        let skip_samples = noprop::sample_usize_in(ctx, 0..20);
        let max_samples = noprop::sample_usize_in(ctx, 1..200);

        let data = if file_choice == 0 {
            TEST_MP4_AAC_FILE
        } else {
            TEST_MP4_H264_FILE
        };
        let input = Input { position: 0, data };
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(input);
        let _ = demuxer.tracks().expect("tracks の取得に失敗した");

        let mut skipped = 0usize;
        while skipped < skip_samples {
            match demuxer
                .next_sample()
                .expect("次の sample の読み取りに失敗した")
            {
                Some(_) => {
                    skipped += 1;
                }
                None => break,
            }
        }

        let mut forward = Vec::new();
        while let Some(sample) = demuxer
            .next_sample()
            .expect("次の sample の読み取りに失敗した")
        {
            forward.push(sample_to_digest(&sample));
            if forward.len() >= max_samples {
                break;
            }
        }
        if forward.is_empty() {
            ctx.reject_case();
        }

        let mut backward = Vec::new();
        for _ in 0..forward.len() {
            let sample = demuxer
                .prev_sample()
                .expect("前の sample の読み取りに失敗した");
            assert!(sample.is_some());
            backward.push(sample_to_digest(
                sample.as_ref().expect("sample が欠落している"),
            ));
        }
        backward.reverse();
        assert_eq!(backward.as_slice(), forward.as_slice());

        let mut forward_again = Vec::new();
        for _ in 0..forward.len() {
            let sample = demuxer
                .next_sample()
                .expect("次の sample の読み取りに失敗した");
            assert!(sample.is_some());
            forward_again.push(sample_to_digest(
                sample.as_ref().expect("sample が欠落している"),
            ));
        }
        assert_eq!(forward_again.as_slice(), forward.as_slice());
        Ok(())
    })?;
    Ok(())
}

/// seek() 後に next_sample() が指定時刻を含むサンプルを返すことを確認
#[test]
fn seek_returns_sample_containing_position() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let file_choice = noprop::sample_u64_in(ctx, 0..2) as u8;
        let seek_ticks_offset = noprop::sample_u64_in(ctx, 0..5_000);

        let data = if file_choice == 0 {
            TEST_MP4_AAC_FILE
        } else {
            TEST_MP4_H264_FILE
        };
        let input = Input { position: 0, data };
        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(input);
        let tracks = demuxer.tracks().expect("tracks の取得に失敗した").to_vec();
        if tracks.is_empty() {
            ctx.reject_case();
        }

        let max_duration = tracks
            .iter()
            .map(|track| ticks_to_duration(track.duration, track.timescale.get()))
            .max()
            .expect("実装バグ");
        let offset_duration = std::time::Duration::from_millis(seek_ticks_offset);
        let seek_duration = max_duration / 2 + offset_duration;

        demuxer.seek(seek_duration).expect("seek に失敗した");
        let sample = demuxer.next_sample().expect("sample の読み取りに失敗した");

        let any_track_has_sample = tracks.iter().any(|track| {
            let track_seek_ticks = duration_to_ticks(seek_duration, track.timescale.get());
            track_seek_ticks < track.duration
        });

        if let Some(sample) = sample {
            let sample_seek_ticks = duration_to_ticks(seek_duration, sample.track.timescale.get());
            assert!(sample.timestamp <= sample_seek_ticks);
            assert!(sample_seek_ticks < sample.timestamp + u64::from(sample.duration));
        } else {
            assert!(!any_track_has_sample);
        }
        Ok(())
    })?;
    Ok(())
}

mod boundary_tests {
    use super::*;

    #[test]
    fn fixed_boundary_inputs_no_infinite_loop() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        // 固定入力を等確率で選ぶ
        let cases: Vec<Vec<u8>> = vec![
            Vec::new(),
            vec![0; 8],
            TEST_MP4_H264[..32.min(TEST_MP4_H264.len())].to_vec(),
            vec![0xFF; 256],
            vec![0x00; 256],
        ];
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let idx = noprop::sample_usize_in(ctx, 0..cases.len());
            let case = &cases[idx];
            let result = demux_with_loop_detection(case, 100);
            assert!(result.is_ok(), "エラー: {:?}", result.err());
            Ok(())
        })?;
        Ok(())
    }
}

// ===== RequiredInput のテスト =====

mod required_input_tests {
    use noprop::TestCaseContext;
    use shiguredo_mp4::demux::{Input, RequiredInput};

    /// このモジュールの PBT ケース数
    const CASES: usize = 200;

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

    #[test]
    fn is_satisfied_by_matches_reference_implementation() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let required_position = noprop::sample_u64(ctx);
            let required_size = if noprop::sample_bool(ctx) {
                Some(noprop::sample_usize_in(ctx, 0..2048))
            } else {
                None
            };
            let input_position = noprop::sample_u64(ctx);
            let input_len = noprop::sample_usize_in(ctx, 0..2048);

            let data = vec![0u8; input_len];
            let required = RequiredInput {
                position: required_position,
                size: required_size,
            };
            let input = Input {
                position: input_position,
                data: &data,
            };

            assert_eq!(
                required.is_satisfied_by(input),
                reference_is_satisfied_by(required, input_position, input_len),
            );
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn to_input_sets_position_and_data() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let position = noprop::sample_u64(ctx);
            let size = if noprop::sample_bool(ctx) {
                Some(noprop::sample_usize_in(ctx, 0..2048))
            } else {
                None
            };
            let data = sample_vec(ctx, 0..2048, noprop::sample_u8);

            let required = RequiredInput { position, size };
            let input = required.to_input(&data);

            assert_eq!(input.position, position);
            assert_eq!(input.data, data.as_slice());
            Ok(())
        })?;
        Ok(())
    }
}

// ===== DemuxError のテスト =====

mod demux_error_tests {
    use shiguredo_mp4::{
        aux::SampleTableAccessorError,
        demux::{DemuxError, RequiredInput},
    };

    /// このモジュールの PBT ケース数
    const CASES: usize = 200;

    #[test]
    fn demux_error_from_sample_table_error() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let chunk_count = noprop::sample_u32(ctx);
            let error = SampleTableAccessorError::ChunksExistButNoSamples { chunk_count };
            let demux_error: DemuxError = error.into();
            let debug_str = format!("{demux_error:?}");

            assert!(debug_str.contains(&chunk_count.to_string()));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn demux_error_input_required() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let position = noprop::sample_u64(ctx);
            let size = if noprop::sample_bool(ctx) {
                Some(noprop::sample_usize_in(ctx, 0..2048))
            } else {
                None
            };
            let required = RequiredInput { position, size };
            let demux_error = DemuxError::InputRequired(required);
            let debug_str = format!("{demux_error:?}");

            assert!(debug_str.contains(&position.to_string()));
            Ok(())
        })?;
        Ok(())
    }
}

mod handle_input_validation_tests {
    use shiguredo_mp4::demux::{DemuxError, Input, Mp4FileDemuxer};

    const TEST_MP4_AAC_FILE: &[u8] = include_bytes!("../../tests/testdata/beep-aac-audio.mp4");

    /// このモジュールの PBT ケース数
    const CASES: usize = 200;

    #[test]
    fn wrong_position_input_is_rejected() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let wrong_position = noprop::sample_u64_in(ctx, 1..2048);

            let mut demuxer = Mp4FileDemuxer::new();
            let start = usize::try_from(wrong_position)
                .ok()
                .map(|position| position.min(TEST_MP4_AAC_FILE.len()))
                .expect("サポート対象では usize への変換は失敗しない");
            let input = Input {
                position: wrong_position,
                data: &TEST_MP4_AAC_FILE[start..],
            };

            demuxer.handle_input(input);

            assert!(matches!(demuxer.tracks(), Err(DemuxError::DecodeError(_))));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn insufficient_initial_input_is_rejected() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let input_len = noprop::sample_usize_in(ctx, 0..8);

            let mut demuxer = Mp4FileDemuxer::new();
            let input = Input {
                position: 0,
                data: &TEST_MP4_AAC_FILE[..input_len],
            };

            demuxer.handle_input(input);

            assert!(matches!(demuxer.tracks(), Err(DemuxError::DecodeError(_))));
            Ok(())
        })?;
        Ok(())
    }
}
