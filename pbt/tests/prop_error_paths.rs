//! エラーパスの Property-Based Testing
//!
//! 各種 Box のエンコード/デコード時のエラーパスをテストする

// ===== SampleEntry のメソッド網羅テスト =====

mod sample_entry_inner_box_tests {
    use std::num::NonZeroU16;

    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, FixedPointNumber, Uint, Utf8String,
        boxes::{
            AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, DflaBox, DopsBox, EsdsBox,
            FlacBox, FlacMetadataBlock, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox, SampleEntry,
            StppBox, UnknownBox, VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox, VttCBox,
            WvttBox,
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

    /// SampleEntry::Stpp の inner_box() テスト
    ///
    /// Stpp は必須の型付き子ボックスを持たないため、`unknown_boxes` が空なら
    /// children も空になる（既存 SampleEntry の `assert!(count >= 1)` パターンとは異なる）
    #[test]
    fn sample_entry_stpp_inner_box() {
        let entry = SampleEntry::Stpp(StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
            schema_location: Utf8String::EMPTY,
            auxiliary_mime_types: Utf8String::EMPTY,
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), StppBox::TYPE);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.children().count(), 0);
    }

    /// SampleEntry::Wvtt の inner_box() テスト
    ///
    /// Wvtt は必須の型付き子ボックス `vttc_box` を持つため、`unknown_boxes` が空でも
    /// children は 1 個（vttc_box）になる
    #[test]
    fn sample_entry_wvtt_inner_box() {
        let entry = SampleEntry::Wvtt(WvttBox {
            data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
            vttc_box: VttCBox {
                config: String::from("WEBVTT"),
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), WvttBox::TYPE);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.children().count(), 1);
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

// ===== BaseBox トレイトのテスト =====

mod base_box_tests {
    use std::num::NonZeroU16;

    use shiguredo_mp4::{
        BaseBox, BoxType, Decode, Encode, FixedPointNumber,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, Hev1Box, HvccBox, HvccNalUintArray,
            OpusBox, SampleEntry,
        },
    };

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
