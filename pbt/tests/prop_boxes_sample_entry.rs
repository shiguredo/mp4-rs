//! `src/boxes_sample_entry.rs` に定義される SampleEntry 系ボックスの Property-Based Testing

use std::num::NonZeroU16;

use shiguredo_mp4::{
    FixedPointNumber, Uint,
    boxes::{AudioSampleEntryFields, Av1cBox, AvccBox, HvccBox, VisualSampleEntryFields, VpccBox},
};

// ===== 各 mod 共通のヘルパー =====

fn create_audio_fields() -> AudioSampleEntryFields {
    AudioSampleEntryFields {
        data_reference_index: NonZeroU16::new(1).expect("1 は非ゼロ"),
        channelcount: 2,
        samplesize: 16,
        samplerate: FixedPointNumber::new(48000, 0),
    }
}

fn create_visual_fields() -> VisualSampleEntryFields {
    VisualSampleEntryFields {
        data_reference_index: NonZeroU16::new(1).expect("1 は非ゼロ"),
        width: 1920,
        height: 1080,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
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

// ===== SampleEntry のメソッド網羅テスト =====

mod sample_entry_inner_box_tests {
    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Uint, Utf8String,
        boxes::{
            Av01Box, Avc1Box, DflaBox, DopsBox, EsdsBox, FlacBox, FlacMetadataBlock, Hev1Box,
            Hvc1Box, Mp4aBox, OpusBox, SampleEntry, StppBox, UnknownBox, Vp08Box, Vp09Box, VttCBox,
            WvttBox,
        },
        descriptors::{DecoderConfigDescriptor, EsDescriptor, SlConfigDescriptor},
    };

    use super::{
        create_audio_fields, create_av1c_box, create_avcc_box, create_hvcc_box,
        create_visual_fields, create_vpcc_box,
    };

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

// ===== SampleEntry / boxes_sample_entry.rs 系 BaseBox の実装テスト =====

mod sample_entry_base_box_tests {
    use shiguredo_mp4::{
        BaseBox, BoxType, Decode, Encode, Uint,
        boxes::{
            Av01Box, Avc1Box, DflaBox, DopsBox, EsdsBox, FlacBox, FlacMetadataBlock, Hev1Box,
            Hvc1Box, Mp4aBox, OpusBox, SampleEntry, Vp08Box, Vp09Box,
        },
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

    use super::{
        create_audio_fields, create_av1c_box, create_avcc_box, create_hvcc_box,
        create_visual_fields, create_vpcc_box,
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
        let avcc = create_avcc_box();
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
        let hvcc = create_hvcc_box();
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

    /// SampleEntry::children() の非空検証 (Avc1 / Opus)
    #[test]
    fn sample_entry_children_non_empty() {
        let avc1 = SampleEntry::Avc1(create_avc1_box());
        let children: Vec<_> = avc1.children().collect();
        assert!(!children.is_empty());

        let opus = SampleEntry::Opus(create_opus_box());
        let children: Vec<_> = opus.children().collect();
        assert!(!children.is_empty());
    }

    fn create_avc1_box() -> Avc1Box {
        Avc1Box {
            visual: create_visual_fields(),
            avcc_box: create_avcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_hev1_box() -> Hev1Box {
        Hev1Box {
            visual: create_visual_fields(),
            hvcc_box: create_hvcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_opus_box() -> OpusBox {
        OpusBox {
            audio: create_audio_fields(),
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

    fn create_hvc1_box() -> Hvc1Box {
        Hvc1Box {
            visual: create_visual_fields(),
            hvcc_box: create_hvcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_vp08_box() -> Vp08Box {
        Vp08Box {
            visual: create_visual_fields(),
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_vp09_box() -> Vp09Box {
        Vp09Box {
            visual: create_visual_fields(),
            vpcc_box: create_vpcc_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_av01_box() -> Av01Box {
        Av01Box {
            visual: create_visual_fields(),
            av1c_box: create_av1c_box(),
            unknown_boxes: vec![],
        }
    }

    fn create_mp4a_box() -> Mp4aBox {
        Mp4aBox {
            audio: create_audio_fields(),
            esds_box: EsdsBox {
                es: EsDescriptor {
                    es_id: 1,
                    stream_priority: Uint::new(0),
                    depends_on_es_id: None,
                    url_string: None,
                    ocr_es_id: None,
                    dec_config_descr: DecoderConfigDescriptor {
                        object_type_indication: 0x40, // AAC
                        stream_type: Uint::new(5),    // Audio
                        up_stream: Uint::new(0),
                        buffer_size_db: Uint::new(0),
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

    fn create_flac_box() -> FlacBox {
        FlacBox {
            audio: create_audio_fields(),
            dfla_box: DflaBox {
                metadata_blocks: vec![FlacMetadataBlock {
                    last_metadata_block_flag: Uint::new(1),
                    block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
                    block_data: vec![0; 34],
                }],
            },
            unknown_boxes: vec![],
        }
    }

    fn create_dfla_box() -> DflaBox {
        DflaBox {
            metadata_blocks: vec![FlacMetadataBlock {
                last_metadata_block_flag: Uint::new(1),
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

// ===== SampleEntry のメソッドテスト =====

mod sample_entry_tests {
    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Decode, Encode, Uint, Utf8String,
        boxes::{
            Av01Box, Av1cBox, Avc1Box, AvccBox, DopsBox, EsdsBox, FlacBox, FlacMetadataBlock,
            Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox, SampleEntry, StppBox, UnknownBox, Vp08Box,
            Vp09Box, VpccBox, VttCBox, WvttBox,
        },
        descriptors::{DecoderConfigDescriptor, EsDescriptor, SlConfigDescriptor},
    };

    use super::{create_audio_fields, create_visual_fields};

    /// テスト用の StppBox を生成する
    ///
    /// TTML 名前空間を持つ最小構成（schema_location / auxiliary_mime_types は空文字列）
    fn create_stpp_box() -> StppBox {
        StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
            schema_location: Utf8String::EMPTY,
            auxiliary_mime_types: Utf8String::EMPTY,
            unknown_boxes: vec![],
        }
    }

    /// テスト用の WvttBox を生成する
    ///
    /// 最小構成（`vttC.config = "WEBVTT"`、任意子は無し）
    fn create_wvtt_box() -> WvttBox {
        WvttBox {
            data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
            vttc_box: VttCBox {
                config: String::from("WEBVTT"),
            },
            unknown_boxes: vec![],
        }
    }

    /// SampleEntry::Opus の audio_* メソッドのテスト
    #[test]
    fn sample_entry_opus_audio_methods() {
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

        assert_eq!(entry.audio_channel_count(), Some(2));
        assert_eq!(entry.audio_sample_rate(), Some(48000));
        assert_eq!(entry.audio_sample_size(), Some(16));
        assert_eq!(entry.video_resolution(), None);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.box_type(), OpusBox::TYPE);
    }

    /// SampleEntry::Mp4a の audio_* メソッドのテスト
    #[test]
    fn sample_entry_mp4a_audio_methods() {
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

        assert_eq!(entry.audio_channel_count(), Some(2));
        assert_eq!(entry.audio_sample_rate(), Some(48000));
        assert_eq!(entry.audio_sample_size(), Some(16));
        assert_eq!(entry.video_resolution(), None);
    }

    /// SampleEntry::Flac の audio_* メソッドのテスト
    #[test]
    fn sample_entry_flac_audio_methods() {
        let entry = SampleEntry::Flac(FlacBox {
            audio: create_audio_fields(),
            dfla_box: shiguredo_mp4::boxes::DflaBox {
                metadata_blocks: vec![FlacMetadataBlock {
                    last_metadata_block_flag: Uint::new(1),
                    block_type: Uint::new(0),
                    block_data: vec![0; 34],
                }],
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.audio_channel_count(), Some(2));
        assert_eq!(entry.audio_sample_rate(), Some(48000));
        assert_eq!(entry.audio_sample_size(), Some(16));
        assert_eq!(entry.video_resolution(), None);
    }

    /// SampleEntry::Avc1 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_avc1_video_methods() {
        let entry = SampleEntry::Avc1(Avc1Box {
            visual: create_visual_fields(),
            avcc_box: AvccBox {
                avc_profile_indication: 66,
                profile_compatibility: 0,
                avc_level_indication: 40,
                length_size_minus_one: Uint::new(3),
                sps_list: vec![],
                pps_list: vec![],
                chroma_format: None,
                bit_depth_luma_minus8: None,
                bit_depth_chroma_minus8: None,
                sps_ext_list: vec![],
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.audio_channel_count(), None);
        assert_eq!(entry.audio_sample_rate(), None);
        assert_eq!(entry.audio_sample_size(), None);
        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Hev1 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_hev1_video_methods() {
        let entry = SampleEntry::Hev1(Hev1Box {
            visual: create_visual_fields(),
            hvcc_box: HvccBox {
                general_profile_space: Uint::new(0),
                general_tier_flag: Uint::new(0),
                general_profile_idc: Uint::new(1),
                general_profile_compatibility_flags: 0,
                general_constraint_indicator_flags: Uint::new(0),
                general_level_idc: 0,
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
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Hvc1 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_hvc1_video_methods() {
        let entry = SampleEntry::Hvc1(Hvc1Box {
            visual: create_visual_fields(),
            hvcc_box: HvccBox {
                general_profile_space: Uint::new(0),
                general_tier_flag: Uint::new(0),
                general_profile_idc: Uint::new(1),
                general_profile_compatibility_flags: 0,
                general_constraint_indicator_flags: Uint::new(0),
                general_level_idc: 0,
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
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Vp08 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_vp08_video_methods() {
        let entry = SampleEntry::Vp08(Vp08Box {
            visual: create_visual_fields(),
            vpcc_box: VpccBox {
                profile: 0,
                level: 10,
                bit_depth: Uint::new(8),
                chroma_subsampling: Uint::new(1),
                video_full_range_flag: Uint::new(0),
                colour_primaries: 1,
                transfer_characteristics: 1,
                matrix_coefficients: 1,
                codec_initialization_data: vec![],
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Vp09 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_vp09_video_methods() {
        let entry = SampleEntry::Vp09(Vp09Box {
            visual: create_visual_fields(),
            vpcc_box: VpccBox {
                profile: 0,
                level: 10,
                bit_depth: Uint::new(8),
                chroma_subsampling: Uint::new(1),
                video_full_range_flag: Uint::new(0),
                colour_primaries: 1,
                transfer_characteristics: 1,
                matrix_coefficients: 1,
                codec_initialization_data: vec![],
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Av01 の video_resolution メソッドのテスト
    #[test]
    fn sample_entry_av01_video_methods() {
        let entry = SampleEntry::Av01(Av01Box {
            visual: create_visual_fields(),
            av1c_box: Av1cBox {
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
            },
            unknown_boxes: vec![],
        });

        assert_eq!(entry.video_resolution(), Some((1920, 1080)));
    }

    /// SampleEntry::Unknown のテスト
    #[test]
    fn sample_entry_unknown_methods() {
        let entry = SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::U32(8),
            payload: vec![],
        });

        assert_eq!(entry.audio_channel_count(), None);
        assert_eq!(entry.audio_sample_rate(), None);
        assert_eq!(entry.audio_sample_size(), None);
        assert_eq!(entry.video_resolution(), None);
        assert!(entry.is_unknown_box());
    }

    /// SampleEntry の encode/decode roundtrip テスト
    #[test]
    fn sample_entry_encode_decode_roundtrip() {
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

        let encoded = entry.encode_to_vec().unwrap();
        let (decoded, size) = SampleEntry::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, SampleEntry::Opus(_)));
        assert_eq!(decoded.audio_channel_count(), Some(2));
    }

    /// SampleEntry::Opus の children() 件数検証
    #[test]
    fn sample_entry_opus_children_count() {
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

        // Opus の children は dops_box
        let children: Vec<_> = entry.children().collect();
        assert_eq!(children.len(), 1);
    }

    /// SampleEntry::Stpp のメソッドおよび分類のテスト
    ///
    /// 字幕トラックなので audio_*・video_resolution はいずれも None を返し、
    /// is_unknown_box は false（型付きの Stpp バリアントとして識別される）
    #[test]
    fn sample_entry_stpp_methods() {
        let entry = SampleEntry::Stpp(create_stpp_box());

        assert_eq!(entry.audio_channel_count(), None);
        assert_eq!(entry.audio_sample_rate(), None);
        assert_eq!(entry.audio_sample_size(), None);
        assert_eq!(entry.video_resolution(), None);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.box_type(), StppBox::TYPE);
    }

    /// SampleEntry::Stpp の encode/decode ラウンドトリップ
    ///
    /// stpp サンプルエントリーが型付きで decode されて Stpp バリアントに復元されることを検証する
    #[test]
    fn sample_entry_stpp_encode_decode_roundtrip() {
        let entry = SampleEntry::Stpp(create_stpp_box());

        let encoded = entry.encode_to_vec().unwrap();
        let (decoded, size) = SampleEntry::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        // 3 フィールドが正しく復元されていることを確認する
        let SampleEntry::Stpp(decoded_stpp) = decoded else {
            unreachable!();
        };
        assert_eq!(decoded_stpp.namespace.get(), "http://www.w3.org/ns/ttml");
        assert_eq!(decoded_stpp.schema_location.get(), "");
        assert_eq!(decoded_stpp.auxiliary_mime_types.get(), "");
    }

    /// 有効な stpp box のバイト列を組み立てるヘルパー
    ///
    /// SampleEntry ヘッダー（8 バイト）と 3 本の null 終端文字列で構成する。
    /// エラーテストで一部フィールドを差し替える起点として使う
    fn build_valid_stpp_bytes(
        namespace: &[u8],
        schema_location: &[u8],
        auxiliary_mime_types: &[u8],
    ) -> Vec<u8> {
        // ペイロード: 6 bytes reserved + data_reference_index (u16) + 3 本の null 終端文字列
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(namespace);
        payload.push(0); // null 終端
        payload.extend_from_slice(schema_location);
        payload.push(0);
        payload.extend_from_slice(auxiliary_mime_types);
        payload.push(0);

        // BoxHeader: size (4B) + type (4B, "stpp")
        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"stpp");
        bytes.extend(payload);
        bytes
    }

    /// 有効なバイト列で組み立てて decode できることを念のため確認する
    #[test]
    fn stpp_box_decode_valid_bytes() {
        let bytes = build_valid_stpp_bytes(b"http://www.w3.org/ns/ttml", b"", b"");
        let (decoded, _) = StppBox::decode(&bytes).unwrap();
        assert_eq!(decoded.namespace.get(), "http://www.w3.org/ns/ttml");
    }

    /// namespace の null 終端が無いと invalid_input エラーになる
    ///
    /// エラーメッセージには "stpp.namespace" が含まれる（`StppBox::decode` 内で
    /// `.map_err(|e| Error::invalid_input(format!("stpp.namespace: {e}")))` している）
    #[test]
    fn stpp_box_missing_namespace_null_terminator() {
        // namespace の後の null 終端バイトを削って組み立てる。
        // 手作業でバイト列を組み立てる（build_valid_stpp_bytes は必ず null を付けるため）
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(b"http://www.w3.org/ns/ttml");
        // ここで null 終端を意図的に省略する（残りバッファに 0 バイトを含めない）

        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"stpp");
        bytes.extend(payload);

        let err = StppBox::decode(&bytes).unwrap_err();
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("stpp.namespace"),
            "エラーメッセージに stpp.namespace が含まれること: {err}"
        );
    }

    /// schema_location の null 終端が無いと invalid_input エラーになる
    #[test]
    fn stpp_box_missing_schema_location_null_terminator() {
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(b"http://example/");
        payload.push(0); // namespace の null 終端
        payload.extend_from_slice(b"https://example/schema.xsd");
        // ここで schema_location の null 終端を省略する

        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"stpp");
        bytes.extend(payload);

        let err = StppBox::decode(&bytes).unwrap_err();
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("stpp.schema_location"),
            "エラーメッセージに stpp.schema_location が含まれること: {err}"
        );
    }

    /// auxiliary_mime_types の null 終端が無いと invalid_input エラーになる
    #[test]
    fn stpp_box_missing_auxiliary_mime_types_null_terminator() {
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(b"http://example/");
        payload.push(0);
        payload.push(0); // 空の schema_location
        payload.extend_from_slice(b"application/mp4");
        // ここで auxiliary_mime_types の null 終端を省略する

        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"stpp");
        bytes.extend(payload);

        let err = StppBox::decode(&bytes).unwrap_err();
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("stpp.auxiliary_mime_types"),
            "エラーメッセージに stpp.auxiliary_mime_types が含まれること: {err}"
        );
    }

    /// namespace に UTF-8 として不正なバイト列が入っているとエラーになる
    ///
    /// `Utf8String::decode` は UTF-8 不正時にも invalid_input を返す。
    /// エラーメッセージに "stpp.namespace" が含まれる
    #[test]
    fn stpp_box_invalid_utf8_in_namespace() {
        // 0xff は UTF-8 として無効なバイト
        let bytes = build_valid_stpp_bytes(&[0xff, 0xfe], b"", b"");

        let err = StppBox::decode(&bytes).unwrap_err();
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("stpp.namespace"),
            "エラーメッセージに stpp.namespace が含まれること: {err}"
        );
    }

    /// StppBox::decode に stpp 以外の box_type を持つバイト列を渡すとエラーになる
    #[test]
    fn stpp_box_decode_wrong_box_type() {
        // box_type だけ "wvtt" に書き換えたバイト列
        let mut bytes = build_valid_stpp_bytes(b"http://www.w3.org/ns/ttml", b"", b"");
        bytes[4..8].copy_from_slice(b"wvtt");

        let result = StppBox::decode(&bytes);
        assert!(result.is_err(), "stpp 以外の box_type ではエラーになること");
    }

    /// SampleEntry::decode で stpp box_type を持つ入力が Stpp バリアントとして取り出されることを検証する
    ///
    /// 型付き Stpp バリアント追加前は `SampleEntry::Unknown` にフォールバックしていたため、
    /// dispatch の回帰確認として置く
    #[test]
    fn sample_entry_decode_stpp_dispatches_to_stpp_variant() {
        let bytes = build_valid_stpp_bytes(b"http://www.w3.org/ns/ttml", b"", b"");
        let (decoded, _) = SampleEntry::decode(&bytes).unwrap();
        assert!(
            matches!(decoded, SampleEntry::Stpp(_)),
            "stpp box_type は SampleEntry::Stpp として取り出せること"
        );
    }

    // ===== Wvtt Sample Entry Box のテスト =====

    /// SampleEntry::Wvtt のメソッドおよび分類のテスト
    ///
    /// 字幕トラックなので audio_*・video_resolution はいずれも None を返し、
    /// is_unknown_box は false（型付きの Wvtt バリアントとして識別される）
    #[test]
    fn sample_entry_wvtt_methods() {
        let entry = SampleEntry::Wvtt(create_wvtt_box());

        assert_eq!(entry.audio_channel_count(), None);
        assert_eq!(entry.audio_sample_rate(), None);
        assert_eq!(entry.audio_sample_size(), None);
        assert_eq!(entry.video_resolution(), None);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.box_type(), WvttBox::TYPE);
    }

    /// SampleEntry::Wvtt の encode/decode ラウンドトリップ
    ///
    /// wvtt サンプルエントリーが型付きで decode されて Wvtt バリアントに復元されることを検証する
    #[test]
    fn sample_entry_wvtt_encode_decode_roundtrip() {
        let entry = SampleEntry::Wvtt(create_wvtt_box());

        let encoded = entry.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) =
            SampleEntry::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");

        assert_eq!(size, encoded.len());
        // vttc_box の config フィールドが正しく復元されていることを確認する
        let SampleEntry::Wvtt(decoded_wvtt) = decoded else {
            unreachable!();
        };
        assert_eq!(decoded_wvtt.vttc_box.config, "WEBVTT");
    }

    /// 有効な wvtt box のバイト列を組み立てるヘルパー
    ///
    /// SampleEntry ヘッダー（8 バイト）と必須子 vttC ボックスで構成する。
    /// エラーテストで一部フィールドを差し替える起点として使う
    fn build_valid_wvtt_bytes(config: &[u8]) -> Vec<u8> {
        // vttC 子ボックス: BoxHeader 8 バイト + config バイト列
        let vttc_size = 8 + config.len() as u32;
        let mut vttc = Vec::with_capacity(vttc_size as usize);
        vttc.extend_from_slice(&vttc_size.to_be_bytes());
        vttc.extend_from_slice(b"vttC");
        vttc.extend_from_slice(config);

        // wvtt payload: 6 bytes reserved + data_reference_index (u16) + vttC 子ボックス
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(&vttc);

        // BoxHeader: size (4B) + type (4B, "wvtt")
        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"wvtt");
        bytes.extend(payload);
        bytes
    }

    /// 有効なバイト列で組み立てて decode できることを念のため確認する
    #[test]
    fn wvtt_box_decode_valid_bytes() {
        let bytes = build_valid_wvtt_bytes(b"WEBVTT");
        let (decoded, _) = WvttBox::decode(&bytes).expect("有効な wvtt バイト列は decode 可能");
        assert_eq!(decoded.vttc_box.config, "WEBVTT");
    }

    /// vttC 子ボックスが無い wvtt payload では必須子欠落エラーになる
    #[test]
    fn wvtt_box_missing_vttc() {
        // vttC 子ボックスを省略した wvtt payload（reserved + data_reference_index のみ）
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());

        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"wvtt");
        bytes.extend(payload);

        let err = WvttBox::decode(&bytes).expect_err("vttC 欠落で decode がエラーを返すはず");
        assert!(
            err.to_string().contains("vttC"),
            "エラーメッセージに vttC の欠落が示されること: {err}"
        );
    }

    /// WvttBox::decode に wvtt 以外の box_type を持つバイト列を渡すとエラーになる
    #[test]
    fn wvtt_box_decode_wrong_box_type() {
        // box_type だけ "stpp" に書き換えたバイト列
        let mut bytes = build_valid_wvtt_bytes(b"WEBVTT");
        bytes[4..8].copy_from_slice(b"stpp");

        let result = WvttBox::decode(&bytes);
        assert!(result.is_err(), "wvtt 以外の box_type ではエラーになること");
    }

    /// vttC の payload に UTF-8 として不正なバイト列が入っているとエラーになる
    ///
    /// エラーメッセージには "vttC.config" が含まれる（`VttCBox::decode` 内で
    /// `.map_err(|e| Error::invalid_input(format!("vttC.config: {e}")))` している）。
    /// なお `{e}` の詳細は `FromUtf8Error` の Display 由来で Stpp（`Utf8String::decode`
    /// 由来）と異なる文字列になるため、接頭辞 `"vttC.config"` のみを `contains` で照合する
    #[test]
    fn vttc_box_invalid_utf8_config() {
        // 0xff は UTF-8 として無効なバイト
        let bytes = build_valid_wvtt_bytes(&[0xff, 0xfe]);

        let err = WvttBox::decode(&bytes).expect_err("UTF-8 不正で decode がエラーを返すはず");
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("vttC.config"),
            "エラーメッセージに vttC.config が含まれること: {err}"
        );
    }

    /// VttCBox::decode に vttC 以外の box_type を持つバイト列を渡すとエラーになる
    #[test]
    fn vttc_box_decode_wrong_box_type() {
        // vttC 単体のバイト列を組み立て、box_type を "abcd" に書き換える
        let config = b"WEBVTT";
        let box_size = 8 + config.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"abcd");
        bytes.extend_from_slice(config);

        let result = VttCBox::decode(&bytes);
        assert!(result.is_err(), "vttC 以外の box_type ではエラーになること");
    }

    /// SampleEntry::decode で wvtt box_type を持つ入力が Wvtt バリアントとして取り出されることを検証する
    ///
    /// 型付き Wvtt バリアント追加前は `SampleEntry::Unknown` にフォールバックしていたため、
    /// dispatch の回帰確認として置く
    #[test]
    fn sample_entry_decode_wvtt_dispatches_to_wvtt_variant() {
        let bytes = build_valid_wvtt_bytes(b"WEBVTT");
        let (decoded, _) = SampleEntry::decode(&bytes).expect("有効な wvtt バイト列は decode 可能");
        assert!(
            matches!(decoded, SampleEntry::Wvtt(_)),
            "wvtt box_type は SampleEntry::Wvtt として取り出せること"
        );
    }
}
