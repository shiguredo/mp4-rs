//! コーデック設定ボックスの Property-Based Testing

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    boxes::{Av1cBox, AvccBox, DopsBox, EsdsBox, HvccBox, HvccNalUintArray, VpccBox},
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

// ===== Strategy 定義 =====

/// AvccBox (Baseline/Main/Extended profile) を生成する Strategy
fn arb_avcc_box_baseline() -> impl Strategy<Value = AvccBox> {
    (
        prop_oneof![Just(66u8), Just(77u8), Just(88u8)], // Baseline, Main, Extended
        any::<u8>(),                                     // profile_compatibility
        any::<u8>(),                                     // avc_level_indication
        0u8..4,                                          // length_size_minus_one (2 bits)
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..50), 0..5), // sps_list
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..50), 0..5), // pps_list
    )
        .prop_map(
            |(
                avc_profile_indication,
                profile_compatibility,
                avc_level_indication,
                length_size_minus_one,
                sps_list,
                pps_list,
            )| {
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
            },
        )
}

/// AvccBox (High profile 以上) を生成する Strategy
fn arb_avcc_box_high() -> impl Strategy<Value = AvccBox> {
    (
        prop_oneof![Just(100u8), Just(110u8), Just(122u8), Just(244u8)], // High profiles
        any::<u8>(),
        any::<u8>(),
        0u8..4,
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..50), 0..5),
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..50), 0..5),
        0u8..4, // chroma_format (2 bits)
        0u8..8, // bit_depth_luma_minus8 (3 bits)
        0u8..8, // bit_depth_chroma_minus8 (3 bits)
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..30), 0..3), // sps_ext_list
    )
        .prop_map(
            |(
                avc_profile_indication,
                profile_compatibility,
                avc_level_indication,
                length_size_minus_one,
                sps_list,
                pps_list,
                chroma_format,
                bit_depth_luma_minus8,
                bit_depth_chroma_minus8,
                sps_ext_list,
            )| {
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
            },
        )
}

/// HvccNalUintArray を生成する Strategy
fn arb_hvcc_nalu_array() -> impl Strategy<Value = HvccNalUintArray> {
    (
        any::<bool>(), // array_completeness
        0u8..64,       // nal_unit_type (6 bits)
        prop::collection::vec(prop::collection::vec(any::<u8>(), 1..30), 0..3), // nalus
    )
        .prop_map(
            |(array_completeness, nal_unit_type, nalus)| HvccNalUintArray {
                array_completeness: Uint::new(array_completeness as u8),
                nal_unit_type: Uint::new(nal_unit_type),
                nalus,
            },
        )
}

/// HvccBox を生成する Strategy
fn arb_hvcc_box() -> impl Strategy<Value = HvccBox> {
    // proptest のタプルは 12 要素まで。2 つに分割する
    let part1 = (
        0u8..4,                                               // general_profile_space (2 bits)
        any::<bool>(),                                        // general_tier_flag
        0u8..32,                                              // general_profile_idc (5 bits)
        any::<u32>(),                                         // general_profile_compatibility_flags
        any::<u64>().prop_map(|v| v & 0x0000_FFFF_FFFF_FFFF), // 48 bits
        any::<u8>(),                                          // general_level_idc
        0u16..4096, // min_spatial_segmentation_idc (12 bits)
        0u8..4,     // parallelism_type (2 bits)
    );
    let part2 = (
        0u8..4,        // chroma_format_idc (2 bits)
        0u8..8,        // bit_depth_luma_minus8 (3 bits)
        0u8..8,        // bit_depth_chroma_minus8 (3 bits)
        any::<u16>(),  // avg_frame_rate
        0u8..4,        // constant_frame_rate (2 bits)
        0u8..8,        // num_temporal_layers (3 bits)
        any::<bool>(), // temporal_id_nested
        0u8..4,        // length_size_minus_one (2 bits)
        prop::collection::vec(arb_hvcc_nalu_array(), 0..3),
    );

    (part1, part2).prop_map(
        |(
            (
                general_profile_space,
                general_tier_flag,
                general_profile_idc,
                general_profile_compatibility_flags,
                general_constraint_indicator_flags,
                general_level_idc,
                min_spatial_segmentation_idc,
                parallelism_type,
            ),
            (
                chroma_format_idc,
                bit_depth_luma_minus8,
                bit_depth_chroma_minus8,
                avg_frame_rate,
                constant_frame_rate,
                num_temporal_layers,
                temporal_id_nested,
                length_size_minus_one,
                nalu_arrays,
            ),
        )| {
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
        },
    )
}

/// VpccBox を生成する Strategy
fn arb_vpcc_box() -> impl Strategy<Value = VpccBox> {
    (
        any::<u8>(),                               // profile
        any::<u8>(),                               // level
        0u8..16,                                   // bit_depth (4 bits)
        0u8..8,                                    // chroma_subsampling (3 bits)
        any::<bool>(),                             // video_full_range_flag
        any::<u8>(),                               // colour_primaries
        any::<u8>(),                               // transfer_characteristics
        any::<u8>(),                               // matrix_coefficients
        prop::collection::vec(any::<u8>(), 0..50), // codec_initialization_data
    )
        .prop_map(
            |(
                profile,
                level,
                bit_depth,
                chroma_subsampling,
                video_full_range_flag,
                colour_primaries,
                transfer_characteristics,
                matrix_coefficients,
                codec_initialization_data,
            )| {
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
            },
        )
}

/// Av1cBox を生成する Strategy
fn arb_av1c_box() -> impl Strategy<Value = Av1cBox> {
    (
        0u8..8,                                    // seq_profile (3 bits)
        0u8..32,                                   // seq_level_idx_0 (5 bits)
        any::<bool>(),                             // seq_tier_0
        any::<bool>(),                             // high_bitdepth
        any::<bool>(),                             // twelve_bit
        any::<bool>(),                             // monochrome
        any::<bool>(),                             // chroma_subsampling_x
        any::<bool>(),                             // chroma_subsampling_y
        0u8..4,                                    // chroma_sample_position (2 bits)
        prop::option::of(0u8..16),                 // initial_presentation_delay_minus_one (4 bits)
        prop::collection::vec(any::<u8>(), 0..50), // config_obus
    )
        .prop_map(
            |(
                seq_profile,
                seq_level_idx_0,
                seq_tier_0,
                high_bitdepth,
                twelve_bit,
                monochrome,
                chroma_subsampling_x,
                chroma_subsampling_y,
                chroma_sample_position,
                initial_presentation_delay_minus_one,
                config_obus,
            )| {
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
                    initial_presentation_delay_minus_one: initial_presentation_delay_minus_one
                        .map(Uint::new),
                    config_obus,
                }
            },
        )
}

/// DopsBox を生成する Strategy
fn arb_dops_box() -> impl Strategy<Value = DopsBox> {
    (
        1u8..=8,      // output_channel_count (1-8)
        any::<u16>(), // pre_skip
        any::<u32>(), // input_sample_rate
        any::<i16>(), // output_gain
    )
        .prop_map(
            |(output_channel_count, pre_skip, input_sample_rate, output_gain)| DopsBox {
                output_channel_count,
                pre_skip,
                input_sample_rate,
                output_gain,
            },
        )
}

/// EsdsBox を生成する Strategy
fn arb_esds_box() -> impl Strategy<Value = EsdsBox> {
    (
        1u16..=u16::MAX,                                             // es_id
        0u8..32,                                                     // stream_priority (5 bits)
        0u8..64,                                                     // stream_type (6 bits)
        any::<u32>().prop_map(|v| v & 0x00FF_FFFF),                  // buffer_size_db (24 bits)
        any::<u32>(),                                                // max_bitrate
        any::<u32>(),                                                // avg_bitrate
        prop::option::of(prop::collection::vec(any::<u8>(), 0..30)), // dec_specific_info
    )
        .prop_map(
            |(
                es_id,
                stream_priority,
                stream_type,
                buffer_size_db,
                max_bitrate,
                avg_bitrate,
                dec_specific_info,
            )| {
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
                            dec_specific_info: dec_specific_info
                                .map(|payload| DecoderSpecificInfo { payload }),
                        },
                        sl_config_descr: SlConfigDescriptor,
                    },
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // ===== AvccBox のテスト =====

    /// AvccBox (Baseline profile) の encode/decode roundtrip
    #[test]
    fn avcc_box_baseline_roundtrip(avcc in arb_avcc_box_baseline()) {
        let encoded = avcc.encode_to_vec().unwrap();
        let (decoded, size) = AvccBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.avc_profile_indication, avcc.avc_profile_indication);
        prop_assert_eq!(decoded.profile_compatibility, avcc.profile_compatibility);
        prop_assert_eq!(decoded.avc_level_indication, avcc.avc_level_indication);
        prop_assert_eq!(decoded.length_size_minus_one.get(), avcc.length_size_minus_one.get());
        prop_assert_eq!(decoded.sps_list, avcc.sps_list);
        prop_assert_eq!(decoded.pps_list, avcc.pps_list);
        prop_assert!(decoded.chroma_format.is_none());
    }

    /// AvccBox (High profile) の encode/decode roundtrip
    #[test]
    fn avcc_box_high_roundtrip(avcc in arb_avcc_box_high()) {
        let encoded = avcc.encode_to_vec().unwrap();
        let (decoded, size) = AvccBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.avc_profile_indication, avcc.avc_profile_indication);
        prop_assert_eq!(decoded.chroma_format.map(|u| u.get()), avcc.chroma_format.map(|u| u.get()));
        prop_assert_eq!(decoded.bit_depth_luma_minus8.map(|u| u.get()), avcc.bit_depth_luma_minus8.map(|u| u.get()));
        prop_assert_eq!(decoded.bit_depth_chroma_minus8.map(|u| u.get()), avcc.bit_depth_chroma_minus8.map(|u| u.get()));
        prop_assert_eq!(decoded.sps_ext_list, avcc.sps_ext_list);
    }

    // ===== HvccBox のテスト =====

    /// HvccBox の encode/decode roundtrip
    #[test]
    fn hvcc_box_roundtrip(hvcc in arb_hvcc_box()) {
        let encoded = hvcc.encode_to_vec().unwrap();
        let (decoded, size) = HvccBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.general_profile_space.get(), hvcc.general_profile_space.get());
        prop_assert_eq!(decoded.general_tier_flag.get(), hvcc.general_tier_flag.get());
        prop_assert_eq!(decoded.general_profile_idc.get(), hvcc.general_profile_idc.get());
        prop_assert_eq!(decoded.general_profile_compatibility_flags, hvcc.general_profile_compatibility_flags);
        prop_assert_eq!(decoded.general_level_idc, hvcc.general_level_idc);
        prop_assert_eq!(decoded.avg_frame_rate, hvcc.avg_frame_rate);
        prop_assert_eq!(decoded.length_size_minus_one.get(), hvcc.length_size_minus_one.get());
        prop_assert_eq!(decoded.nalu_arrays.len(), hvcc.nalu_arrays.len());
    }

    // ===== VpccBox のテスト =====

    /// VpccBox の encode/decode roundtrip
    #[test]
    fn vpcc_box_roundtrip(vpcc in arb_vpcc_box()) {
        let encoded = vpcc.encode_to_vec().unwrap();
        let (decoded, size) = VpccBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.profile, vpcc.profile);
        prop_assert_eq!(decoded.level, vpcc.level);
        prop_assert_eq!(decoded.bit_depth.get(), vpcc.bit_depth.get());
        prop_assert_eq!(decoded.chroma_subsampling.get(), vpcc.chroma_subsampling.get());
        prop_assert_eq!(decoded.video_full_range_flag.get(), vpcc.video_full_range_flag.get());
        prop_assert_eq!(decoded.colour_primaries, vpcc.colour_primaries);
        prop_assert_eq!(decoded.transfer_characteristics, vpcc.transfer_characteristics);
        prop_assert_eq!(decoded.matrix_coefficients, vpcc.matrix_coefficients);
        prop_assert_eq!(decoded.codec_initialization_data, vpcc.codec_initialization_data);
    }

    // ===== Av1cBox のテスト =====

    /// Av1cBox の encode/decode roundtrip
    #[test]
    fn av1c_box_roundtrip(av1c in arb_av1c_box()) {
        let encoded = av1c.encode_to_vec().unwrap();
        let (decoded, size) = Av1cBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.seq_profile.get(), av1c.seq_profile.get());
        prop_assert_eq!(decoded.seq_level_idx_0.get(), av1c.seq_level_idx_0.get());
        prop_assert_eq!(decoded.seq_tier_0.get(), av1c.seq_tier_0.get());
        prop_assert_eq!(decoded.high_bitdepth.get(), av1c.high_bitdepth.get());
        prop_assert_eq!(decoded.twelve_bit.get(), av1c.twelve_bit.get());
        prop_assert_eq!(decoded.monochrome.get(), av1c.monochrome.get());
        prop_assert_eq!(decoded.chroma_subsampling_x.get(), av1c.chroma_subsampling_x.get());
        prop_assert_eq!(decoded.chroma_subsampling_y.get(), av1c.chroma_subsampling_y.get());
        prop_assert_eq!(decoded.chroma_sample_position.get(), av1c.chroma_sample_position.get());
        prop_assert_eq!(
            decoded.initial_presentation_delay_minus_one.map(|u| u.get()),
            av1c.initial_presentation_delay_minus_one.map(|u| u.get())
        );
        prop_assert_eq!(decoded.config_obus, av1c.config_obus);
    }

    // ===== DopsBox のテスト =====

    /// DopsBox の encode/decode roundtrip
    #[test]
    fn dops_box_roundtrip(dops in arb_dops_box()) {
        let encoded = dops.encode_to_vec().unwrap();
        let (decoded, size) = DopsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.output_channel_count, dops.output_channel_count);
        prop_assert_eq!(decoded.pre_skip, dops.pre_skip);
        prop_assert_eq!(decoded.input_sample_rate, dops.input_sample_rate);
        prop_assert_eq!(decoded.output_gain, dops.output_gain);
    }

    // ===== EsdsBox のテスト =====

    /// EsdsBox の encode/decode roundtrip
    #[test]
    fn esds_box_roundtrip(esds in arb_esds_box()) {
        let encoded = esds.encode_to_vec().unwrap();
        let (decoded, size) = EsdsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.es.es_id, esds.es.es_id);
        prop_assert_eq!(decoded.es.stream_priority.get(), esds.es.stream_priority.get());
        prop_assert_eq!(decoded.es.dec_config_descr.object_type_indication, esds.es.dec_config_descr.object_type_indication);
        prop_assert_eq!(decoded.es.dec_config_descr.max_bitrate, esds.es.dec_config_descr.max_bitrate);
        prop_assert_eq!(decoded.es.dec_config_descr.avg_bitrate, esds.es.dec_config_descr.avg_bitrate);
    }
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
        let encoded = avcc.encode_to_vec().unwrap();
        let (decoded, _) = AvccBox::decode(&encoded).unwrap();
        assert!(decoded.sps_list.is_empty());
        assert!(decoded.pps_list.is_empty());
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
        let encoded = hvcc.encode_to_vec().unwrap();
        let (decoded, _) = HvccBox::decode(&encoded).unwrap();
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
        let encoded = vpcc.encode_to_vec().unwrap();
        let (decoded, _) = VpccBox::decode(&encoded).unwrap();
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
        let encoded = av1c.encode_to_vec().unwrap();
        let (decoded, _) = Av1cBox::decode(&encoded).unwrap();
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
        let encoded = av1c.encode_to_vec().unwrap();
        let (decoded, _) = Av1cBox::decode(&encoded).unwrap();
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
        let encoded = dops.encode_to_vec().unwrap();
        let (decoded, _) = DopsBox::decode(&encoded).unwrap();
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
        let encoded = dops.encode_to_vec().unwrap();
        let (decoded, _) = DopsBox::decode(&encoded).unwrap();
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
        let encoded = esds.encode_to_vec().unwrap();
        let (decoded, _) = EsdsBox::decode(&encoded).unwrap();
        assert_eq!(decoded.es.dec_config_descr.object_type_indication, 0x40);
        assert_eq!(decoded.es.dec_config_descr.max_bitrate, 128000);
    }
}
