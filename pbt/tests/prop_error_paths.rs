//! エラーパスの Property-Based Testing
//!
//! 各種 Box のエンコード/デコード時のエラーパスをテストする

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    boxes::{AvccBox, DflaBox, DopsBox, EsdsBox, HvccBox, HvccNalUintArray},
};

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

// ===== Property-based エラーパステスト =====

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// ランダムなバイト列での AvccBox デコードはパニックしない
    #[test]
    fn avcc_box_decode_no_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
        // パニックしないことを確認 (エラーは OK)
        let _ = AvccBox::decode(&data);
    }

    /// ランダムなバイト列での HvccBox デコードはパニックしない
    #[test]
    fn hvcc_box_decode_no_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = HvccBox::decode(&data);
    }

    /// ランダムなバイト列での DflaBox デコードはパニックしない
    #[test]
    fn dfla_box_decode_no_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = DflaBox::decode(&data);
    }

    /// ランダムなバイト列での DopsBox デコードはパニックしない
    #[test]
    fn dops_box_decode_no_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = DopsBox::decode(&data);
    }

    /// ランダムなバイト列での EsdsBox デコードはパニックしない
    #[test]
    fn esds_box_decode_no_panic(data in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = EsdsBox::decode(&data);
    }
}

// ===== SampleEntry のメソッド網羅テスト =====

mod sample_entry_inner_box_tests {
    use std::num::NonZeroU16;

    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, FixedPointNumber, Uint,
        boxes::{
            AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, DflaBox, DopsBox, EsdsBox,
            FlacBox, FlacMetadataBlock, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox, SampleEntry,
            UnknownBox, VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox,
        },
        descriptors::{DecoderConfigDescriptor, EsDescriptor, SlConfigDescriptor},
    };

    fn create_audio_fields() -> AudioSampleEntryFields {
        AudioSampleEntryFields {
            data_reference_index: NonZeroU16::new(1).unwrap(),
            channelcount: 2,
            samplesize: 16,
            samplerate: FixedPointNumber::new(48000, 0),
        }
    }

    fn create_visual_fields() -> VisualSampleEntryFields {
        VisualSampleEntryFields {
            data_reference_index: NonZeroU16::new(1).unwrap(),
            width: 1920,
            height: 1080,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        }
    }

    fn create_avcc_box() -> AvccBox {
        AvccBox {
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
        }
    }

    fn create_hvcc_box() -> HvccBox {
        HvccBox {
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
            temporal_id_nested: Uint::new(0),
            length_size_minus_one: Uint::new(3),
            nalu_arrays: vec![],
        }
    }

    fn create_vpcc_box() -> VpccBox {
        VpccBox {
            profile: 0,
            level: 10,
            bit_depth: Uint::new(8),
            chroma_subsampling: Uint::new(1),
            video_full_range_flag: Uint::new(0),
            colour_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
            codec_initialization_data: vec![],
        }
    }

    fn create_av1c_box() -> Av1cBox {
        Av1cBox {
            seq_profile: Uint::new(0),
            seq_level_idx_0: Uint::new(0),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(0),
            twelve_bit: Uint::new(0),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        }
    }

    /// SampleEntry::Avc1 の inner_box() テスト
    #[test]
    fn sample_entry_avc1_inner_box() {
        let entry = SampleEntry::Avc1(Avc1Box {
            visual: create_visual_fields(),
            avcc_box: create_avcc_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Avc1Box::TYPE);
        assert!(!entry.is_unknown_box());
        assert!(entry.children().count() >= 1);
    }

    /// SampleEntry::Hev1 の inner_box() テスト
    #[test]
    fn sample_entry_hev1_inner_box() {
        let entry = SampleEntry::Hev1(Hev1Box {
            visual: create_visual_fields(),
            hvcc_box: create_hvcc_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Hev1Box::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Hvc1 の inner_box() テスト
    #[test]
    fn sample_entry_hvc1_inner_box() {
        let entry = SampleEntry::Hvc1(Hvc1Box {
            visual: create_visual_fields(),
            hvcc_box: create_hvcc_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Hvc1Box::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Vp08 の inner_box() テスト
    #[test]
    fn sample_entry_vp08_inner_box() {
        let entry = SampleEntry::Vp08(Vp08Box {
            visual: create_visual_fields(),
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Vp08Box::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Vp09 の inner_box() テスト
    #[test]
    fn sample_entry_vp09_inner_box() {
        let entry = SampleEntry::Vp09(Vp09Box {
            visual: create_visual_fields(),
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Vp09Box::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Av01 の inner_box() テスト
    #[test]
    fn sample_entry_av01_inner_box() {
        let entry = SampleEntry::Av01(Av01Box {
            visual: create_visual_fields(),
            av1c_box: create_av1c_box(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Av01Box::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Opus の inner_box() テスト
    #[test]
    fn sample_entry_opus_inner_box() {
        let entry = SampleEntry::Opus(OpusBox {
            audio: create_audio_fields(),
            dops_box: DopsBox {
                output_channel_count: 2,
                pre_skip: 312,
                input_sample_rate: 48000,
                output_gain: 0,
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), OpusBox::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Mp4a の inner_box() テスト
    #[test]
    fn sample_entry_mp4a_inner_box() {
        let entry = SampleEntry::Mp4a(Mp4aBox {
            audio: create_audio_fields(),
            esds_box: EsdsBox {
                es: EsDescriptor {
                    es_id: 1,
                    stream_priority: Uint::new(0),
                    depends_on_es_id: None,
                    url_string: None,
                    ocr_es_id: None,
                    dec_config_descr: DecoderConfigDescriptor {
                        object_type_indication: 0x40,
                        stream_type: Uint::new(0x05),
                        up_stream: Uint::new(0),
                        buffer_size_db: Uint::new(0),
                        max_bitrate: 128000,
                        avg_bitrate: 128000,
                        dec_specific_info: None,
                    },
                    sl_config_descr: SlConfigDescriptor,
                },
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Mp4aBox::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Flac の inner_box() テスト
    #[test]
    fn sample_entry_flac_inner_box() {
        let entry = SampleEntry::Flac(FlacBox {
            audio: create_audio_fields(),
            dfla_box: DflaBox {
                metadata_blocks: vec![FlacMetadataBlock {
                    last_metadata_block_flag: Uint::new(1),
                    block_type: Uint::new(0),
                    block_data: vec![0; 34],
                }],
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), FlacBox::TYPE);
        assert!(!entry.is_unknown_box());
    }

    /// SampleEntry::Unknown の inner_box() テスト
    #[test]
    fn sample_entry_unknown_inner_box() {
        let entry = SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::U32(8),
            payload: vec![],
        });

        assert_eq!(entry.box_type(), BoxType::Normal(*b"test"));
        assert!(entry.is_unknown_box());
    }
}

// ===== boxes_moov_tree.rs のエラーパステスト =====

mod moov_tree_error_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        Decode, Encode, FixedPointNumber, Mp4FileTime, Utf8String,
        boxes::{
            Co64Box, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry, HdlrBox, MdhdBox, MinfBox,
            MvhdBox, StblBox, StcoBox, StscBox, StsdBox, StszBox, SttsBox, TkhdBox, UrlBox,
            VmhdBox,
        },
    };

    // ===== MdhdBox の不正な言語コードエラー =====

    /// MdhdBox: 言語コードが 0x60 未満でエンコードエラー
    #[test]
    fn mdhd_box_invalid_language_code_low() {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            duration: 0,
            language: [0x00, 0x61, 0x61], // 最初の文字が 0x60 未満
        };
        let result = mdhd.encode_to_vec();
        assert!(result.is_err());
    }

    /// MdhdBox: 言語コードが 0x60 未満 (2番目の文字)
    #[test]
    fn mdhd_box_invalid_language_code_middle() {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            duration: 0,
            language: [0x61, 0x00, 0x61], // 2番目の文字が 0x60 未満
        };
        let result = mdhd.encode_to_vec();
        assert!(result.is_err());
    }

    /// MdhdBox: 言語コードが 0x60 未満 (3番目の文字)
    #[test]
    fn mdhd_box_invalid_language_code_last() {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            duration: 0,
            language: [0x61, 0x61, 0x00], // 3番目の文字が 0x60 未満
        };
        let result = mdhd.encode_to_vec();
        assert!(result.is_err());
    }

    // ===== FullBox version == 1 パスのテスト (64ビット版) =====

    /// MvhdBox: version 1 (64ビット) - creation_time が u32::MAX を超える
    #[test]
    fn mvhd_box_version_1_large_creation_time() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(u32::MAX as u64 + 1),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("timescale should be non-zero"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.creation_time.as_secs(), u32::MAX as u64 + 1);
    }

    /// MvhdBox: version 1 (64ビット) - modification_time が u32::MAX を超える
    #[test]
    fn mvhd_box_version_1_large_modification_time() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(u32::MAX as u64 + 1),
            timescale: NonZeroU32::new(1000).expect("timescale should be non-zero"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.modification_time.as_secs(), u32::MAX as u64 + 1);
    }

    /// MvhdBox: version 1 (64ビット) - duration が u32::MAX を超える
    #[test]
    fn mvhd_box_version_1_large_duration() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("timescale should be non-zero"),
            duration: u32::MAX as u64 + 1,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.duration, u32::MAX as u64 + 1);
    }

    /// TkhdBox: version 1 (64ビット) - creation_time が u32::MAX を超える
    #[test]
    fn tkhd_box_version_1_large_creation_time() {
        let tkhd = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: Mp4FileTime::from_secs(u32::MAX as u64 + 1),
            modification_time: Mp4FileTime::from_secs(0),
            track_id: 1,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(0, 0),
            height: FixedPointNumber::new(0, 0),
        };
        let encoded = tkhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = TkhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.creation_time.as_secs(), u32::MAX as u64 + 1);
    }

    /// TkhdBox: 全フラグを有効化
    #[test]
    fn tkhd_box_all_flags_enabled() {
        let tkhd = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: true,
            flag_track_size_is_aspect_ratio: true,
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            track_id: 1,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(0, 0),
            height: FixedPointNumber::new(0, 0),
        };
        let encoded = tkhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = TkhdBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.flag_track_enabled);
        assert!(decoded.flag_track_in_movie);
        assert!(decoded.flag_track_in_preview);
        assert!(decoded.flag_track_size_is_aspect_ratio);
    }

    /// MdhdBox: version 1 (64ビット) - creation_time が u32::MAX を超える
    #[test]
    fn mdhd_box_version_1_large_creation_time() {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(u32::MAX as u64 + 1),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            duration: 0,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };
        let encoded = mdhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = MdhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.creation_time.as_secs(), u32::MAX as u64 + 1);
    }

    // ===== ElstBox のテスト =====

    /// ElstBox: version 1 (64ビット) - edit_duration が u32::MAX を超える
    #[test]
    fn elst_box_version_1_large_edit_duration() {
        let elst = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: u32::MAX as u64 + 1,
                media_time: 0,
                media_rate: FixedPointNumber::new(1, 0),
            }],
        };
        let encoded = elst.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = ElstBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.entries[0].edit_duration, u32::MAX as u64 + 1);
    }

    /// ElstBox: version 1 (64ビット) - media_time が i32::MAX を超える
    #[test]
    fn elst_box_version_1_large_media_time() {
        let elst = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: 1000,
                media_time: i32::MAX as i64 + 1,
                media_rate: FixedPointNumber::new(1, 0),
            }],
        };
        let encoded = elst.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = ElstBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.entries[0].media_time, i32::MAX as i64 + 1);
    }

    /// ElstBox: version 1 (64ビット) - media_time が i32::MIN を下回る
    #[test]
    fn elst_box_version_1_negative_media_time() {
        let elst = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: 1000,
                media_time: i32::MIN as i64 - 1,
                media_rate: FixedPointNumber::new(1, 0),
            }],
        };
        let encoded = elst.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = ElstBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.entries[0].media_time, i32::MIN as i64 - 1);
    }

    /// ElstBox: 複数エントリ
    #[test]
    fn elst_box_multiple_entries() {
        let elst = ElstBox {
            entries: vec![
                ElstEntry {
                    edit_duration: 1000,
                    media_time: 0,
                    media_rate: FixedPointNumber::new(1, 0),
                },
                ElstEntry {
                    edit_duration: 2000,
                    media_time: 1000,
                    media_rate: FixedPointNumber::new(2, 0),
                },
            ],
        };
        let encoded = elst.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = ElstBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.entries.len(), 2);
    }

    // ===== EdtsBox のテスト =====

    /// EdtsBox: elst_box を含む
    #[test]
    fn edts_box_with_elst() {
        let edts = EdtsBox {
            elst_box: Some(ElstBox {
                entries: vec![ElstEntry {
                    edit_duration: 1000,
                    media_time: 0,
                    media_rate: FixedPointNumber::new(1, 0),
                }],
            }),
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = EdtsBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.elst_box.is_some());
    }

    /// EdtsBox: elst_box なし
    #[test]
    fn edts_box_without_elst() {
        let edts = EdtsBox {
            elst_box: None,
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = EdtsBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.elst_box.is_none());
    }

    // ===== UrlBox のテスト =====

    /// UrlBox: location あり
    #[test]
    fn url_box_with_location() {
        let url = UrlBox {
            location: Utf8String::new("http://example.com"),
        };
        let encoded = url.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = UrlBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.location.is_some());
        assert_eq!(
            decoded.location.as_ref().map(|l| l.get()),
            Some("http://example.com")
        );
    }

    /// UrlBox: location なし (LOCAL_FILE)
    #[test]
    fn url_box_local_file() {
        let url = UrlBox::LOCAL_FILE;
        let encoded = url.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = UrlBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.location.is_none());
    }

    // ===== DrefBox のテスト =====

    /// DrefBox: url_box なし
    #[test]
    fn dref_box_without_url() {
        let dref = DrefBox {
            url_box: None,
            unknown_boxes: vec![],
        };
        let encoded = dref.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = DrefBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.url_box.is_none());
    }

    // ===== MinfBox のテスト =====

    /// MinfBox: smhd_or_vmhd_box なし
    #[test]
    fn minf_box_without_smhd_vmhd() {
        use shiguredo_mp4::Either;
        use shiguredo_mp4::boxes::{AudioSampleEntryFields, DopsBox, OpusBox, SampleEntry};
        use std::num::NonZeroU16;

        let minf = MinfBox {
            smhd_or_vmhd_box: None,
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: StblBox {
                stsd_box: StsdBox {
                    entries: vec![SampleEntry::Opus(OpusBox {
                        audio: AudioSampleEntryFields {
                            data_reference_index: NonZeroU16::new(1)
                                .expect("data_reference_index should be non-zero"),
                            channelcount: 2,
                            samplesize: 16,
                            samplerate: FixedPointNumber::new(48000, 0),
                        },
                        dops_box: DopsBox {
                            output_channel_count: 2,
                            pre_skip: 312,
                            input_sample_rate: 48000,
                            output_gain: 0,
                        },
                        unknown_boxes: vec![],
                    })],
                },
                stts_box: SttsBox { entries: vec![] },
                stsc_box: StscBox { entries: vec![] },
                stsz_box: StszBox::Variable {
                    entry_sizes: vec![],
                },
                stco_or_co64_box: Either::A(StcoBox {
                    chunk_offsets: vec![],
                }),
                stss_box: None,
                ctts_box: None,
                cslg_box: None,
                sdtp_box: None,
                unknown_boxes: vec![],
            },
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = MinfBox::decode(&encoded).expect("decode should succeed");
        assert!(decoded.smhd_or_vmhd_box.is_none());
    }

    // ===== HdlrBox のテスト =====

    /// HdlrBox: video ハンドラータイプ
    #[test]
    fn hdlr_box_video_handler() {
        let hdlr = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: b"VideoHandler\0".to_vec(),
        };
        let encoded = hdlr.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = HdlrBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.handler_type, HdlrBox::HANDLER_TYPE_VIDE);
    }

    // ===== VmhdBox のテスト =====

    /// VmhdBox: 非デフォルト値
    #[test]
    fn vmhd_box_non_default() {
        let vmhd = VmhdBox {
            graphicsmode: 100,
            opcolor: [255, 128, 64],
        };
        let encoded = vmhd.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = VmhdBox::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.graphicsmode, 100);
        assert_eq!(decoded.opcolor, [255, 128, 64]);
    }

    // ===== StszBox のテスト =====

    /// StszBox: Fixed サイズ
    #[test]
    fn stsz_box_fixed_size() {
        let stsz = StszBox::Fixed {
            sample_size: NonZeroU32::new(1024).expect("sample_size should be non-zero"),
            sample_count: 100,
        };
        let encoded = stsz.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = StszBox::decode(&encoded).expect("decode should succeed");
        match decoded {
            StszBox::Fixed {
                sample_size,
                sample_count,
            } => {
                assert_eq!(sample_size.get(), 1024);
                assert_eq!(sample_count, 100);
            }
            _ => panic!("Expected Fixed variant"),
        }
    }

    // ===== Co64Box のテスト =====

    /// Co64Box: 大きなオフセット値
    #[test]
    fn co64_box_large_offsets() {
        let co64 = Co64Box {
            chunk_offsets: vec![u32::MAX as u64 + 1, u64::MAX / 2],
        };
        let encoded = co64.encode_to_vec().expect("encode should succeed");
        let (decoded, _) = Co64Box::decode(&encoded).expect("decode should succeed");
        assert_eq!(decoded.chunk_offsets.len(), 2);
        assert_eq!(decoded.chunk_offsets[0], u32::MAX as u64 + 1);
    }
}

// ===== descriptors.rs のエラーパステスト =====

mod descriptor_error_tests {
    use shiguredo_mp4::{
        Decode, Encode, Uint,
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

    // ===== EsDescriptor のエラーパス =====

    /// EsDescriptor: URL 文字列が長すぎる (256バイト以上)
    #[test]
    fn es_descriptor_url_too_long() {
        let desc = EsDescriptor {
            es_id: 1,
            stream_priority: Uint::new(0),
            depends_on_es_id: None,
            url_string: Some("x".repeat(256)), // 256 バイト
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication: 0x40,
                stream_type: Uint::new(0x05),
                up_stream: Uint::new(0),
                buffer_size_db: Uint::new(0),
                max_bitrate: 0,
                avg_bitrate: 0,
                dec_specific_info: None,
            },
            sl_config_descr: SlConfigDescriptor,
        };
        let result = desc.encode_to_vec();
        assert!(result.is_err());
    }

    /// EsDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn es_descriptor_invalid_tag() {
        // tag = 4 (DecoderConfigDescriptor のタグ) だが EsDescriptor を期待
        let data = [
            0x04, // tag = 4 (不正、3 を期待)
            0x05, // size = 5
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x00, 0x00, // padding
        ];
        let result = EsDescriptor::decode(&data);
        assert!(result.is_err());
    }

    // ===== DecoderConfigDescriptor のエラーパス =====

    /// DecoderConfigDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn decoder_config_descriptor_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、4 を期待)
            0x05, // size = 5
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
        ];
        let result = DecoderConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// DecoderConfigDescriptor: buffer_size_db がバッファ境界を超過
    #[test]
    fn decoder_config_descriptor_buffer_size_exceeds_boundary() {
        let data = [
            0x04, // tag = 4
            0x02, // size = 2 (小さすぎ)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
                  // buffer_size_db の 3 バイトがない
        ];
        let result = DecoderConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    // ===== DecoderSpecificInfo のエラーパス =====

    /// DecoderSpecificInfo: 不正なタグでのデコードエラー
    #[test]
    fn decoder_specific_info_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、5 を期待)
            0x02, // size = 2
            0x11, 0x90, // payload
        ];
        let result = DecoderSpecificInfo::decode(&data);
        assert!(result.is_err());
    }

    /// DecoderSpecificInfo: ペイロードがバッファ境界を超過
    #[test]
    fn decoder_specific_info_payload_exceeds_boundary() {
        let data = [
            0x05, // tag = 5
            0xFF, 0x01, // size = 129 (境界超過)
            0x11, 0x90, // 2 バイトしかない
        ];
        let result = DecoderSpecificInfo::decode(&data);
        assert!(result.is_err());
    }

    // ===== SlConfigDescriptor のエラーパス =====

    /// SlConfigDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn sl_config_descriptor_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、6 を期待)
            0x01, // size = 1
            0x02, // predefined = 2
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// SlConfigDescriptor: 未サポートの predefined 値
    #[test]
    fn sl_config_descriptor_unsupported_predefined() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x00, // predefined = 0 (未サポート、2 のみ対応)
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// SlConfigDescriptor: predefined = 1 (未サポート)
    #[test]
    fn sl_config_descriptor_predefined_1() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x01, // predefined = 1 (未サポート)
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// SlConfigDescriptor: predefined = 3 (未サポート)
    #[test]
    fn sl_config_descriptor_predefined_3() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x03, // predefined = 3 (未サポート)
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }
}

// ===== mux.rs のエラーパステスト =====

mod mux_error_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        FixedPointNumber, TrackKind, Uint,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry,
            VisualSampleEntryFields,
        },
        mux::{Mp4FileMuxer, MuxError, Sample},
    };

    fn create_avc1_sample_entry() -> SampleEntry {
        SampleEntry::Avc1(Avc1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
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
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
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

    /// タイムスケール不一致エラー (Video)
    #[test]
    fn timescale_mismatch_video() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer should be created");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 最初のサンプル (timescale = 30)
        let sample1 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("timescale should be non-zero"),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample1)
            .expect("sample1 should succeed");

        // 2番目のサンプル (timescale = 60) - 不一致
        let sample2 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::new(60).expect("timescale should be non-zero"), // 不一致
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 512,
        };
        let result = muxer.append_sample(&sample2);
        assert!(matches!(
            result,
            Err(MuxError::TimescaleMismatch {
                track_kind: TrackKind::Video,
                ..
            })
        ));
    }

    /// タイムスケール不一致エラー (Audio)
    #[test]
    fn timescale_mismatch_audio() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer should be created");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 最初のサンプル (timescale = 48000)
        let sample1 = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: false,
            timescale: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            duration: 960,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 256,
        };
        muxer
            .append_sample(&sample1)
            .expect("sample1 should succeed");

        // 2番目のサンプル (timescale = 44100) - 不一致
        let sample2 = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::new(44100).expect("timescale should be non-zero"), // 不一致
            duration: 1024,
            composition_time_offset: None,
            data_offset: initial_size + 256,
            data_size: 256,
        };
        let result = muxer.append_sample(&sample2);
        assert!(matches!(
            result,
            Err(MuxError::TimescaleMismatch {
                track_kind: TrackKind::Audio,
                ..
            })
        ));
    }

    /// MuxError の Display 実装テスト
    #[test]
    fn mux_error_display() {
        // PositionMismatch
        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        let display_str = format!("{}", pos_error);
        assert!(display_str.contains("100"));
        assert!(display_str.contains("200"));

        // MissingSampleEntry
        let missing_error = MuxError::MissingSampleEntry {
            track_kind: TrackKind::Video,
        };
        let display_str = format!("{}", missing_error);
        assert!(display_str.contains("Video"));

        // AlreadyFinalized
        let finalized_error = MuxError::AlreadyFinalized;
        let display_str = format!("{}", finalized_error);
        assert!(display_str.contains("finalized"));

        // TimescaleMismatch
        let timescale_error = MuxError::TimescaleMismatch {
            track_kind: TrackKind::Audio,
            expected: NonZeroU32::new(48000).expect("timescale should be non-zero"),
            actual: NonZeroU32::new(44100).expect("timescale should be non-zero"),
        };
        let display_str = format!("{}", timescale_error);
        assert!(display_str.contains("Audio"));
        assert!(display_str.contains("48000"));
        assert!(display_str.contains("44100"));
    }

    /// MuxError の Debug 実装テスト
    /// Debug 実装は Display と同じ出力を返すため、Display の出力を検証する
    #[test]
    fn mux_error_debug() {
        let error = MuxError::AlreadyFinalized;
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("finalized"));

        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        let debug_str = format!("{:?}", pos_error);
        assert!(debug_str.contains("mismatch"));
    }

    /// MuxError::source() のテスト
    #[test]
    fn mux_error_source() {
        use std::error::Error as StdError;

        // 他のエラーでは source は None
        let other_error = MuxError::AlreadyFinalized;
        assert!(other_error.source().is_none());

        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        assert!(pos_error.source().is_none());
    }

    /// 二重 finalize エラーのテスト
    #[test]
    fn double_finalize_error() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer should be created");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("timescale should be non-zero"),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer.append_sample(&sample).expect("sample should succeed");

        // 最初の finalize は成功
        muxer.finalize().expect("first finalize should succeed");

        // 2回目の finalize は失敗
        let result = muxer.finalize();
        assert!(matches!(result, Err(MuxError::AlreadyFinalized)));
    }
}

// ===== BaseBox トレイトのテスト =====

mod base_box_tests {
    use std::num::{NonZeroU16, NonZeroU32};

    use shiguredo_mp4::{
        BaseBox, BoxType, Decode, Either, Encode, FixedPointNumber, Mp4FileTime,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, Co64Box, DinfBox, DopsBox, DrefBox, EdtsBox,
            ElstBox, ElstEntry, HdlrBox, Hev1Box, HvccBox, HvccNalUintArray, MdhdBox, MdiaBox,
            MinfBox, MoovBox, MvhdBox, OpusBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox,
            StsdBox, StszBox, SttsBox, TkhdBox, TrakBox, UrlBox, VmhdBox,
        },
    };

    /// MoovBox の box_type() と children() テスト
    #[test]
    fn moov_box_base_box() {
        let moov = create_minimal_moov_box();
        assert_eq!(moov.box_type(), BoxType::Normal(*b"moov"));
        let children: Vec<_> = moov.children().collect();
        assert!(!children.is_empty());
    }

    /// MvhdBox の box_type() と children() テスト
    #[test]
    fn mvhd_box_base_box() {
        let mvhd = create_mvhd_box();
        assert_eq!(mvhd.box_type(), BoxType::Normal(*b"mvhd"));
        let children: Vec<_> = mvhd.children().collect();
        assert!(children.is_empty());
    }

    /// TrakBox の box_type() と children() テスト
    #[test]
    fn trak_box_base_box() {
        let trak = create_video_trak_box();
        assert_eq!(trak.box_type(), BoxType::Normal(*b"trak"));
        let children: Vec<_> = trak.children().collect();
        assert!(!children.is_empty());
    }

    /// TkhdBox の box_type() と children() テスト
    #[test]
    fn tkhd_box_base_box() {
        let tkhd = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            track_id: 1,
            duration: 1000,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(1920, 0),
            height: FixedPointNumber::new(1080, 0),
        };
        assert_eq!(tkhd.box_type(), BoxType::Normal(*b"tkhd"));
        let children: Vec<_> = tkhd.children().collect();
        assert!(children.is_empty());
    }

    /// MdiaBox の box_type() と children() テスト
    #[test]
    fn mdia_box_base_box() {
        let mdia = create_video_mdia_box();
        assert_eq!(mdia.box_type(), BoxType::Normal(*b"mdia"));
        let children: Vec<_> = mdia.children().collect();
        assert!(!children.is_empty());
    }

    /// MdhdBox の box_type() と children() テスト
    #[test]
    fn mdhd_box_base_box() {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(30).expect("timescale should be non-zero"),
            duration: 1000,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };
        assert_eq!(mdhd.box_type(), BoxType::Normal(*b"mdhd"));
        let children: Vec<_> = mdhd.children().collect();
        assert!(children.is_empty());
    }

    /// HdlrBox の box_type() と children() テスト
    #[test]
    fn hdlr_box_base_box() {
        let hdlr = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: vec![],
        };
        assert_eq!(hdlr.box_type(), BoxType::Normal(*b"hdlr"));
        let children: Vec<_> = hdlr.children().collect();
        assert!(children.is_empty());
    }

    /// MinfBox の box_type() と children() テスト
    #[test]
    fn minf_box_base_box() {
        let minf = create_video_minf_box();
        assert_eq!(minf.box_type(), BoxType::Normal(*b"minf"));
        let children: Vec<_> = minf.children().collect();
        assert!(!children.is_empty());
    }

    /// VmhdBox の box_type() と children() テスト
    #[test]
    fn vmhd_box_base_box() {
        let vmhd = VmhdBox {
            graphicsmode: VmhdBox::DEFAULT_GRAPHICSMODE,
            opcolor: VmhdBox::DEFAULT_OPCOLOR,
        };
        assert_eq!(vmhd.box_type(), BoxType::Normal(*b"vmhd"));
        let children: Vec<_> = vmhd.children().collect();
        assert!(children.is_empty());
    }

    /// SmhdBox の box_type() と children() テスト
    #[test]
    fn smhd_box_base_box() {
        let smhd = SmhdBox {
            balance: SmhdBox::DEFAULT_BALANCE,
        };
        assert_eq!(smhd.box_type(), BoxType::Normal(*b"smhd"));
        let children: Vec<_> = smhd.children().collect();
        assert!(children.is_empty());
    }

    /// DinfBox の box_type() と children() テスト
    #[test]
    fn dinf_box_base_box() {
        let dinf = DinfBox::LOCAL_FILE;
        assert_eq!(dinf.box_type(), BoxType::Normal(*b"dinf"));
        let children: Vec<_> = dinf.children().collect();
        assert!(!children.is_empty());
    }

    /// DrefBox の box_type() と children() テスト
    #[test]
    fn dref_box_base_box() {
        let dref = DrefBox {
            url_box: Some(UrlBox::LOCAL_FILE),
            unknown_boxes: vec![],
        };
        assert_eq!(dref.box_type(), BoxType::Normal(*b"dref"));
        let children: Vec<_> = dref.children().collect();
        assert!(!children.is_empty());
    }

    /// UrlBox の box_type() と children() テスト
    #[test]
    fn url_box_base_box() {
        let url = UrlBox::LOCAL_FILE;
        assert_eq!(url.box_type(), BoxType::Normal(*b"url "));
        let children: Vec<_> = url.children().collect();
        assert!(children.is_empty());
    }

    /// StblBox の box_type() と children() テスト
    #[test]
    fn stbl_box_base_box() {
        let stbl = create_empty_stbl_box();
        assert_eq!(stbl.box_type(), BoxType::Normal(*b"stbl"));
        let children: Vec<_> = stbl.children().collect();
        assert!(!children.is_empty());
    }

    /// StsdBox の box_type() と children() テスト
    #[test]
    fn stsd_box_base_box() {
        let stsd = StsdBox { entries: vec![] };
        assert_eq!(stsd.box_type(), BoxType::Normal(*b"stsd"));
        let children: Vec<_> = stsd.children().collect();
        assert!(children.is_empty());
    }

    /// SttsBox の box_type() と children() テスト
    #[test]
    fn stts_box_base_box() {
        let stts = SttsBox { entries: vec![] };
        assert_eq!(stts.box_type(), BoxType::Normal(*b"stts"));
        let children: Vec<_> = stts.children().collect();
        assert!(children.is_empty());
    }

    /// StscBox の box_type() と children() テスト
    #[test]
    fn stsc_box_base_box() {
        let stsc = StscBox { entries: vec![] };
        assert_eq!(stsc.box_type(), BoxType::Normal(*b"stsc"));
        let children: Vec<_> = stsc.children().collect();
        assert!(children.is_empty());
    }

    /// StszBox の box_type() と children() テスト
    #[test]
    fn stsz_box_base_box() {
        let stsz = StszBox::Variable {
            entry_sizes: vec![],
        };
        assert_eq!(stsz.box_type(), BoxType::Normal(*b"stsz"));
        let children: Vec<_> = stsz.children().collect();
        assert!(children.is_empty());
    }

    /// StcoBox の box_type() と children() テスト
    #[test]
    fn stco_box_base_box() {
        let stco = StcoBox {
            chunk_offsets: vec![],
        };
        assert_eq!(stco.box_type(), BoxType::Normal(*b"stco"));
        let children: Vec<_> = stco.children().collect();
        assert!(children.is_empty());
    }

    /// Co64Box の box_type() と children() テスト
    #[test]
    fn co64_box_base_box() {
        let co64 = Co64Box {
            chunk_offsets: vec![],
        };
        assert_eq!(co64.box_type(), BoxType::Normal(*b"co64"));
        let children: Vec<_> = co64.children().collect();
        assert!(children.is_empty());
    }

    /// EdtsBox の box_type() と children() テスト
    #[test]
    fn edts_box_base_box() {
        let edts = EdtsBox {
            elst_box: Some(ElstBox { entries: vec![] }),
            unknown_boxes: vec![],
        };
        assert_eq!(edts.box_type(), BoxType::Normal(*b"edts"));
        let children: Vec<_> = edts.children().collect();
        assert!(!children.is_empty());
    }

    /// ElstBox の box_type() と children() テスト
    #[test]
    fn elst_box_base_box() {
        let elst = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: 1000,
                media_time: 0,
                media_rate: FixedPointNumber::new(1, 0),
            }],
        };
        assert_eq!(elst.box_type(), BoxType::Normal(*b"elst"));
        let children: Vec<_> = elst.children().collect();
        assert!(children.is_empty());
    }

    // ===== SampleEntry の box_type() と children() テスト =====

    /// Avc1Box の box_type() と children() テスト
    #[test]
    fn avc1_box_base_box() {
        let avc1 = create_avc1_box();
        assert_eq!(avc1.box_type(), BoxType::Normal(*b"avc1"));
        let children: Vec<_> = avc1.children().collect();
        assert!(!children.is_empty());
    }

    /// AvccBox の box_type() と children() テスト
    #[test]
    fn avcc_box_base_box() {
        let avcc = AvccBox {
            avc_profile_indication: 66,
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: shiguredo_mp4::Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        };
        assert_eq!(avcc.box_type(), BoxType::Normal(*b"avcC"));
        let children: Vec<_> = avcc.children().collect();
        assert!(children.is_empty());
    }

    /// Hev1Box の box_type() と children() テスト
    #[test]
    fn hev1_box_base_box() {
        let hev1 = create_hev1_box();
        assert_eq!(hev1.box_type(), BoxType::Normal(*b"hev1"));
        let children: Vec<_> = hev1.children().collect();
        assert!(!children.is_empty());
    }

    /// HvccBox の box_type() と children() テスト
    #[test]
    fn hvcc_box_base_box() {
        let hvcc = HvccBox {
            general_profile_space: shiguredo_mp4::Uint::new(0),
            general_tier_flag: shiguredo_mp4::Uint::new(0),
            general_profile_idc: shiguredo_mp4::Uint::new(1),
            general_profile_compatibility_flags: 0,
            general_constraint_indicator_flags: shiguredo_mp4::Uint::new(0),
            general_level_idc: 93,
            min_spatial_segmentation_idc: shiguredo_mp4::Uint::new(0),
            parallelism_type: shiguredo_mp4::Uint::new(0),
            chroma_format_idc: shiguredo_mp4::Uint::new(1),
            bit_depth_luma_minus8: shiguredo_mp4::Uint::new(0),
            bit_depth_chroma_minus8: shiguredo_mp4::Uint::new(0),
            avg_frame_rate: 0,
            constant_frame_rate: shiguredo_mp4::Uint::new(0),
            num_temporal_layers: shiguredo_mp4::Uint::new(1),
            temporal_id_nested: shiguredo_mp4::Uint::new(0),
            length_size_minus_one: shiguredo_mp4::Uint::new(3),
            nalu_arrays: vec![],
        };
        assert_eq!(hvcc.box_type(), BoxType::Normal(*b"hvcC"));
        let children: Vec<_> = hvcc.children().collect();
        assert!(children.is_empty());
    }

    /// OpusBox の box_type() と children() テスト
    #[test]
    fn opus_box_base_box() {
        let opus = create_opus_box();
        assert_eq!(opus.box_type(), BoxType::Normal(*b"Opus"));
        let children: Vec<_> = opus.children().collect();
        assert!(!children.is_empty());
    }

    /// DopsBox の box_type() と children() テスト
    #[test]
    fn dops_box_base_box() {
        let dops = DopsBox {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        };
        assert_eq!(dops.box_type(), BoxType::Normal(*b"dOps"));
        let children: Vec<_> = dops.children().collect();
        assert!(children.is_empty());
    }

    /// SampleEntry の box_type() テスト
    #[test]
    fn sample_entry_box_type() {
        let avc1 = SampleEntry::Avc1(create_avc1_box());
        assert_eq!(avc1.box_type(), BoxType::Normal(*b"avc1"));

        let hev1 = SampleEntry::Hev1(create_hev1_box());
        assert_eq!(hev1.box_type(), BoxType::Normal(*b"hev1"));

        let opus = SampleEntry::Opus(create_opus_box());
        assert_eq!(opus.box_type(), BoxType::Normal(*b"Opus"));
    }

    /// SampleEntry の children() テスト
    #[test]
    fn sample_entry_children() {
        let avc1 = SampleEntry::Avc1(create_avc1_box());
        let children: Vec<_> = avc1.children().collect();
        assert!(!children.is_empty());

        let opus = SampleEntry::Opus(create_opus_box());
        let children: Vec<_> = opus.children().collect();
        assert!(!children.is_empty());
    }

    // ===== ヘルパー関数 =====

    fn create_mvhd_box() -> MvhdBox {
        MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("timescale should be non-zero"),
            duration: 1000,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 2,
        }
    }

    fn create_empty_stbl_box() -> StblBox {
        StblBox {
            stsd_box: StsdBox { entries: vec![] },
            stts_box: SttsBox { entries: vec![] },
            stsc_box: StscBox { entries: vec![] },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![],
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: vec![],
        }
    }

    fn create_video_minf_box() -> MinfBox {
        MinfBox {
            smhd_or_vmhd_box: Some(Either::B(VmhdBox {
                graphicsmode: VmhdBox::DEFAULT_GRAPHICSMODE,
                opcolor: VmhdBox::DEFAULT_OPCOLOR,
            })),
            dinf_box: DinfBox::LOCAL_FILE,
            stbl_box: create_empty_stbl_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_video_mdia_box() -> MdiaBox {
        MdiaBox {
            mdhd_box: MdhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(30).expect("timescale should be non-zero"),
                duration: 1000,
                language: MdhdBox::LANGUAGE_UNDEFINED,
            },
            hdlr_box: HdlrBox {
                handler_type: HdlrBox::HANDLER_TYPE_VIDE,
                name: vec![],
            },
            minf_box: create_video_minf_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_video_trak_box() -> TrakBox {
        TrakBox {
            tkhd_box: TkhdBox {
                flag_track_enabled: true,
                flag_track_in_movie: true,
                flag_track_in_preview: false,
                flag_track_size_is_aspect_ratio: false,
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                track_id: 1,
                duration: 1000,
                layer: TkhdBox::DEFAULT_LAYER,
                alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
                volume: TkhdBox::DEFAULT_VIDEO_VOLUME,
                matrix: TkhdBox::DEFAULT_MATRIX,
                width: FixedPointNumber::new(1920, 0),
                height: FixedPointNumber::new(1080, 0),
            },
            edts_box: None,
            mdia_box: create_video_mdia_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_minimal_moov_box() -> MoovBox {
        MoovBox {
            mvhd_box: create_mvhd_box(),
            trak_boxes: vec![create_video_trak_box()],
            mvex_box: None,
            unknown_boxes: vec![],
        }
    }

    fn create_avc1_box() -> Avc1Box {
        use shiguredo_mp4::boxes::VisualSampleEntryFields;
        Avc1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
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
                length_size_minus_one: shiguredo_mp4::Uint::new(3),
                sps_list: vec![],
                pps_list: vec![],
                chroma_format: None,
                bit_depth_luma_minus8: None,
                bit_depth_chroma_minus8: None,
                sps_ext_list: vec![],
            },
            unknown_boxes: vec![],
        }
    }

    fn create_hev1_box() -> Hev1Box {
        use shiguredo_mp4::boxes::VisualSampleEntryFields;
        Hev1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            hvcc_box: HvccBox {
                general_profile_space: shiguredo_mp4::Uint::new(0),
                general_tier_flag: shiguredo_mp4::Uint::new(0),
                general_profile_idc: shiguredo_mp4::Uint::new(1),
                general_profile_compatibility_flags: 0,
                general_constraint_indicator_flags: shiguredo_mp4::Uint::new(0),
                general_level_idc: 93,
                min_spatial_segmentation_idc: shiguredo_mp4::Uint::new(0),
                parallelism_type: shiguredo_mp4::Uint::new(0),
                chroma_format_idc: shiguredo_mp4::Uint::new(1),
                bit_depth_luma_minus8: shiguredo_mp4::Uint::new(0),
                bit_depth_chroma_minus8: shiguredo_mp4::Uint::new(0),
                avg_frame_rate: 0,
                constant_frame_rate: shiguredo_mp4::Uint::new(0),
                num_temporal_layers: shiguredo_mp4::Uint::new(1),
                temporal_id_nested: shiguredo_mp4::Uint::new(0),
                length_size_minus_one: shiguredo_mp4::Uint::new(3),
                nalu_arrays: vec![HvccNalUintArray {
                    array_completeness: shiguredo_mp4::Uint::new(0),
                    nal_unit_type: shiguredo_mp4::Uint::new(32),
                    nalus: vec![],
                }],
            },
            unknown_boxes: vec![],
        }
    }

    fn create_opus_box() -> OpusBox {
        OpusBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1)
                    .expect("data_reference_index should be non-zero"),
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(48000, 0),
            },
            dops_box: DopsBox {
                output_channel_count: 2,
                pre_skip: 312,
                input_sample_rate: 48000,
                output_gain: 0,
            },
            unknown_boxes: vec![],
        }
    }

    // ===== boxes_sample_entry.rs の追加テスト =====

    /// Hvc1Box の box_type() と children() テスト
    #[test]
    fn hvc1_box_base_box() {
        let hvc1 = create_hvc1_box();
        assert_eq!(hvc1.box_type(), BoxType::Normal(*b"hvc1"));
        let children: Vec<_> = hvc1.children().collect();
        assert!(!children.is_empty());
    }

    /// Vp08Box の box_type() と children() テスト
    #[test]
    fn vp08_box_base_box() {
        let vp08 = create_vp08_box();
        assert_eq!(vp08.box_type(), BoxType::Normal(*b"vp08"));
        let children: Vec<_> = vp08.children().collect();
        assert!(!children.is_empty());
    }

    /// Vp09Box の box_type() と children() テスト
    #[test]
    fn vp09_box_base_box() {
        let vp09 = create_vp09_box();
        assert_eq!(vp09.box_type(), BoxType::Normal(*b"vp09"));
        let children: Vec<_> = vp09.children().collect();
        assert!(!children.is_empty());
    }

    /// VpccBox の box_type() と children() テスト
    #[test]
    fn vpcc_box_base_box() {
        let vpcc = create_vpcc_box();
        assert_eq!(vpcc.box_type(), BoxType::Normal(*b"vpcC"));
        let children: Vec<_> = vpcc.children().collect();
        assert!(children.is_empty());
    }

    /// Av01Box の box_type() と children() テスト
    #[test]
    fn av01_box_base_box() {
        let av01 = create_av01_box();
        assert_eq!(av01.box_type(), BoxType::Normal(*b"av01"));
        let children: Vec<_> = av01.children().collect();
        assert!(!children.is_empty());
    }

    /// Av1cBox の box_type() と children() テスト
    #[test]
    fn av1c_box_base_box() {
        let av1c = create_av1c_box();
        assert_eq!(av1c.box_type(), BoxType::Normal(*b"av1C"));
        let children: Vec<_> = av1c.children().collect();
        assert!(children.is_empty());
    }

    /// Mp4aBox の box_type() と children() テスト
    #[test]
    fn mp4a_box_base_box() {
        let mp4a = create_mp4a_box();
        assert_eq!(mp4a.box_type(), BoxType::Normal(*b"mp4a"));
        let children: Vec<_> = mp4a.children().collect();
        assert!(!children.is_empty());
    }

    /// FlacBox の box_type() と children() テスト
    #[test]
    fn flac_box_base_box() {
        let flac = create_flac_box();
        assert_eq!(flac.box_type(), BoxType::Normal(*b"fLaC"));
        let children: Vec<_> = flac.children().collect();
        assert!(!children.is_empty());
    }

    /// DflaBox の box_type() と children() テスト
    #[test]
    fn dfla_box_base_box() {
        let dfla = create_dfla_box();
        assert_eq!(dfla.box_type(), BoxType::Normal(*b"dfLa"));
        let children: Vec<_> = dfla.children().collect();
        assert!(children.is_empty());
    }

    // ===== 追加ヘルパー関数 =====

    fn create_hvc1_box() -> shiguredo_mp4::boxes::Hvc1Box {
        use shiguredo_mp4::boxes::{Hvc1Box, VisualSampleEntryFields};
        Hvc1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            hvcc_box: HvccBox {
                general_profile_space: shiguredo_mp4::Uint::new(0),
                general_tier_flag: shiguredo_mp4::Uint::new(0),
                general_profile_idc: shiguredo_mp4::Uint::new(1),
                general_profile_compatibility_flags: 0,
                general_constraint_indicator_flags: shiguredo_mp4::Uint::new(0),
                general_level_idc: 93,
                min_spatial_segmentation_idc: shiguredo_mp4::Uint::new(0),
                parallelism_type: shiguredo_mp4::Uint::new(0),
                chroma_format_idc: shiguredo_mp4::Uint::new(1),
                bit_depth_luma_minus8: shiguredo_mp4::Uint::new(0),
                bit_depth_chroma_minus8: shiguredo_mp4::Uint::new(0),
                avg_frame_rate: 0,
                constant_frame_rate: shiguredo_mp4::Uint::new(0),
                num_temporal_layers: shiguredo_mp4::Uint::new(1),
                temporal_id_nested: shiguredo_mp4::Uint::new(0),
                length_size_minus_one: shiguredo_mp4::Uint::new(3),
                nalu_arrays: vec![],
            },
            unknown_boxes: vec![],
        }
    }

    fn create_vp08_box() -> shiguredo_mp4::boxes::Vp08Box {
        use shiguredo_mp4::boxes::{VisualSampleEntryFields, Vp08Box};
        Vp08Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_vp09_box() -> shiguredo_mp4::boxes::Vp09Box {
        use shiguredo_mp4::boxes::{VisualSampleEntryFields, Vp09Box};
        Vp09Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_vpcc_box() -> shiguredo_mp4::boxes::VpccBox {
        use shiguredo_mp4::boxes::VpccBox;
        VpccBox {
            profile: 0,
            level: 10,
            bit_depth: shiguredo_mp4::Uint::new(8),
            chroma_subsampling: shiguredo_mp4::Uint::new(1),
            video_full_range_flag: shiguredo_mp4::Uint::new(0),
            colour_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
            codec_initialization_data: vec![],
        }
    }

    fn create_av01_box() -> shiguredo_mp4::boxes::Av01Box {
        use shiguredo_mp4::boxes::{Av01Box, VisualSampleEntryFields};
        Av01Box {
            visual: VisualSampleEntryFields {
                data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            av1c_box: create_av1c_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_av1c_box() -> shiguredo_mp4::boxes::Av1cBox {
        use shiguredo_mp4::boxes::Av1cBox;
        Av1cBox {
            seq_profile: shiguredo_mp4::Uint::new(0),
            seq_level_idx_0: shiguredo_mp4::Uint::new(0),
            seq_tier_0: shiguredo_mp4::Uint::new(0),
            high_bitdepth: shiguredo_mp4::Uint::new(0),
            twelve_bit: shiguredo_mp4::Uint::new(0),
            monochrome: shiguredo_mp4::Uint::new(0),
            chroma_subsampling_x: shiguredo_mp4::Uint::new(1),
            chroma_subsampling_y: shiguredo_mp4::Uint::new(1),
            chroma_sample_position: shiguredo_mp4::Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        }
    }

    fn create_mp4a_box() -> shiguredo_mp4::boxes::Mp4aBox {
        use shiguredo_mp4::boxes::{EsdsBox, Mp4aBox};
        use shiguredo_mp4::descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        };
        Mp4aBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1)
                    .expect("data_reference_index should be non-zero"),
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(48000, 0),
            },
            esds_box: EsdsBox {
                es: EsDescriptor {
                    es_id: 1,
                    stream_priority: shiguredo_mp4::Uint::new(0),
                    depends_on_es_id: None,
                    url_string: None,
                    ocr_es_id: None,
                    dec_config_descr: DecoderConfigDescriptor {
                        object_type_indication: 0x40,             // AAC
                        stream_type: shiguredo_mp4::Uint::new(5), // Audio
                        up_stream: shiguredo_mp4::Uint::new(0),
                        buffer_size_db: shiguredo_mp4::Uint::new(0),
                        max_bitrate: 128000,
                        avg_bitrate: 128000,
                        dec_specific_info: Some(DecoderSpecificInfo { payload: vec![] }),
                    },
                    sl_config_descr: SlConfigDescriptor,
                },
            },
            unknown_boxes: vec![],
        }
    }

    fn create_flac_box() -> shiguredo_mp4::boxes::FlacBox {
        use shiguredo_mp4::boxes::{DflaBox, FlacBox, FlacMetadataBlock};
        FlacBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1)
                    .expect("data_reference_index should be non-zero"),
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(48000, 0),
            },
            dfla_box: DflaBox {
                metadata_blocks: vec![FlacMetadataBlock {
                    last_metadata_block_flag: shiguredo_mp4::Uint::new(1),
                    block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
                    block_data: vec![0; 34],
                }],
            },
            unknown_boxes: vec![],
        }
    }

    fn create_dfla_box() -> shiguredo_mp4::boxes::DflaBox {
        use shiguredo_mp4::boxes::{DflaBox, FlacMetadataBlock};
        DflaBox {
            metadata_blocks: vec![FlacMetadataBlock {
                last_metadata_block_flag: shiguredo_mp4::Uint::new(1),
                block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
                block_data: vec![0; 34],
            }],
        }
    }

    // ===== SampleEntry::decode のコーデック分岐テスト =====

    /// SampleEntry::decode で Hvc1Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_hvc1() {
        let hvc1 = create_hvc1_box();
        let mut buf = vec![0u8; 4096];
        let size = hvc1.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Hvc1(_)));
    }

    /// SampleEntry::decode で Vp08Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_vp08() {
        let vp08 = create_vp08_box();
        let mut buf = vec![0u8; 4096];
        let size = vp08.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Vp08(_)));
    }

    /// SampleEntry::decode で Vp09Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_vp09() {
        let vp09 = create_vp09_box();
        let mut buf = vec![0u8; 4096];
        let size = vp09.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Vp09(_)));
    }

    /// SampleEntry::decode で Av01Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_av01() {
        let av01 = create_av01_box();
        let mut buf = vec![0u8; 4096];
        let size = av01.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Av01(_)));
    }

    /// SampleEntry::decode で Mp4aBox を直接デコードするテスト
    #[test]
    fn sample_entry_decode_mp4a() {
        let mp4a = create_mp4a_box();
        let mut buf = vec![0u8; 4096];
        let size = mp4a.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Mp4a(_)));
    }

    /// SampleEntry::decode で FlacBox を直接デコードするテスト
    #[test]
    fn sample_entry_decode_flac() {
        let flac = create_flac_box();
        let mut buf = vec![0u8; 4096];
        let size = flac.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Flac(_)));
    }

    /// SampleEntry::decode で Hev1Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_hev1() {
        let hev1 = create_hev1_box();
        let mut buf = vec![0u8; 4096];
        let size = hev1.encode(&mut buf).expect("encode should succeed");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("decode should succeed");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Hev1(_)));
    }
}
