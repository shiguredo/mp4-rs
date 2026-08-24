//! `shiguredo_mp4::bitstream::aac` の Property-Based Testing
//!
//! 受理条件を満たす ASC / ADTS を noprop サンプラーでランダム生成し、
//! `encode` / `parse`、`wrap` / `parse` の往復が意味を保つことを検証する。

use std::cell::Cell;

use shiguredo_mp4::bitstream::aac::{
    AdtsEncodeConfig, AdtsMpegVersion, AudioObjectType, AudioSpecificConfig, SamplingFrequency,
    encode_audio_specific_config, parse_adts_frame, parse_audio_specific_config,
    wrap_raw_aac_in_adts,
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// ADTS ヘッダー (CRC なし) の長さ。wrap の frame_length 境界計算に使う
const ADTS_HEADER_SIZE_NO_CRC: usize = 7;

/// ADTS の `frame_length` (13 ビット) の最大値
const ADTS_FRAME_LENGTH_MAX: usize = (1 << 13) - 1;

/// `samplingFrequencyIndex` (0..=12) に対応する実効サンプリング周波数 (Hz)
///
/// `SamplingFrequency::from_hz` が標準周波数を `Index` に写す経路を PBT で踏むための
/// 生成用テーブル
const SAMPLING_FREQUENCIES: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

/// 受理条件を満たす [`AudioSpecificConfig`] を生成する
///
/// - `sampling_frequency` は標準 Hz (index 形式) と任意 Hz (明示形式) を重み付きで混ぜる
/// - `channel_configuration` は 1..=7 を境界化する
fn sample_valid_asc(ctx: &mut noprop::TestCaseContext) -> AudioSpecificConfig {
    let hz = match noprop::sample_weighted_index(ctx, &[4, 1]) {
        0 => SAMPLING_FREQUENCIES[noprop::sample_u64_in(ctx, 0..=12) as usize],
        _ => noprop::sample_with_boundaries(
            ctx,
            &[1u32, 0xFF_FFFF],
            noprop::Ratio::one_nth(3),
            |ctx| noprop::sample_u64_in(ctx, 1..=0xFF_FFFF) as u32,
        ),
    };
    AudioSpecificConfig {
        audio_object_type: AudioObjectType::AacLc,
        sampling_frequency: SamplingFrequency::from_hz(hz).expect("有効な Hz は生成成功する"),
        channel_configuration: sample_channel_configuration(ctx),
    }
}

/// ADTS への wrap に使う、受理条件を満たす [`AudioSpecificConfig`] を生成する
///
/// ADTS に 24 ビット明示周波数は存在しないため、`sampling_frequency` は
/// 標準 Hz (index 形式) に閉じる (拒否サンプリングでケースを捨てない)
fn sample_valid_asc_for_adts(ctx: &mut noprop::TestCaseContext) -> AudioSpecificConfig {
    let index = noprop::sample_with_boundaries(ctx, &[0u8, 12], noprop::Ratio::one_nth(3), |ctx| {
        noprop::sample_u64_in(ctx, 0..=12) as u8
    });
    AudioSpecificConfig {
        audio_object_type: AudioObjectType::AacLc,
        sampling_frequency: SamplingFrequency::from_hz(SAMPLING_FREQUENCIES[index as usize])
            .expect("標準 Hz は生成成功する"),
        channel_configuration: sample_channel_configuration(ctx),
    }
}

/// 受理条件を満たす `channel_configuration` (1..=7) を生成する
fn sample_channel_configuration(ctx: &mut noprop::TestCaseContext) -> u8 {
    noprop::sample_with_boundaries(ctx, &[1u8, 7], noprop::Ratio::one_nth(3), |ctx| {
        noprop::sample_u64_in(ctx, 1..=7) as u8
    })
}

/// 受理条件を満たす [`AdtsEncodeConfig`] を生成する
fn sample_adts_config(ctx: &mut noprop::TestCaseContext) -> AdtsEncodeConfig {
    let mpeg_version = match noprop::sample_weighted_index(ctx, &[1, 1]) {
        0 => AdtsMpegVersion::Mpeg4,
        _ => AdtsMpegVersion::Mpeg2,
    };
    AdtsEncodeConfig {
        mpeg_version,
        original_copy: noprop::sample_bool(ctx),
        home: noprop::sample_bool(ctx),
    }
}

/// 受理条件を満たす ASC は `encode(parse(encode))` で往復し、正規形の長さが保たれる
///
/// - 標準 Hz (index 形式) は 2 バイト、非標準 Hz (明示形式) は 5 バイト
/// - 生成値は valid-by-construction なので `encode` が拒否しないこと
///
/// 明示形式は重み付き分岐なので、到達ゲート (encode 長 5 バイト) で踏まれていることを
/// 保証する (index / 明示の両経路のエンコードを検証する)
#[test]
fn asc_encode_parse_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let explicit_frequency_reached = Cell::new(false);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let config = sample_valid_asc(ctx);

        let encoded =
            encode_audio_specific_config(&config).expect("有効な ASC はエンコード成功する");
        // 正規形の長さ: 標準 Hz (index 形式) は 2 バイト、非標準 Hz (明示形式) は 5 バイト
        assert!(
            encoded.len() == 2 || encoded.len() == 5,
            "正規形の長さは 2 または 5 バイトであるべき"
        );
        if encoded.len() == 5 {
            explicit_frequency_reached.set(true);
        }

        let parsed =
            parse_audio_specific_config(&encoded).expect("エンコードした ASC は再解析成功する");
        assert_eq!(parsed, config, "encode → parse で ASC が往復する");
        Ok(())
    })?;
    assert!(
        explicit_frequency_reached.get(),
        "明示形式 (5 バイト) のケースが 1 件も生成されていない\n{runner}"
    );
    Ok(())
}

/// ADTS への wrap → parse で意味が往復する
///
/// - MPEG バージョン / original_copy / home / raw AAC ペイロードが保持される
/// - `frame_length` は `ADTS_HEADER_SIZE_NO_CRC + raw.len()` と一致する
/// - `protection_absent` は常に `true` (CRC なし固定)
///
/// `raw` の長さは 13 ビット境界 (8184) を含めて生成し、組み立て拒否を踏まない範囲の
/// 最大ケースも往復させる。MPEG-2 側の分岐が踏まれていることは到達ゲートで保証する
#[test]
fn adts_wrap_parse_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mpeg2_reached = Cell::new(false);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let asc = sample_valid_asc_for_adts(ctx);
        let config = sample_adts_config(ctx);
        if matches!(config.mpeg_version, AdtsMpegVersion::Mpeg2) {
            mpeg2_reached.set(true);
        }
        let raw_len = noprop::sample_with_boundaries(
            ctx,
            &[0usize, ADTS_FRAME_LENGTH_MAX - ADTS_HEADER_SIZE_NO_CRC],
            noprop::Ratio::one_nth(4),
            |ctx| noprop::sample_usize_in(ctx, 0..=ADTS_FRAME_LENGTH_MAX - ADTS_HEADER_SIZE_NO_CRC),
        );
        let raw = noprop::sample_bytes_vec(ctx, raw_len);

        let frame =
            wrap_raw_aac_in_adts(&raw, &asc, &config).expect("有効な入力は組み立て成功する");
        assert_eq!(frame.len(), ADTS_HEADER_SIZE_NO_CRC + raw.len());

        let (header, parsed_raw) =
            parse_adts_frame(&frame).expect("組み立てたフレームは解析成功する");
        assert_eq!(header.mpeg_version, config.mpeg_version);
        assert!(header.protection_absent, "組み立ては CRC なし固定");
        assert_eq!(header.audio_object_type, AudioObjectType::AacLc);
        // ADTS 用サンプラーは標準 Hz (index 形式) のみを生成する。ヘッダーの index が
        // ASC の実効周波数と一致することを、index の対応表経由で照合する
        let expected_hz = SAMPLING_FREQUENCIES[header.sampling_frequency_index as usize];
        assert_eq!(asc.sampling_frequency.hz(), expected_hz);
        assert_eq!(header.channel_configuration, asc.channel_configuration);
        assert_eq!(header.frame_length as usize, frame.len());
        assert_eq!(header.original_copy, config.original_copy);
        assert_eq!(header.home, config.home);
        assert_eq!(parsed_raw, &raw[..], "raw AAC ペイロードが往復する");
        Ok(())
    })?;
    assert!(
        mpeg2_reached.get(),
        "MPEG-2 (ID = 1) のケースが 1 件も生成されていない\n{runner}"
    );
    Ok(())
}

/// `SamplingFrequency::from_hz(hz)` の生成値は `hz()` で往復し、encode → parse でも往復する
///
/// - 標準テーブル値 (重み付き分岐) は index 形式 (2 バイト)、任意 Hz は明示形式 (5 バイト)
/// - どちらの形式も踏まれることを encode 長の到達ゲートで保証する
/// - encode → parse で形式 (index / 明示) が保持される (往復で正規形の長さも保たれる)
#[test]
fn sampling_frequency_from_hz_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let index_reached = Cell::new(false);
    let explicit_reached = Cell::new(false);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        // 標準テーブル値 (index 形式経路) と任意 Hz (明示形式経路) を重み付きで混ぜる
        let hz = match noprop::sample_weighted_index(ctx, &[1, 3]) {
            0 => SAMPLING_FREQUENCIES[noprop::sample_u64_in(ctx, 0..=12) as usize],
            _ => noprop::sample_with_boundaries(
                ctx,
                &[1u32, 0xFF_FFFF],
                noprop::Ratio::one_nth(3),
                |ctx| noprop::sample_u64_in(ctx, 1..=0xFF_FFFF) as u32,
            ),
        };
        let frequency = SamplingFrequency::from_hz(hz).expect("有効な Hz は生成成功する");
        assert_eq!(frequency.hz(), hz);

        // 生成した ASC の encode → parse 往復 (形式を保持する)
        let config = AudioSpecificConfig {
            audio_object_type: AudioObjectType::AacLc,
            sampling_frequency: frequency,
            channel_configuration: sample_channel_configuration(ctx),
        };
        let encoded =
            encode_audio_specific_config(&config).expect("有効な ASC はエンコード成功する");
        if encoded.len() == 2 {
            index_reached.set(true);
        }
        if encoded.len() == 5 {
            explicit_reached.set(true);
        }
        let parsed =
            parse_audio_specific_config(&encoded).expect("エンコードした ASC は再解析成功する");
        assert_eq!(parsed, config, "encode → parse で ASC が往復する");
        Ok(())
    })?;
    assert!(
        index_reached.get(),
        "標準周波数 (index 形式) のケースが 1 件も生成されていない\n{runner}"
    );
    assert!(
        explicit_reached.get(),
        "非標準周波数 (明示形式) のケースが 1 件も生成されていない\n{runner}"
    );
    Ok(())
}
