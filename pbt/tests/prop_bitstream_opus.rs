//! `shiguredo_mp4::bitstream::opus` の Property-Based Testing
//!
//! 受理条件を満たす `OpusSampleEntryConfig` を noprop サンプラーでランダム生成し、
//! `build_opus_box` の構築結果が固定値と設定値の写り込みを保ち、encode → decode で
//! 意味が往復することを検証する。

use std::cell::Cell;

use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber,
    bitstream::opus::{ChannelCount, OpusSampleEntryConfig, build_opus_box},
    boxes::{AudioSampleEntryFields, OpusBox},
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// 受理条件を満たす [`OpusSampleEntryConfig`] を生成する
///
/// - `channel_count` は mono / stereo (1 / 2) に閉じる
/// - `pre_skip` / `input_sample_rate` / `output_gain` は全範囲を境界化する
fn sample_opus_config(ctx: &mut noprop::TestCaseContext) -> OpusSampleEntryConfig {
    let channel_count = if noprop::sample_bool(ctx) {
        ChannelCount::Mono
    } else {
        ChannelCount::Stereo
    };
    OpusSampleEntryConfig {
        channel_count,
        pre_skip: noprop::sample_with_boundaries(
            ctx,
            &[0u16, u16::MAX],
            noprop::Ratio::one_nth(3),
            |ctx| noprop::sample_u64_in(ctx, 0..=u64::from(u16::MAX)) as u16,
        ),
        input_sample_rate: noprop::sample_u32(ctx),
        output_gain: noprop::sample_i16(ctx),
    }
}

/// 任意の有効な設定から構築した `OpusBox` が固定値と設定値の写り込みを保つ
///
/// - 固定値 (data reference index / samplesize / samplerate = 48000 / 空
///   `unknown_boxes`) が保持される
/// - 設定値 (channelcount / `dOps` 4 フィールド) が失われずに写る
/// - 生成値は valid-by-construction なので `build_opus_box` が拒否しないこと
///
/// mono / stereo の両方が踏まれることは到達ゲートで保証する
#[test]
fn build_opus_box_preserves_fixed_and_config_values() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mono_reached = Cell::new(0usize);
    let stereo_reached = Cell::new(0usize);
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let config = sample_opus_config(ctx);
        if config.channel_count == ChannelCount::Mono {
            mono_reached.set(mono_reached.get() + 1);
        } else {
            stereo_reached.set(stereo_reached.get() + 1);
        }

        let opus = build_opus_box(&config);

        // 固定値
        assert_eq!(
            opus.audio.data_reference_index,
            AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX
        );
        assert_eq!(
            opus.audio.samplesize,
            AudioSampleEntryFields::DEFAULT_SAMPLESIZE
        );
        assert_eq!(opus.audio.samplerate, FixedPointNumber::new(48000u16, 0));
        assert!(
            opus.unknown_boxes.is_empty(),
            "unknown_boxes は空であるべき"
        );

        // 設定値の写り込み
        assert_eq!(
            opus.audio.channelcount,
            u16::from(config.channel_count.as_u8())
        );
        assert_eq!(
            opus.dops_box.output_channel_count,
            config.channel_count.as_u8()
        );
        assert_eq!(opus.dops_box.pre_skip, config.pre_skip);
        assert_eq!(opus.dops_box.input_sample_rate, config.input_sample_rate);
        assert_eq!(opus.dops_box.output_gain, config.output_gain);
        Ok(())
    })?;
    assert!(mono_reached.get() > 0, "mono を一度も見ていない\n{runner}");
    assert!(
        stereo_reached.get() > 0,
        "stereo を一度も見ていない\n{runner}"
    );
    Ok(())
}

/// 任意の有効な設定から構築した `OpusBox` が encode → decode で意味を往復する
///
/// Audio Sample Entry の固定値と `dOps` 4 フィールド、空 `unknown_boxes` が
/// ボックス全体として decode で一致する
#[test]
fn build_opus_box_encode_decode_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let config = sample_opus_config(ctx);
        let opus = build_opus_box(&config);

        let encoded = opus.encode_to_vec().expect("encode 成功");
        let (decoded, size) = OpusBox::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, opus, "encode → decode で OpusBox が往復する");
        Ok(())
    })?;
    Ok(())
}
