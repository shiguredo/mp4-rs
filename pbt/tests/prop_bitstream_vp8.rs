//! `shiguredo_mp4::bitstream::vp8` の Property-Based Testing
//!
//! 手動構築の VP8 フレームバイト列を生成し、`parse_frame_header` が
//! frame tag / uncompressed data chunk のビット配置を正しく復元することを検証する。

use shiguredo_mp4::{
    Decode, Encode,
    bitstream::vp8::{Vp8FrameType, Vp8SampleEntryConfig, build_vp08_box, parse_frame_header},
    boxes::Vp08Box,
};

/// このファイルの PBT ケース数
const CASES: usize = 500;

/// VP8 キーフレーム開始コード (`parse_frame_header` が要求する固定値)
const KEY_FRAME_START_CODE: [u8; 3] = [0x9D, 0x01, 0x2A];

/// `first_partition_size` の最大値 (19 ビット)
const FIRST_PARTITION_SIZE_MAX: u32 = (1 << 19) - 1;

/// キーフレームのバイト列を組み立てる (payload なし、7 バイトの tail のみ)
fn build_keyframe_bytes(
    version: u8,
    show_frame: bool,
    first_partition_size: u32,
    width: u16,
    horizontal_scale: u8,
    height: u16,
    vertical_scale: u8,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let tag = ((first_partition_size & FIRST_PARTITION_SIZE_MAX) << 5)
        | ((u32::from(show_frame) & 0x1) << 4)
        | ((u32::from(version) & 0x7) << 1);
    bytes.push((tag & 0xFF) as u8);
    bytes.push(((tag >> 8) & 0xFF) as u8);
    bytes.push(((tag >> 16) & 0xFF) as u8);
    bytes.extend_from_slice(&KEY_FRAME_START_CODE);
    let width_field = (width & 0x3FFF) | ((u16::from(horizontal_scale) & 0x3) << 14);
    bytes.extend_from_slice(&width_field.to_le_bytes());
    let height_field = (height & 0x3FFF) | ((u16::from(vertical_scale) & 0x3) << 14);
    bytes.extend_from_slice(&height_field.to_le_bytes());
    bytes
}

/// interframe のバイト列を組み立てる (payload なし)
fn build_interframe_bytes(version: u8, show_frame: bool, first_partition_size: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let tag = ((first_partition_size & FIRST_PARTITION_SIZE_MAX) << 5)
        | ((u32::from(show_frame) & 0x1) << 4)
        | ((u32::from(version) & 0x7) << 1)
        | 0x1; // frame_type = 1 (Inter)
    bytes.push((tag & 0xFF) as u8);
    bytes.push(((tag >> 8) & 0xFF) as u8);
    bytes.push(((tag >> 16) & 0xFF) as u8);
    bytes
}

/// キーフレームの全 4 フィールド (version, show_frame, width, height) が
/// frame tag / uncompressed data chunk のビット配置どおりに復元されることを検証する
///
/// - `version` は 0..=3 の 4 値 (値域が小さいので一様サンプルで全値到達)
/// - `first_partition_size` は 19 ビット全域だが、payload を付けないので 0 に固定する
///   (`first_partition_size = 0` は残入力 0 と一致するので受理される)
/// - width / height は 1..=0x3FFF (14 ビット境界を境界指定でヒット率を担保)
/// - scale は 0..=3 の 4 値
#[test]
fn keyframe_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let version = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let show_frame = noprop::sample_bool(ctx);
        let width = noprop::sample_with_boundaries(
            ctx,
            &[1u16, 0x3FFF],
            noprop::Ratio::one_nth(4),
            |ctx| noprop::sample_u64_in(ctx, 1..=0x3FFF) as u16,
        );
        let height = noprop::sample_with_boundaries(
            ctx,
            &[1u16, 0x3FFF],
            noprop::Ratio::one_nth(4),
            |ctx| noprop::sample_u64_in(ctx, 1..=0x3FFF) as u16,
        );
        let horizontal_scale = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let vertical_scale = noprop::sample_u64_in(ctx, 0..=3) as u8;

        let bytes = build_keyframe_bytes(
            version,
            show_frame,
            0,
            width,
            horizontal_scale,
            height,
            vertical_scale,
        );
        let header = parse_frame_header(&bytes).expect("有効な入力は解析成功する");

        assert_eq!(header.frame_type, Vp8FrameType::Key);
        assert_eq!(header.version, version);
        assert_eq!(header.show_frame, show_frame);
        assert_eq!(header.first_partition_size, 0);
        let key = header.keyframe.expect("キーフレームは keyframe を持つ");
        assert_eq!(key.width, width);
        assert_eq!(key.height, height);
        assert_eq!(key.horizontal_scale, horizontal_scale);
        assert_eq!(key.vertical_scale, vertical_scale);
        Ok(())
    })?;
    Ok(())
}

/// interframe の frame tag 3 フィールド (version, show_frame, first_partition_size)
/// が復元されること
///
/// `first_partition_size` は payload バイトも生成して境界を張り、
/// 「残入力 == first_partition_size」を含む値域を探索する
#[test]
fn interframe_bit_layout_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let version = noprop::sample_u64_in(ctx, 0..=3) as u8;
        let show_frame = noprop::sample_bool(ctx);
        // 19 ビット全域を使うと payload 生成が高コストなので、内側は現実的な範囲に絞り、
        // 仕様上限 `FIRST_PARTITION_SIZE_MAX` を境界指定で明示的にヒットさせる
        // (MAX を踏むケースでは 524287 バイトの payload を確保する)
        let first_partition_size = noprop::sample_with_boundaries(
            ctx,
            &[0u32, 1, 128, FIRST_PARTITION_SIZE_MAX],
            noprop::Ratio::new(3, 4),
            |ctx| noprop::sample_u64_in(ctx, 0..=1024) as u32,
        );

        let mut bytes = build_interframe_bytes(version, show_frame, first_partition_size);
        // 残入力 = first_partition_size ちょうどになるよう payload を付ける
        bytes.resize(bytes.len() + first_partition_size as usize, 0);

        let header = parse_frame_header(&bytes).expect("有効な interframe は解析成功する");
        assert_eq!(header.frame_type, Vp8FrameType::Inter);
        assert_eq!(header.version, version);
        assert_eq!(header.show_frame, show_frame);
        assert_eq!(header.first_partition_size, first_partition_size);
        assert!(header.keyframe.is_none());
        Ok(())
    })?;
    Ok(())
}

/// `first_partition_size` が残入力ちょうどの境界を受理し、+ 1 を拒否する
///
/// interframe / キーフレームの両方で境界動作を確認する。
/// noprop skill の「境界値ヒット率を保つ」指針に沿って `sample_with_boundaries`
/// で `payload_len` の 0 / 1 / 128 を確実に踏む。
/// 128 は payload 生成コストを抑えるための任意サンプル値であり、真の 19 ビット MAX
/// (`FIRST_PARTITION_SIZE_MAX`) の残入力境界は tests/ 側の
/// `first_partition_size_max_value_within_bounds` で単体検証している
#[test]
fn first_partition_size_boundary() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let payload_len =
            noprop::sample_with_boundaries(ctx, &[0u32, 1, 128], noprop::Ratio::new(3, 4), |ctx| {
                noprop::sample_u64_in(ctx, 0..=256) as u32
            });
        let is_keyframe = noprop::sample_bool(ctx);

        // 境界ちょうど: first_partition_size = payload_len なら受理
        let mut bytes_exact = if is_keyframe {
            build_keyframe_bytes(0, true, payload_len, 320, 0, 240, 0)
        } else {
            build_interframe_bytes(0, true, payload_len)
        };
        bytes_exact.resize(bytes_exact.len() + payload_len as usize, 0);
        parse_frame_header(&bytes_exact).expect("境界ちょうどは受理される");

        // 境界超過: first_partition_size = payload_len + 1 なら拒否
        let mut bytes_over = if is_keyframe {
            build_keyframe_bytes(0, true, payload_len + 1, 320, 0, 240, 0)
        } else {
            build_interframe_bytes(0, true, payload_len + 1)
        };
        bytes_over.resize(bytes_over.len() + payload_len as usize, 0);
        parse_frame_header(&bytes_over).expect_err("境界 + 1 は拒否される");
        Ok(())
    })?;
    Ok(())
}

/// `build_vp08_box` が config の各フィールドを反映し、encode/decode でラウンドトリップする
#[test]
fn build_vp08_box_config_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let config = Vp8SampleEntryConfig {
            video_full_range_flag: noprop::sample_bool(ctx),
            colour_primaries: noprop::sample_u8(ctx),
            transfer_characteristics: noprop::sample_u8(ctx),
            matrix_coefficients: noprop::sample_u8(ctx),
            width: noprop::sample_u64_in(ctx, 1..=1920) as u16,
            height: noprop::sample_u64_in(ctx, 1..=1080) as u16,
        };
        let vp08 = build_vp08_box(&config);

        // config フィールドが反映されている
        assert_eq!(
            vp08.vpcc_box.video_full_range_flag.get(),
            u8::from(config.video_full_range_flag),
        );
        assert_eq!(vp08.vpcc_box.colour_primaries, config.colour_primaries);
        assert_eq!(
            vp08.vpcc_box.transfer_characteristics,
            config.transfer_characteristics,
        );
        assert_eq!(
            vp08.vpcc_box.matrix_coefficients,
            config.matrix_coefficients
        );
        assert_eq!(vp08.visual.width, config.width);
        assert_eq!(vp08.visual.height, config.height);

        // VP8 の仕様固定値
        assert_eq!(vp08.vpcc_box.profile, 0);
        assert_eq!(vp08.vpcc_box.bit_depth.get(), 8);
        assert_eq!(vp08.vpcc_box.chroma_subsampling.get(), 1);
        assert!(vp08.vpcc_box.codec_initialization_data.is_empty());
        assert!(vp08.unknown_boxes.is_empty());

        // encode → decode でラウンドトリップする
        let encoded = vp08.encode_to_vec().expect("encode 成功");
        let (decoded, size) = Vp08Box::decode(&encoded).expect("decode 成功");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, vp08);
        Ok(())
    })?;
    Ok(())
}
