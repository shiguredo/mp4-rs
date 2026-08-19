//! コーデック設定ボックスの Property-Based Testing

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    boxes::{Av1cBox, AvccBox, DflaBox, DopsBox, EsdsBox, HvccBox, HvccNalUintArray, VpccBox},
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

/// このファイルの主要 PBT ケース数（旧 `with_cases(200)` を維持）
const CASES: usize = 200;

/// パニック安全性テスト用のケース数（旧 `with_cases(50)` を維持）
const CASES_PANIC: usize = 50;

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

// ===== サンプラー定義 =====

/// AvccBox (Baseline/Main/Extended profile) を生成する
fn arb_avcc_box_baseline(ctx: &mut TestCaseContext) -> AvccBox {
    let avc_profile_indication = noprop::sample_choice(ctx, &[66u8, 77u8, 88u8]);
    let profile_compatibility = noprop::sample_u8(ctx);
    let avc_level_indication = noprop::sample_u8(ctx);
    let length_size_minus_one = noprop::sample_u64_in(ctx, 0..4) as u8;
    // SPS は numOfSequenceParameterSets（unsigned int(5)）で最大 31 個、
    // PPS は numOfPictureParameterSets（unsigned int(8)）で最大 255 個まで格納できる。
    let sps_list = sample_vec(ctx, 0..32, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..50);
        noprop::sample_bytes_vec(ctx, n)
    });
    let pps_list = sample_vec(ctx, 0..256, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..50);
        noprop::sample_bytes_vec(ctx, n)
    });
    AvccBox {
        avc_profile_indication,
        profile_compatibility,
        avc_level_indication,
        length_size_minus_one: Uint::new(length_size_minus_one),
        sps_list,
        pps_list,
        chroma_format: None,
        bit_depth_luma_minus8: None,
        bit_depth_chroma_minus8: None,
        sps_ext_list: vec![],
    }
}

/// AvccBox (High profile 以上) を生成する
fn arb_avcc_box_high(ctx: &mut TestCaseContext) -> AvccBox {
    let avc_profile_indication = noprop::sample_choice(ctx, &[100u8, 110u8, 122u8, 244u8]);
    let profile_compatibility = noprop::sample_u8(ctx);
    let avc_level_indication = noprop::sample_u8(ctx);
    let length_size_minus_one = noprop::sample_u64_in(ctx, 0..4) as u8;
    let sps_list = sample_vec(ctx, 0..32, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..50);
        noprop::sample_bytes_vec(ctx, n)
    });
    let pps_list = sample_vec(ctx, 0..256, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..50);
        noprop::sample_bytes_vec(ctx, n)
    });
    let chroma_format = noprop::sample_u64_in(ctx, 0..4) as u8;
    let bit_depth_luma_minus8 = noprop::sample_u64_in(ctx, 0..8) as u8;
    let bit_depth_chroma_minus8 = noprop::sample_u64_in(ctx, 0..8) as u8;
    let sps_ext_list = sample_vec(ctx, 0..3, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..30);
        noprop::sample_bytes_vec(ctx, n)
    });
    AvccBox {
        avc_profile_indication,
        profile_compatibility,
        avc_level_indication,
        length_size_minus_one: Uint::new(length_size_minus_one),
        sps_list,
        pps_list,
        chroma_format: Some(Uint::new(chroma_format)),
        bit_depth_luma_minus8: Some(Uint::new(bit_depth_luma_minus8)),
        bit_depth_chroma_minus8: Some(Uint::new(bit_depth_chroma_minus8)),
        sps_ext_list,
    }
}

/// HvccNalUintArray を生成する
fn arb_hvcc_nalu_array(ctx: &mut TestCaseContext) -> HvccNalUintArray {
    let array_completeness = noprop::sample_bool(ctx);
    let nal_unit_type = noprop::sample_u64_in(ctx, 0..64) as u8; // 6 bits
    let nalus = sample_vec(ctx, 0..3, |ctx| {
        let n = noprop::sample_usize_in(ctx, 1..30);
        noprop::sample_bytes_vec(ctx, n)
    });
    HvccNalUintArray {
        array_completeness: Uint::new(array_completeness as u8),
        nal_unit_type: Uint::new(nal_unit_type),
        nalus,
    }
}

/// HvccBox を生成する
fn arb_hvcc_box(ctx: &mut TestCaseContext) -> HvccBox {
    let general_profile_space = noprop::sample_u64_in(ctx, 0..4) as u8;
    let general_tier_flag = noprop::sample_bool(ctx);
    let general_profile_idc = noprop::sample_u64_in(ctx, 0..32) as u8;
    let general_profile_compatibility_flags = noprop::sample_u32(ctx);
    let general_constraint_indicator_flags = noprop::sample_u64(ctx) & 0x0000_FFFF_FFFF_FFFF;
    let general_level_idc = noprop::sample_u8(ctx);
    let min_spatial_segmentation_idc = noprop::sample_u64_in(ctx, 0..4096) as u16;
    let parallelism_type = noprop::sample_u64_in(ctx, 0..4) as u8;
    let chroma_format_idc = noprop::sample_u64_in(ctx, 0..4) as u8;
    let bit_depth_luma_minus8 = noprop::sample_u64_in(ctx, 0..8) as u8;
    let bit_depth_chroma_minus8 = noprop::sample_u64_in(ctx, 0..8) as u8;
    let avg_frame_rate = noprop::sample_u16(ctx);
    let constant_frame_rate = noprop::sample_u64_in(ctx, 0..4) as u8;
    let num_temporal_layers = noprop::sample_u64_in(ctx, 0..8) as u8;
    let temporal_id_nested = noprop::sample_bool(ctx);
    let length_size_minus_one = noprop::sample_u64_in(ctx, 0..4) as u8;
    let nalu_arrays = sample_vec(ctx, 0..3, arb_hvcc_nalu_array);

    HvccBox {
        general_profile_space: Uint::new(general_profile_space),
        general_tier_flag: Uint::new(general_tier_flag as u8),
        general_profile_idc: Uint::new(general_profile_idc),
        general_profile_compatibility_flags,
        general_constraint_indicator_flags: Uint::new(general_constraint_indicator_flags),
        general_level_idc,
        min_spatial_segmentation_idc: Uint::new(min_spatial_segmentation_idc),
        parallelism_type: Uint::new(parallelism_type),
        chroma_format_idc: Uint::new(chroma_format_idc),
        bit_depth_luma_minus8: Uint::new(bit_depth_luma_minus8),
        bit_depth_chroma_minus8: Uint::new(bit_depth_chroma_minus8),
        avg_frame_rate,
        constant_frame_rate: Uint::new(constant_frame_rate),
        num_temporal_layers: Uint::new(num_temporal_layers),
        temporal_id_nested: Uint::new(temporal_id_nested as u8),
        length_size_minus_one: Uint::new(length_size_minus_one),
        nalu_arrays,
    }
}

/// VpccBox を生成する
fn arb_vpcc_box(ctx: &mut TestCaseContext) -> VpccBox {
    let profile = noprop::sample_u8(ctx);
    let level = noprop::sample_u8(ctx);
    let bit_depth = noprop::sample_u64_in(ctx, 0..16) as u8;
    let chroma_subsampling = noprop::sample_u64_in(ctx, 0..8) as u8;
    let video_full_range_flag = noprop::sample_bool(ctx);
    let colour_primaries = noprop::sample_u8(ctx);
    let transfer_characteristics = noprop::sample_u8(ctx);
    let matrix_coefficients = noprop::sample_u8(ctx);
    let init_len = noprop::sample_usize_in(ctx, 0..50);
    let codec_initialization_data = noprop::sample_bytes_vec(ctx, init_len);
    VpccBox {
        profile,
        level,
        bit_depth: Uint::new(bit_depth),
        chroma_subsampling: Uint::new(chroma_subsampling),
        video_full_range_flag: Uint::new(video_full_range_flag as u8),
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        codec_initialization_data,
    }
}

/// Av1cBox を生成する
fn arb_av1c_box(ctx: &mut TestCaseContext) -> Av1cBox {
    let seq_profile = noprop::sample_u64_in(ctx, 0..8) as u8;
    let seq_level_idx_0 = noprop::sample_u64_in(ctx, 0..32) as u8;
    let seq_tier_0 = noprop::sample_bool(ctx);
    let high_bitdepth = noprop::sample_bool(ctx);
    let twelve_bit = noprop::sample_bool(ctx);
    let monochrome = noprop::sample_bool(ctx);
    let chroma_subsampling_x = noprop::sample_bool(ctx);
    let chroma_subsampling_y = noprop::sample_bool(ctx);
    let chroma_sample_position = noprop::sample_u64_in(ctx, 0..4) as u8;
    let initial_presentation_delay_minus_one = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u64_in(ctx, 0..16) as u8)
    } else {
        None
    };
    let obu_len = noprop::sample_usize_in(ctx, 0..50);
    let config_obus = noprop::sample_bytes_vec(ctx, obu_len);
    Av1cBox {
        seq_profile: Uint::new(seq_profile),
        seq_level_idx_0: Uint::new(seq_level_idx_0),
        seq_tier_0: Uint::new(seq_tier_0 as u8),
        high_bitdepth: Uint::new(high_bitdepth as u8),
        twelve_bit: Uint::new(twelve_bit as u8),
        monochrome: Uint::new(monochrome as u8),
        chroma_subsampling_x: Uint::new(chroma_subsampling_x as u8),
        chroma_subsampling_y: Uint::new(chroma_subsampling_y as u8),
        chroma_sample_position: Uint::new(chroma_sample_position),
        initial_presentation_delay_minus_one: initial_presentation_delay_minus_one.map(Uint::new),
        config_obus,
    }
}

/// DopsBox を生成する
fn arb_dops_box(ctx: &mut TestCaseContext) -> DopsBox {
    DopsBox {
        output_channel_count: noprop::sample_u64_in(ctx, 1..=8) as u8,
        pre_skip: noprop::sample_u16(ctx),
        input_sample_rate: noprop::sample_u32(ctx),
        output_gain: noprop::sample_i16(ctx),
    }
}

/// EsdsBox を生成する
fn arb_esds_box(ctx: &mut TestCaseContext) -> EsdsBox {
    let es_id = noprop::sample_u64_in(ctx, 1..=u16::MAX as u64) as u16;
    let stream_priority = noprop::sample_u64_in(ctx, 0..32) as u8;
    let stream_type = noprop::sample_u64_in(ctx, 0..64) as u8;
    let buffer_size_db = noprop::sample_u32(ctx) & 0x00FF_FFFF;
    let max_bitrate = noprop::sample_u32(ctx);
    let avg_bitrate = noprop::sample_u32(ctx);
    let dec_specific_info = if noprop::sample_bool(ctx) {
        let n = noprop::sample_usize_in(ctx, 0..30);
        Some(noprop::sample_bytes_vec(ctx, n))
    } else {
        None
    };
    EsdsBox {
        es: EsDescriptor {
            es_id,
            stream_priority: Uint::new(stream_priority),
            depends_on_es_id: None,
            url_string: None,
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication: 0x40, // AAC
                stream_type: Uint::new(stream_type),
                up_stream: Uint::new(0),
                buffer_size_db: Uint::new(buffer_size_db),
                max_bitrate,
                avg_bitrate,
                dec_specific_info: dec_specific_info.map(|payload| DecoderSpecificInfo { payload }),
            },
            sl_config_descr: SlConfigDescriptor,
        },
    }
}

// ===== AvccBox のテスト =====

/// AvccBox (Baseline profile) の encode/decode roundtrip
#[test]
fn avcc_box_baseline_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let avcc = arb_avcc_box_baseline(ctx);
        let encoded = avcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = AvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な AvccBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.avc_profile_indication, avcc.avc_profile_indication);
        assert_eq!(decoded.profile_compatibility, avcc.profile_compatibility);
        assert_eq!(decoded.avc_level_indication, avcc.avc_level_indication);
        assert_eq!(
            decoded.length_size_minus_one.get(),
            avcc.length_size_minus_one.get()
        );
        assert_eq!(decoded.sps_list, avcc.sps_list);
        assert_eq!(decoded.pps_list, avcc.pps_list);
        assert!(decoded.chroma_format.is_none());
        Ok(())
    })?;
    Ok(())
}

/// AvccBox (High profile) の encode/decode roundtrip
#[test]
fn avcc_box_high_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let avcc = arb_avcc_box_high(ctx);
        let encoded = avcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = AvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な AvccBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.avc_profile_indication, avcc.avc_profile_indication);
        assert_eq!(
            decoded.chroma_format.map(|u| u.get()),
            avcc.chroma_format.map(|u| u.get())
        );
        assert_eq!(
            decoded.bit_depth_luma_minus8.map(|u| u.get()),
            avcc.bit_depth_luma_minus8.map(|u| u.get())
        );
        assert_eq!(
            decoded.bit_depth_chroma_minus8.map(|u| u.get()),
            avcc.bit_depth_chroma_minus8.map(|u| u.get())
        );
        assert_eq!(decoded.sps_ext_list, avcc.sps_ext_list);
        Ok(())
    })?;
    Ok(())
}

// ===== HvccBox のテスト =====

/// HvccBox の encode/decode roundtrip
#[test]
fn hvcc_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let hvcc = arb_hvcc_box(ctx);
        let encoded = hvcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = HvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な HvccBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(
            decoded.general_profile_space.get(),
            hvcc.general_profile_space.get()
        );
        assert_eq!(
            decoded.general_tier_flag.get(),
            hvcc.general_tier_flag.get()
        );
        assert_eq!(
            decoded.general_profile_idc.get(),
            hvcc.general_profile_idc.get()
        );
        assert_eq!(
            decoded.general_profile_compatibility_flags,
            hvcc.general_profile_compatibility_flags
        );
        assert_eq!(decoded.general_level_idc, hvcc.general_level_idc);
        assert_eq!(decoded.avg_frame_rate, hvcc.avg_frame_rate);
        assert_eq!(
            decoded.length_size_minus_one.get(),
            hvcc.length_size_minus_one.get()
        );
        assert_eq!(decoded.nalu_arrays.len(), hvcc.nalu_arrays.len());
        Ok(())
    })?;
    Ok(())
}

// ===== VpccBox のテスト =====

/// VpccBox の encode/decode roundtrip
#[test]
fn vpcc_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let vpcc = arb_vpcc_box(ctx);
        let encoded = vpcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = VpccBox::decode(&encoded)
            .expect("直前にエンコードした有効な VpccBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.profile, vpcc.profile);
        assert_eq!(decoded.level, vpcc.level);
        assert_eq!(decoded.bit_depth.get(), vpcc.bit_depth.get());
        assert_eq!(
            decoded.chroma_subsampling.get(),
            vpcc.chroma_subsampling.get()
        );
        assert_eq!(
            decoded.video_full_range_flag.get(),
            vpcc.video_full_range_flag.get()
        );
        assert_eq!(decoded.colour_primaries, vpcc.colour_primaries);
        assert_eq!(
            decoded.transfer_characteristics,
            vpcc.transfer_characteristics
        );
        assert_eq!(decoded.matrix_coefficients, vpcc.matrix_coefficients);
        assert_eq!(
            decoded.codec_initialization_data,
            vpcc.codec_initialization_data
        );
        Ok(())
    })?;
    Ok(())
}

// ===== Av1cBox のテスト =====

/// Av1cBox の encode/decode roundtrip
#[test]
fn av1c_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let av1c = arb_av1c_box(ctx);
        let encoded = av1c.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = Av1cBox::decode(&encoded)
            .expect("直前にエンコードした有効な Av1cBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.seq_profile.get(), av1c.seq_profile.get());
        assert_eq!(decoded.seq_level_idx_0.get(), av1c.seq_level_idx_0.get());
        assert_eq!(decoded.seq_tier_0.get(), av1c.seq_tier_0.get());
        assert_eq!(decoded.high_bitdepth.get(), av1c.high_bitdepth.get());
        assert_eq!(decoded.twelve_bit.get(), av1c.twelve_bit.get());
        assert_eq!(decoded.monochrome.get(), av1c.monochrome.get());
        assert_eq!(
            decoded.chroma_subsampling_x.get(),
            av1c.chroma_subsampling_x.get()
        );
        assert_eq!(
            decoded.chroma_subsampling_y.get(),
            av1c.chroma_subsampling_y.get()
        );
        assert_eq!(
            decoded.chroma_sample_position.get(),
            av1c.chroma_sample_position.get()
        );
        assert_eq!(
            decoded
                .initial_presentation_delay_minus_one
                .map(|u| u.get()),
            av1c.initial_presentation_delay_minus_one.map(|u| u.get())
        );
        assert_eq!(decoded.config_obus, av1c.config_obus);
        Ok(())
    })?;
    Ok(())
}

// ===== DopsBox のテスト =====

/// DopsBox の encode/decode roundtrip
#[test]
fn dops_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let dops = arb_dops_box(ctx);
        let encoded = dops.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = DopsBox::decode(&encoded)
            .expect("直前にエンコードした有効な DopsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.output_channel_count, dops.output_channel_count);
        assert_eq!(decoded.pre_skip, dops.pre_skip);
        assert_eq!(decoded.input_sample_rate, dops.input_sample_rate);
        assert_eq!(decoded.output_gain, dops.output_gain);
        Ok(())
    })?;
    Ok(())
}

// ===== EsdsBox のテスト =====

/// EsdsBox の encode/decode roundtrip
#[test]
fn esds_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let esds = arb_esds_box(ctx);
        let encoded = esds.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = EsdsBox::decode(&encoded)
            .expect("直前にエンコードした有効な EsdsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.es.es_id, esds.es.es_id);
        assert_eq!(
            decoded.es.stream_priority.get(),
            esds.es.stream_priority.get()
        );
        assert_eq!(
            decoded.es.dec_config_descr.object_type_indication,
            esds.es.dec_config_descr.object_type_indication
        );
        assert_eq!(
            decoded.es.dec_config_descr.max_bitrate,
            esds.es.dec_config_descr.max_bitrate
        );
        assert_eq!(
            decoded.es.dec_config_descr.avg_bitrate,
            esds.es.dec_config_descr.avg_bitrate
        );
        Ok(())
    })?;
    Ok(())
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// AvccBox: 空の SPS/PPS リスト
    #[test]
    fn avcc_box_empty_lists() {
        let avcc = AvccBox {
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
        };
        let encoded = avcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = AvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な AvccBox は必ずデコードできる");
        assert!(decoded.sps_list.is_empty());
        assert!(decoded.pps_list.is_empty());
    }

    /// AvccBox: PPS が 32 個でもエンコードできる
    ///
    /// `numOfPictureParameterSets` は `unsigned int(8)`（最大 255）であり、
    /// SPS の上限 31 と同じ値で拒否してはならないことを回帰として固定する。
    /// 各 PPS のバイト列を index 由来にすることで、順序入れ替わりや隣接データの
    /// 混線も検知できるようにする。
    #[test]
    fn avcc_box_pps_count_32() {
        let avcc = AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: (0..32u8).map(|i| vec![i; 10]).collect(),
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        };
        let encoded = avcc
            .encode_to_vec()
            .expect("PPS 32 個は numOfPictureParameterSets の上限内なのでエンコードできる");
        let (decoded, _) = AvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な AvccBox は必ずデコードできる");
        assert_eq!(decoded.pps_list, avcc.pps_list);
    }

    /// HvccBox: 空の NALU 配列
    #[test]
    fn hvcc_box_empty_nalu_arrays() {
        let hvcc = HvccBox {
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
            temporal_id_nested: Uint::new(1),
            length_size_minus_one: Uint::new(3),
            nalu_arrays: vec![],
        };
        let encoded = hvcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = HvccBox::decode(&encoded)
            .expect("直前にエンコードした有効な HvccBox は必ずデコードできる");
        assert!(decoded.nalu_arrays.is_empty());
    }

    /// VpccBox: 空の codec_initialization_data
    #[test]
    fn vpcc_box_empty_init_data() {
        let vpcc = VpccBox {
            profile: 0,
            level: 10,
            bit_depth: Uint::new(8),
            chroma_subsampling: Uint::new(1),
            video_full_range_flag: Uint::new(0),
            colour_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
            codec_initialization_data: vec![],
        };
        let encoded = vpcc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = VpccBox::decode(&encoded)
            .expect("直前にエンコードした有効な VpccBox は必ずデコードできる");
        assert!(decoded.codec_initialization_data.is_empty());
    }

    /// Av1cBox: initial_presentation_delay なし
    #[test]
    fn av1c_box_no_delay() {
        let av1c = Av1cBox {
            seq_profile: Uint::new(0),
            seq_level_idx_0: Uint::new(8),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(0),
            twelve_bit: Uint::new(0),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        };
        let encoded = av1c.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Av1cBox::decode(&encoded)
            .expect("直前にエンコードした有効な Av1cBox は必ずデコードできる");
        assert!(decoded.initial_presentation_delay_minus_one.is_none());
    }

    /// Av1cBox: initial_presentation_delay あり
    #[test]
    fn av1c_box_with_delay() {
        let av1c = Av1cBox {
            seq_profile: Uint::new(0),
            seq_level_idx_0: Uint::new(8),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(0),
            twelve_bit: Uint::new(0),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: Some(Uint::new(4)),
            config_obus: vec![],
        };
        let encoded = av1c.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Av1cBox::decode(&encoded)
            .expect("直前にエンコードした有効な Av1cBox は必ずデコードできる");
        assert_eq!(
            decoded
                .initial_presentation_delay_minus_one
                .map(|u| u.get()),
            Some(4)
        );
    }

    /// DopsBox: 最小構成 (mono)
    #[test]
    fn dops_box_mono() {
        let dops = DopsBox {
            output_channel_count: 1,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        };
        let encoded = dops.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DopsBox::decode(&encoded)
            .expect("直前にエンコードした有効な DopsBox は必ずデコードできる");
        assert_eq!(decoded.output_channel_count, 1);
        assert_eq!(decoded.pre_skip, 312);
        assert_eq!(decoded.input_sample_rate, 48000);
    }

    /// DopsBox: ステレオ
    #[test]
    fn dops_box_stereo() {
        let dops = DopsBox {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: -256,
        };
        let encoded = dops.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DopsBox::decode(&encoded)
            .expect("直前にエンコードした有効な DopsBox は必ずデコードできる");
        assert_eq!(decoded.output_channel_count, 2);
        assert_eq!(decoded.output_gain, -256);
    }

    /// EsdsBox: AAC-LC 設定
    #[test]
    fn esds_box_aac_lc() {
        let esds = EsdsBox {
            es: EsDescriptor {
                es_id: 1,
                stream_priority: Uint::new(0),
                depends_on_es_id: None,
                url_string: None,
                ocr_es_id: None,
                dec_config_descr: DecoderConfigDescriptor {
                    object_type_indication: 0x40, // Audio ISO/IEC 14496-3
                    stream_type: Uint::new(0x05), // AudioStream
                    up_stream: Uint::new(0),
                    buffer_size_db: Uint::new(0),
                    max_bitrate: 128000,
                    avg_bitrate: 128000,
                    dec_specific_info: Some(DecoderSpecificInfo {
                        payload: vec![0x11, 0x90], // AAC-LC, 48kHz, stereo
                    }),
                },
                sl_config_descr: SlConfigDescriptor,
            },
        };
        let encoded = esds.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = EsdsBox::decode(&encoded)
            .expect("直前にエンコードした有効な EsdsBox は必ずデコードできる");
        assert_eq!(decoded.es.dec_config_descr.object_type_indication, 0x40);
        assert_eq!(decoded.es.dec_config_descr.max_bitrate, 128000);
    }
}

// ===== AvccBox のエラーパステスト =====

mod avcc_error_tests {
    use super::*;

    /// AvccBox: 32個以上の SPS でエンコードエラー
    #[test]
    fn avcc_box_too_many_sps() {
        let avcc = AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: (0..32).map(|_| vec![0u8; 10]).collect(),
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        };
        let result = avcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// AvccBox: 256個以上の PPS でエンコードエラー (u8 超過)
    #[test]
    fn avcc_box_too_many_pps() {
        let avcc = AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: (0..256).map(|_| vec![0u8; 10]).collect(),
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        };
        let result = avcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// AvccBox: High profile で chroma_format が欠落
    #[test]
    fn avcc_box_missing_chroma_format() {
        let avcc = AvccBox {
            avc_profile_indication: 100, // High profile
            profile_compatibility: 0,
            avc_level_indication: 40,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None, // 欠落
            bit_depth_luma_minus8: Some(Uint::new(0)),
            bit_depth_chroma_minus8: Some(Uint::new(0)),
            sps_ext_list: vec![],
        };
        let result = avcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// AvccBox: High profile で bit_depth_luma_minus8 が欠落
    #[test]
    fn avcc_box_missing_bit_depth_luma() {
        let avcc = AvccBox {
            avc_profile_indication: 100, // High profile
            profile_compatibility: 0,
            avc_level_indication: 40,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: Some(Uint::new(1)),
            bit_depth_luma_minus8: None, // 欠落
            bit_depth_chroma_minus8: Some(Uint::new(0)),
            sps_ext_list: vec![],
        };
        let result = avcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// AvccBox: High profile で bit_depth_chroma_minus8 が欠落
    #[test]
    fn avcc_box_missing_bit_depth_chroma() {
        let avcc = AvccBox {
            avc_profile_indication: 100, // High profile
            profile_compatibility: 0,
            avc_level_indication: 40,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: Some(Uint::new(1)),
            bit_depth_luma_minus8: Some(Uint::new(0)),
            bit_depth_chroma_minus8: None, // 欠落
            sps_ext_list: vec![],
        };
        let result = avcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// AvccBox: 不正なバージョンでのデコードエラー
    #[test]
    fn avcc_box_invalid_version() {
        // avcC ボックスヘッダ + 不正なバージョン (2)
        let data = [
            0x00, 0x00, 0x00, 0x10, // size = 16
            b'a', b'v', b'c', b'C', // box type
            0x02, // configuration_version = 2 (不正)
            0x42, // avc_profile_indication = 66
            0x00, // profile_compatibility
            0x1E, // avc_level_indication = 30
            0xFF, // length_size_minus_one = 3
            0xE0, // sps_count = 0
            0x00, // pps_count = 0
        ];
        let result = AvccBox::decode(&data);
        assert!(result.is_err());
    }

    /// AvccBox: SPS データがペイロード境界を超過
    #[test]
    fn avcc_box_sps_exceeds_boundary() {
        let data = [
            0x00, 0x00, 0x00, 0x10, // size = 16
            b'a', b'v', b'c', b'C', // box type
            0x01, // configuration_version = 1
            0x42, // avc_profile_indication = 66
            0x00, // profile_compatibility
            0x1E, // avc_level_indication = 30
            0xFF, // length_size_minus_one = 3
            0xE1, // sps_count = 1
            0x00, 0xFF, // sps_size = 255 (境界超過)
        ];
        let result = AvccBox::decode(&data);
        assert!(result.is_err());
    }

    /// AvccBox: PPS データがペイロード境界を超過
    #[test]
    fn avcc_box_pps_exceeds_boundary() {
        let data = [
            0x00, 0x00, 0x00, 0x12, // size = 18
            b'a', b'v', b'c', b'C', // box type
            0x01, // configuration_version = 1
            0x42, // avc_profile_indication = 66
            0x00, // profile_compatibility
            0x1E, // avc_level_indication = 30
            0xFF, // length_size_minus_one = 3
            0xE0, // sps_count = 0
            0x01, // pps_count = 1
            0x00, 0xFF, // pps_size = 255 (境界超過)
        ];
        let result = AvccBox::decode(&data);
        assert!(result.is_err());
    }
}

// ===== HvccBox のエラーパステスト =====

mod hvcc_error_tests {
    use super::*;

    /// HvccBox: 256個以上の NALU arrays でエンコードエラー
    #[test]
    fn hvcc_box_too_many_nalu_arrays() {
        let hvcc = HvccBox {
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
            temporal_id_nested: Uint::new(1),
            length_size_minus_one: Uint::new(3),
            nalu_arrays: (0..256)
                .map(|_| HvccNalUintArray {
                    array_completeness: Uint::new(1),
                    nal_unit_type: Uint::new(32),
                    nalus: vec![],
                })
                .collect(),
        };
        let result = hvcc.encode_to_vec();
        assert!(result.is_err());
    }

    /// HvccBox: 不正なバージョンでのデコードエラー
    #[test]
    fn hvcc_box_invalid_version() {
        // hvcC ボックスヘッダ + 不正なバージョン (2)
        let data = [
            0x00, 0x00, 0x00, 0x1C, // size = 28
            b'h', b'v', b'c', b'C', // box type
            0x02, // configuration_version = 2 (不正)
            0x01, // general_profile_space + general_tier_flag + general_profile_idc
            0x00, 0x00, 0x00, 0x00, // general_profile_compatibility_flags
            0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, // general_constraint_indicator_flags (48 bits)
            0x5D, // general_level_idc
            0xF0, 0x00, // min_spatial_segmentation_idc
            0xFC, // parallelism_type
            0xFD, // chroma_format_idc
            0xF8, // bit_depth_luma_minus8
            0xF8, // bit_depth_chroma_minus8
            0x00, 0x00, // avg_frame_rate
            0x0F, // constant_frame_rate + num_temporal_layers + temporal_id_nested + length_size_minus_one
            0x00, // num_of_arrays
        ];
        let result = HvccBox::decode(&data);
        assert!(result.is_err());
    }

    /// HvccBox: general_constraint_indicator_flags がペイロード境界を超過
    #[test]
    fn hvcc_box_constraint_flags_exceeds_boundary() {
        let data = [
            0x00, 0x00, 0x00, 0x0E, // size = 14 (小さすぎ)
            b'h', b'v', b'c', b'C', // box type
            0x01, // configuration_version = 1
            0x01, // general_profile_space + general_tier_flag + general_profile_idc
            0x00, 0x00, 0x00,
            0x00, // general_profile_compatibility_flags
                  // general_constraint_indicator_flags の 6 バイトがない
        ];
        let result = HvccBox::decode(&data);
        assert!(result.is_err());
    }

    /// HvccBox: NAL unit データがペイロード境界を超過
    #[test]
    fn hvcc_box_nalu_exceeds_boundary() {
        let data = [
            0x00, 0x00, 0x00, 0x20, // size = 32
            b'h', b'v', b'c', b'C', // box type
            0x01, // configuration_version = 1
            0x01, // general_profile_space + general_tier_flag + general_profile_idc
            0x00, 0x00, 0x00, 0x00, // general_profile_compatibility_flags
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // general_constraint_indicator_flags
            0x5D, // general_level_idc
            0xF0, 0x00, // min_spatial_segmentation_idc
            0xFC, // parallelism_type
            0xFD, // chroma_format_idc
            0xF8, // bit_depth_luma_minus8
            0xF8, // bit_depth_chroma_minus8
            0x00, 0x00, // avg_frame_rate
            0x0F, // constant_frame_rate + etc.
            0x01, // num_of_arrays = 1
            0xA0, // array_completeness + nal_unit_type
            0x00, 0x01, // num_nalus = 1
            0x00, 0xFF, // nal_unit_length = 255 (境界超過)
        ];
        let result = HvccBox::decode(&data);
        assert!(result.is_err());
    }
}

// ===== DflaBox のエラーパステスト =====

mod dfla_error_tests {
    use super::*;

    /// DflaBox: 不正なバージョンでのデコードエラー
    #[test]
    fn dfla_box_invalid_version() {
        // dfLa ボックスヘッダ + FullBox header (version = 1)
        let data = [
            0x00, 0x00, 0x00, 0x0C, // size = 12
            b'd', b'f', b'L', b'a', // box type
            0x01, // version = 1 (不正、0 のみ許可)
            0x00, 0x00, 0x00, // flags
        ];
        let result = DflaBox::decode(&data);
        assert!(result.is_err());
    }
}

// ===== DopsBox のエラーパステスト =====

mod dops_error_tests {
    use super::*;

    /// DopsBox: 不正なバージョンでのデコードエラー
    #[test]
    fn dops_box_invalid_version() {
        // dOps ボックスヘッダ + 不正なバージョン
        let data = [
            0x00, 0x00, 0x00, 0x14, // size = 20
            b'd', b'O', b'p', b's', // box type
            0x01, // version = 1 (不正、0 のみ許可)
            0x02, // output_channel_count
            0x01, 0x38, // pre_skip
            0x00, 0x00, 0xBB, 0x80, // input_sample_rate
            0x00, 0x00, // output_gain
            0x00, // channel_mapping_family
        ];
        let result = DopsBox::decode(&data);
        assert!(result.is_err());
    }
}

// ===== EsdsBox のエラーパステスト =====

mod esds_error_tests {
    use super::*;

    /// EsdsBox: 不正なバージョンでのデコードエラー
    #[test]
    fn esds_box_invalid_version() {
        // esds ボックスヘッダ + FullBox header (version = 1)
        let data = [
            0x00, 0x00, 0x00, 0x0C, // size = 12
            b'e', b's', b'd', b's', // box type
            0x01, // version = 1 (不正、0 のみ許可)
            0x00, 0x00, 0x00, // flags
        ];
        let result = EsdsBox::decode(&data);
        assert!(result.is_err());
    }
}

// ===== ランダムバイト列でのデコードのパニック安全性テスト =====

/// ランダムなバイト列での AvccBox デコードはパニックしない
#[test]
fn avcc_box_decode_no_panic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_PANIC, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, n);
        // パニックしないことを確認 (エラーは OK)
        let _ = AvccBox::decode(&data);
        Ok(())
    })?;
    Ok(())
}

/// ランダムなバイト列での HvccBox デコードはパニックしない
#[test]
fn hvcc_box_decode_no_panic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_PANIC, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, n);
        let _ = HvccBox::decode(&data);
        Ok(())
    })?;
    Ok(())
}

/// ランダムなバイト列での DflaBox デコードはパニックしない
#[test]
fn dfla_box_decode_no_panic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_PANIC, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, n);
        let _ = DflaBox::decode(&data);
        Ok(())
    })?;
    Ok(())
}

/// ランダムなバイト列での DopsBox デコードはパニックしない
#[test]
fn dops_box_decode_no_panic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_PANIC, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, n);
        let _ = DopsBox::decode(&data);
        Ok(())
    })?;
    Ok(())
}

/// ランダムなバイト列での EsdsBox デコードはパニックしない
#[test]
fn esds_box_decode_no_panic() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES_PANIC, |ctx| {
        let n = noprop::sample_usize_in(ctx, 0..256);
        let data = noprop::sample_bytes_vec(ctx, n);
        let _ = EsdsBox::decode(&data);
        Ok(())
    })?;
    Ok(())
}
