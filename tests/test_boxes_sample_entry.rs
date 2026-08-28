//! `src/boxes_sample_entry.rs` に定義される SampleEntry 系ボックスの境界値・BaseBox・メソッド単体テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_additional_boxes.rs` / `pbt/tests/prop_codec_boxes.rs`
//! が担う。本ファイルは PBT では安定して狙いにくい境界値（`codec_initialization_data` 長の
//! `u16::MAX` 境界など）と、各 SampleEntry の `BaseBox` 実装・メソッドを固定する。

use std::num::NonZeroU16;

use shiguredo_mp4::{
    Decode, Encode, ErrorKind, FixedPointNumber, Uint,
    boxes::{AudioSampleEntryFields, Av1cBox, AvccBox, HvccBox, VisualSampleEntryFields, VpccBox},
};

/// 指定した `codec_initialization_data` を持つ `VpccBox` を組み立てる
fn make_vpcc(codec_initialization_data: Vec<u8>) -> VpccBox {
    VpccBox {
        profile: 0,
        level: 10,
        bit_depth: Uint::new(8),
        chroma_subsampling: Uint::new(1),
        video_full_range_flag: Uint::new(0),
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        codec_initialization_data,
    }
}

/// `codec_initialization_data.len() == u16::MAX` のとき encode が成功し、
/// roundtrip でデータが一致すること（PBT `arb_vpcc_box` が 0..50 バイトしか生成しないため、
/// 上限値ちょうどを over-reject しないことをここで押さえる。修正前挙動の回帰検出は
/// `..._exceeds_u16_max` が担う）
#[test]
fn vpcc_box_encode_codec_init_data_at_u16_max() {
    let vpcc = make_vpcc(vec![0xAB; usize::from(u16::MAX)]);

    let encoded = vpcc
        .encode_to_vec()
        .expect("u16::MAX バイトの codec_initialization_data は encode 可能であるはず");
    let (decoded, size) =
        VpccBox::decode(&encoded).expect("直前にエンコードした有効な VpccBox は必ずデコードできる");

    assert_eq!(size, encoded.len());
    // codec_initialization_data だけでなく全フィールドが roundtrip で保存されることを確認する
    // （65535 バイト特有のバッファ書き込みで直前のビットパックが壊れる回帰を検出できるように）
    assert_eq!(decoded, vpcc);
}

/// `codec_initialization_data.len() == u16::MAX + 1` のとき encode が
/// `InvalidInput` を返すこと（長さを黙って切り捨てない）
#[test]
fn vpcc_box_encode_codec_init_data_exceeds_u16_max() {
    let vpcc = make_vpcc(vec![0u8; usize::from(u16::MAX) + 1]);

    let err = vpcc
        .encode_to_vec()
        .expect_err("u16::MAX を超える codec_initialization_data は encode エラーであるはず");

    assert_eq!(
        err.kind,
        ErrorKind::InvalidInput,
        "エラー種別が InvalidInput ではない (実際は {:?})",
        err.kind,
    );
    // 現状 encode 側は with_box_type を通らないため box_type は None のはず。
    // encode 側でも box_type 付与するように変えたときにこの assert が落ちて意図的な変更だと気付ける
    assert_eq!(
        err.box_type, None,
        "encode 側は with_box_type を通っていないため box_type は None のはず (実際は {:?})",
        err.box_type,
    );
    // 実装側のエラー文言と密結合させないため、識別に必要な最小限のキーワードだけ確認する
    assert!(
        err.reason.contains("codec_initialization_data"),
        "エラー理由に対象フィールド名が含まれるはず (実際は {:?})",
        err.reason,
    );
}
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
            Av01Box, Avc1Box, BoxRecord, DflaBox, DopsBox, EsdsBox, FlacBox, FlacMetadataBlock,
            FtabBox, Hev1Box, Hvc1Box, Mp4aBox, OpusBox, SampleEntry, StppBox, StyleRecord,
            Tx3gBox, UnknownBox, Vp08Box, Vp09Box, VttCBox, WvttBox,
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

    /// SampleEntry::Tx3g の inner_box() テスト
    ///
    /// 必須の型付き子ボックス `ftab_box` を 1 個持つため、`children().count()` は 1
    #[test]
    fn sample_entry_tx3g_inner_box() {
        let entry = SampleEntry::Tx3g(Tx3gBox {
            data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
            display_flags: 0,
            horizontal_justification: 0,
            vertical_justification: 0,
            background_color_rgba: [0, 0, 0, 0],
            default_text_box: BoxRecord::default(),
            default_style: StyleRecord::default(),
            ftab_box: FtabBox::default(),
            unknown_boxes: vec![],
        });

        assert_eq!(entry.box_type(), Tx3gBox::TYPE);
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
        let size = hvc1.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Hvc1(_)));
    }

    /// SampleEntry::decode で Vp08Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_vp08() {
        let vp08 = create_vp08_box();
        let mut buf = vec![0u8; 4096];
        let size = vp08.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Vp08(_)));
    }

    /// SampleEntry::decode で Vp09Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_vp09() {
        let vp09 = create_vp09_box();
        let mut buf = vec![0u8; 4096];
        let size = vp09.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Vp09(_)));
    }

    /// SampleEntry::decode で Av01Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_av01() {
        let av01 = create_av01_box();
        let mut buf = vec![0u8; 4096];
        let size = av01.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Av01(_)));
    }

    /// SampleEntry::decode で Mp4aBox を直接デコードするテスト
    #[test]
    fn sample_entry_decode_mp4a() {
        let mp4a = create_mp4a_box();
        let mut buf = vec![0u8; 4096];
        let size = mp4a.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Mp4a(_)));
    }

    /// SampleEntry::decode で FlacBox を直接デコードするテスト
    #[test]
    fn sample_entry_decode_flac() {
        let flac = create_flac_box();
        let mut buf = vec![0u8; 4096];
        let size = flac.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Flac(_)));
    }

    /// SampleEntry::decode で Hev1Box を直接デコードするテスト
    #[test]
    fn sample_entry_decode_hev1() {
        let hev1 = create_hev1_box();
        let mut buf = vec![0u8; 4096];
        let size = hev1.encode(&mut buf).expect("エンコードは失敗しない");
        let (decoded, decoded_size) =
            SampleEntry::decode(&buf[..size]).expect("デコードは失敗しない");
        assert_eq!(size, decoded_size);
        assert!(matches!(decoded, SampleEntry::Hev1(_)));
    }
}

// ===== SampleEntry のメソッドテスト =====

mod sample_entry_tests {
    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Decode, Encode, Uint, Utf8String,
        boxes::{
            Av01Box, Av1cBox, Avc1Box, AvccBox, BoxRecord, DopsBox, EsdsBox, FlacBox,
            FlacMetadataBlock, FontRecord, FtabBox, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox,
            SampleEntry, StppBox, StyleRecord, Tx3gBox, UnknownBox, Vp08Box, Vp09Box, VpccBox,
            VttCBox, WvttBox,
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

        let encoded = entry.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SampleEntry::decode(&encoded)
            .expect("直前にエンコードした有効な SampleEntry は必ずデコードできる");

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

        let encoded = entry.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SampleEntry::decode(&encoded)
            .expect("直前にエンコードした有効な SampleEntry は必ずデコードできる");

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
        let (decoded, _) = StppBox::decode(&bytes)
            .expect("ヘルパーが組み立てる有効な stpp バイト列はデコードできる");
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
        let (decoded, _) = SampleEntry::decode(&bytes)
            .expect("ヘルパーが組み立てる有効な stpp バイト列はデコードできる");
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

    // ===== Tx3g Sample Entry Box のテスト =====

    /// テスト用の Tx3gBox を生成する
    ///
    /// 最小構成（ftab は空、本体は全 0）
    fn create_tx3g_box() -> Tx3gBox {
        Tx3gBox {
            data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
            display_flags: 0,
            horizontal_justification: 0,
            vertical_justification: 0,
            background_color_rgba: [0, 0, 0, 0],
            default_text_box: BoxRecord::default(),
            default_style: StyleRecord::default(),
            ftab_box: FtabBox::default(),
            unknown_boxes: vec![],
        }
    }

    /// SampleEntry::Tx3g のメソッドおよび分類のテスト
    ///
    /// 字幕トラックなので audio_*・video_resolution はいずれも None を返し、
    /// is_unknown_box は false（型付きの Tx3g バリアントとして識別される）
    #[test]
    fn sample_entry_tx3g_methods() {
        let entry = SampleEntry::Tx3g(create_tx3g_box());

        assert_eq!(entry.audio_channel_count(), None);
        assert_eq!(entry.audio_sample_rate(), None);
        assert_eq!(entry.audio_sample_size(), None);
        assert_eq!(entry.video_resolution(), None);
        assert!(!entry.is_unknown_box());
        assert_eq!(entry.box_type(), Tx3gBox::TYPE);
    }

    /// SampleEntry::Tx3g の encode/decode ラウンドトリップ
    ///
    /// tx3g サンプルエントリーが型付きで decode されて Tx3g バリアントに復元されることを検証する
    #[test]
    fn sample_entry_tx3g_encode_decode_roundtrip() {
        let entry = SampleEntry::Tx3g(create_tx3g_box());

        let encoded = entry.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) =
            SampleEntry::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");

        assert_eq!(size, encoded.len());
        let SampleEntry::Tx3g(decoded_tx3g) = decoded else {
            unreachable!();
        };
        assert_eq!(decoded_tx3g.display_flags, 0);
        assert!(decoded_tx3g.ftab_box.entries.is_empty());
    }

    /// 有効な tx3g box のバイト列を組み立てるヘルパー
    ///
    /// SampleEntry ヘッダー（8 バイト）と本体固定 30 バイト、必須子 ftab で構成する。
    /// エラーテストで一部フィールドを差し替える起点として使う。
    /// `ftab_entries` は `(font_id, font_name)` のスライス
    fn build_valid_tx3g_bytes(ftab_entries: &[(u16, &[u8])]) -> Vec<u8> {
        // ftab 子ボックス: BoxHeader 8 バイト + entry_count u16 + FontRecord 群
        let mut ftab_body = Vec::new();
        let entry_count = u16::try_from(ftab_entries.len()).expect("エントリー数は u16 に収まる");
        ftab_body.extend_from_slice(&entry_count.to_be_bytes());
        for (font_id, font_name) in ftab_entries {
            let font_name_length =
                u8::try_from(font_name.len()).expect("font_name は 255 バイト以下");
            ftab_body.extend_from_slice(&font_id.to_be_bytes());
            ftab_body.push(font_name_length);
            ftab_body.extend_from_slice(font_name);
        }
        let ftab_size = 8 + ftab_body.len() as u32;
        let mut ftab = Vec::with_capacity(ftab_size as usize);
        ftab.extend_from_slice(&ftab_size.to_be_bytes());
        ftab.extend_from_slice(b"ftab");
        ftab.extend_from_slice(&ftab_body);

        // tx3g payload: reserved(6) + data_reference_index(u16) + 本体 30 バイト + ftab
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
        payload.extend_from_slice(&0u32.to_be_bytes()); // display_flags
        payload.push(0); // horizontal_justification
        payload.push(0); // vertical_justification
        payload.extend_from_slice(&[0u8; 4]); // background_color_rgba
        payload.extend_from_slice(&[0u8; 8]); // BoxRecord (top / left / bottom / right)
        payload.extend_from_slice(&[0u8; 12]); // StyleRecord
        payload.extend_from_slice(&ftab);

        // BoxHeader: size (4B) + type (4B, "tx3g")
        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"tx3g");
        bytes.extend(payload);
        bytes
    }

    /// ftab 子ボックスが無い tx3g payload では必須子欠落エラーになる
    #[test]
    fn tx3g_box_missing_ftab() {
        // ftab 子ボックスを省略した tx3g payload（本体固定 30 バイトのみ）
        let mut payload = vec![0u8; 6];
        payload.extend_from_slice(&1u16.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.push(0);
        payload.push(0);
        payload.extend_from_slice(&[0u8; 4]);
        payload.extend_from_slice(&[0u8; 8]);
        payload.extend_from_slice(&[0u8; 12]);

        let box_size = 8 + payload.len() as u32;
        let mut bytes = Vec::with_capacity(box_size as usize);
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"tx3g");
        bytes.extend(payload);

        let err = Tx3gBox::decode(&bytes).expect_err("ftab 欠落で decode がエラーを返すはず");
        assert!(
            err.to_string().contains("ftab"),
            "エラーメッセージに ftab の欠落が示されること: {err}"
        );
    }

    /// Tx3gBox::decode に tx3g 以外の box_type を持つバイト列を渡すとエラーになる
    #[test]
    fn tx3g_box_decode_wrong_box_type() {
        // box_type だけ "wvtt" に書き換えたバイト列
        let mut bytes = build_valid_tx3g_bytes(&[(1, b"Serif")]);
        bytes[4..8].copy_from_slice(b"wvtt");

        let result = Tx3gBox::decode(&bytes);
        assert!(result.is_err(), "tx3g 以外の box_type ではエラーになること");
    }

    /// FtabBox::decode に ftab 以外の box_type を持つバイト列を渡すとエラーになる
    #[test]
    fn ftab_box_decode_wrong_box_type() {
        // ftab 単体のバイト列を組み立て、box_type を "abcd" に書き換える
        let mut bytes: Vec<u8> = Vec::new();
        let box_size = 10u32;
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"abcd");
        bytes.extend_from_slice(&0u16.to_be_bytes()); // entry_count = 0

        let result = FtabBox::decode(&bytes);
        assert!(result.is_err(), "ftab 以外の box_type ではエラーになること");
    }

    /// entry_count = 0 の ftab がラウンドトリップできることを決定的に担保する
    ///
    /// `minf_box_subtitle_nmhd_roundtrip` の tx3g typed 化後に依存する invariant
    /// （`ftab_box: FtabBox { entries: vec![] }` で最小構成が成立する）を明示テストする
    #[test]
    fn ftab_box_decode_entry_count_zero_roundtrip() {
        // BoxHeader (size=10, type=b"ftab") + entry_count=0 (u16 BE) の 10 バイト
        let mut bytes: Vec<u8> = Vec::new();
        let box_size = 10u32;
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"ftab");
        bytes.extend_from_slice(&0u16.to_be_bytes());

        let (decoded, size) = FtabBox::decode(&bytes).expect("空 ftab は decode 可能");
        assert_eq!(size, bytes.len());
        assert!(decoded.entries.is_empty());

        // encode すると同じバイト列に戻る
        let reencoded = decoded.encode_to_vec().expect("空 ftab は encode 可能");
        assert_eq!(reencoded, bytes);
    }

    /// FontRecord::encode で font_name が 255 バイトを超える場合にエラーになる
    #[test]
    fn font_record_encode_too_long_name() {
        // 256 バイトの font_name（u8::MAX を超える境界値）
        let record = FontRecord {
            font_id: 1,
            font_name: vec![b'A'; 256],
        };
        // 十分な大きさのバッファを用意して encode を試みる
        let mut buf = vec![0u8; 512];
        let result = record.encode(&mut buf);
        let err = result.expect_err("font_name > 255 で encode がエラーを返すはず");
        assert!(
            err.to_string().contains("font_name_length"),
            "エラーメッセージに font_name_length が示されること: {err}"
        );
    }

    /// FontRecord::decode で `font_name_length` が残りバイト数を超える場合に境界チェックでエラーになる
    ///
    /// 悪意ある入力（`FtabBox::entry_count` を過大に指定して各 FontRecord の残バイトを
    /// 使い切ろうとするケース等）に対する防御コードの回帰確認
    #[test]
    fn font_record_decode_length_exceeds_buffer() {
        // font_id (2B) + font_name_length = 0xFF (1B) だけを渡す。
        // font_name として 255 バイトを読み込もうとするが残バイトは 0 なのでエラー
        let bytes = [0x00, 0x01, 0xff];
        let err = FontRecord::decode(&bytes)
            .expect_err("font_name_length が残バイト超過で decode がエラーを返すはず");
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InsufficientBuffer);
    }

    /// FtabBox::decode で `entry_count = u16::MAX` かつ payload 不足の場合、
    /// 過大な事前アロケーションを起こさずに早期エラーで抜ける
    ///
    /// `Vec::with_capacity(entry_count)` を使わず `Vec::new()` から push で伸ばす
    /// 防御的実装の回帰確認（リファクタリングで `with_capacity` に戻すと DoS/OOM リスクが発生する）
    #[test]
    fn ftab_box_decode_entry_count_overflow_returns_error() {
        // BoxHeader (size=10, type=b"ftab") + entry_count = u16::MAX (2B) だけを渡す。
        // FontRecord が 1 個も後続しないため、最初の FontRecord::decode_at で早期エラー
        let box_size: u32 = 10;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&box_size.to_be_bytes());
        bytes.extend_from_slice(b"ftab");
        bytes.extend_from_slice(&u16::MAX.to_be_bytes());
        let err = FtabBox::decode(&bytes)
            .expect_err("entry_count = u16::MAX + payload 不足で decode がエラーを返すはず");
        assert_eq!(err.kind, shiguredo_mp4::ErrorKind::InsufficientBuffer);
    }

    /// Tx3gBox::decode で SampleEntry base (8B) + 本体固定 30B が満たされない payload は
    /// `check_buffer_size` で早期エラーになる
    ///
    /// SampleEntry ヘッダー 8 バイトと本体 30 バイトの合計 38 バイトが必要だが、
    /// 途中のバイト境界 (0 / 6 / 16 / 30) で切り詰めた payload はすべて decode に失敗する
    #[test]
    fn tx3g_box_decode_truncated_body_returns_error() {
        for payload_len in [0usize, 6, 16, 30] {
            // BoxHeader (size = 8 + payload_len, type = b"tx3g") + payload_len バイトの 0
            let box_size = (8 + payload_len) as u32;
            let mut bytes = Vec::with_capacity(box_size as usize);
            bytes.extend_from_slice(&box_size.to_be_bytes());
            bytes.extend_from_slice(b"tx3g");
            bytes.extend(std::iter::repeat_n(0u8, payload_len));

            let result = Tx3gBox::decode(&bytes);
            assert!(
                result.is_err(),
                "payload_len = {payload_len} で decode がエラーを返すこと"
            );
        }
    }

    /// SampleEntry::decode で tx3g box_type を持つ入力が Tx3g バリアントとして取り出されることを検証する
    ///
    /// 型付き Tx3g バリアント追加前は `SampleEntry::Unknown` にフォールバックしていたため、
    /// ディスパッチの回帰確認として置く
    #[test]
    fn sample_entry_decode_tx3g_dispatches_to_tx3g_variant() {
        let bytes = build_valid_tx3g_bytes(&[(1, b"Serif")]);
        let (decoded, _) = SampleEntry::decode(&bytes).expect("有効な tx3g バイト列は decode 可能");
        let SampleEntry::Tx3g(tx3g) = decoded else {
            panic!("tx3g box_type は SampleEntry::Tx3g として取り出せること");
        };
        assert_eq!(tx3g.ftab_box.entries.len(), 1);
        assert_eq!(tx3g.ftab_box.entries[0].font_id, 1);
        assert_eq!(tx3g.ftab_box.entries[0].font_name, b"Serif");
    }
}

// ===== pbt/tests/prop_basic_types.rs の codec_box_boundary_tests から移動 =====

/// コーデックボックスの境界値テスト (feature/fix-infinite-loop で修正された問題)
mod codec_box_boundary_tests {
    use shiguredo_mp4::{Decode, boxes::HvccBox, boxes::VpccBox};

    /// HvccBox: NAL unit length がペイロード境界を超える場合のテスト
    ///
    /// 修正前: panic (slice index out of bounds)
    /// 修正後: Error を返す
    #[test]
    fn hvcc_box_nal_unit_length_exceeds_payload() {
        // 最小限の有効な HvccBox ヘッダー + 不正な NAL unit length
        let mut buf = Vec::new();

        // BoxHeader: size=0 (可変長), type="hvcC"
        buf.extend_from_slice(&0u32.to_be_bytes()); // size = 0 (variable)
        buf.extend_from_slice(b"hvcC");

        // configuration_version = 1
        buf.push(1);
        // general_profile_space(2) | general_tier_flag(1) | general_profile_idc(5) = 0
        buf.push(0);
        // general_profile_compatibility_flags (4 bytes)
        buf.extend_from_slice(&[0u8; 4]);
        // general_constraint_indicator_flags (6 bytes)
        buf.extend_from_slice(&[0u8; 6]);
        // general_level_idc
        buf.push(0);
        // reserved(4) | min_spatial_segmentation_idc(12) (2 bytes)
        buf.extend_from_slice(&[0xF0, 0x00]);
        // reserved(6) | parallelism_type(2)
        buf.push(0xFC);
        // reserved(6) | chroma_format_idc(2)
        buf.push(0xFC);
        // reserved(5) | bit_depth_luma_minus8(3)
        buf.push(0xF8);
        // reserved(5) | bit_depth_chroma_minus8(3)
        buf.push(0xF8);
        // avg_frame_rate (2 bytes)
        buf.extend_from_slice(&[0, 0]);
        // constant_frame_rate(2) | num_temporal_layers(3) | temporal_id_nested(1) | length_size_minus_one(2)
        buf.push(0);
        // num_of_arrays = 1 (1つの NALU 配列)
        buf.push(1);

        // NALU array
        // array_completeness(1) | reserved(1) | nal_unit_type(6)
        buf.push(0);
        // num_nalus = 1
        buf.extend_from_slice(&1u16.to_be_bytes());
        // nal_unit_length = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の NAL unit データは 0 バイト (境界を超えている)

        let result = HvccBox::decode(&buf);
        // 修正後はエラーを返すはず (panic しない)
        assert!(
            result.is_err(),
            "HvccBox は NAL unit 長が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }

    /// HvccBox: NAL unit length がペイロード境界を超える場合
    ///
    /// ボックスサイズを固定して、ペイロードが正確に計算されるケース
    /// NAL unit length が不正な場合は Error を返す
    #[test]
    fn hvcc_box_nal_unit_length_exceeds_payload_with_fixed_size() {
        // ボックスサイズを固定して、ペイロードが正確に計算されるようにする
        let mut buf = Vec::new();

        // configuration_version = 1
        buf.push(1);
        // general_profile_space(2) | general_tier_flag(1) | general_profile_idc(5) = 0
        buf.push(0);
        // general_profile_compatibility_flags (4 bytes)
        buf.extend_from_slice(&[0u8; 4]);
        // general_constraint_indicator_flags (6 bytes)
        buf.extend_from_slice(&[0u8; 6]);
        // general_level_idc
        buf.push(0);
        // reserved(4) | min_spatial_segmentation_idc(12) (2 bytes)
        buf.extend_from_slice(&[0xF0, 0x00]);
        // reserved(6) | parallelism_type(2)
        buf.push(0xFC);
        // reserved(6) | chroma_format_idc(2)
        buf.push(0xFC);
        // reserved(5) | bit_depth_luma_minus8(3)
        buf.push(0xF8);
        // reserved(5) | bit_depth_chroma_minus8(3)
        buf.push(0xF8);
        // avg_frame_rate (2 bytes)
        buf.extend_from_slice(&[0, 0]);
        // constant_frame_rate(2) | num_temporal_layers(3) | temporal_id_nested(1) | length_size_minus_one(2)
        buf.push(0);
        // num_of_arrays = 1 (1つの NALU 配列)
        buf.push(1);

        // NALU array
        // array_completeness(1) | reserved(1) | nal_unit_type(6)
        buf.push(0);
        // num_nalus = 1
        buf.extend_from_slice(&1u16.to_be_bytes());
        // nal_unit_length = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の NAL unit データは 0 バイト (境界を超えている)

        // ペイロードサイズを計算
        let payload_size = buf.len();

        // BoxHeader を先頭に付加
        let mut full_buf = Vec::new();
        let box_size = (8 + payload_size) as u32; // 8 = BoxHeader サイズ
        full_buf.extend_from_slice(&box_size.to_be_bytes());
        full_buf.extend_from_slice(b"hvcC");
        full_buf.extend_from_slice(&buf);

        let result = HvccBox::decode(&full_buf);
        // 修正前は panic、修正後はエラーを返すはず
        assert!(
            result.is_err(),
            "HvccBox は NAL unit 長が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }

    /// VpccBox: codec_init_size がペイロード境界を超える場合のテスト
    ///
    /// 修正前: panic (slice index out of bounds)
    /// 修正後: Error を返す
    #[test]
    fn vpcc_box_codec_init_size_exceeds_payload() {
        // 最小限の有効な VpccBox ヘッダー + 不正な codec_init_size
        let mut buf = Vec::new();

        // BoxHeader: size=0 (可変長), type="vpcC"
        buf.extend_from_slice(&0u32.to_be_bytes()); // size = 0 (variable)
        buf.extend_from_slice(b"vpcC");

        // FullBoxHeader: version=1, flags=0
        buf.push(1); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags

        // profile
        buf.push(0);
        // level
        buf.push(0);
        // bit_depth(4) | chroma_subsampling(3) | video_full_range_flag(1)
        buf.push(0);
        // colour_primaries
        buf.push(0);
        // transfer_characteristics
        buf.push(0);
        // matrix_coefficients
        buf.push(0);
        // codec_init_size = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の codec_initialization_data は 0 バイト (境界を超えている)

        let result = VpccBox::decode(&buf);
        // 修正後はエラーを返すはず (panic しない)
        assert!(
            result.is_err(),
            "VpccBox は codec_init_size が payload を超える場合にエラーを返す"
        );
    }

    /// VpccBox: codec_init_size がペイロード境界を超える場合
    ///
    /// ボックスサイズを固定して、ペイロードが正確に計算されるケース
    /// codec_init_size が不正な場合は Error を返す
    #[test]
    fn vpcc_box_codec_init_size_exceeds_payload_with_fixed_size() {
        // ボックスサイズを固定して、ペイロードが正確に計算されるようにする
        let mut buf = Vec::new();

        // FullBoxHeader: version=1, flags=0
        buf.push(1); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags

        // profile
        buf.push(0);
        // level
        buf.push(0);
        // bit_depth(4) | chroma_subsampling(3) | video_full_range_flag(1)
        buf.push(0);
        // colour_primaries
        buf.push(0);
        // transfer_characteristics
        buf.push(0);
        // matrix_coefficients
        buf.push(0);
        // codec_init_size = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の codec_initialization_data は 0 バイト (境界を超えている)

        // ペイロードサイズを計算
        let payload_size = buf.len();

        // BoxHeader を先頭に付加
        let mut full_buf = Vec::new();
        let box_size = (8 + payload_size) as u32; // 8 = BoxHeader サイズ
        full_buf.extend_from_slice(&box_size.to_be_bytes());
        full_buf.extend_from_slice(b"vpcC");
        full_buf.extend_from_slice(&buf);

        let result = VpccBox::decode(&full_buf);
        // 修正前は panic、修正後はエラーを返すはず
        assert!(
            result.is_err(),
            "VpccBox は codec_init_size が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }
}

// ===== pbt/tests/prop_additional_boxes.rs の boundary_tests (SampleEntry) から移動 =====

mod additional_sample_entry_boundary_tests {
    use std::num::NonZeroU16;

    use shiguredo_mp4::{
        Decode, Encode, FixedPointNumber, Uint,
        boxes::{
            AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, EsdsBox, Mp4aBox, OpusBox,
            VisualSampleEntryFields,
        },
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

    /// OpusBox: 最小構成
    #[test]
    fn opus_box_minimal() {
        let opus = OpusBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1).expect("1 は非ゼロ"),
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
        };
        let encoded = opus.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = OpusBox::decode(&encoded)
            .expect("直前にエンコードした有効な OpusBox は必ずデコードできる");
        assert_eq!(decoded.audio.channelcount, 2);
        assert_eq!(decoded.dops_box.output_channel_count, 2);
    }

    /// Mp4aBox: AAC-LC 設定
    #[test]
    fn mp4a_box_aac_lc() {
        let mp4a = Mp4aBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1).expect("1 は非ゼロ"),
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(48000, 0),
            },
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
                        dec_specific_info: Some(DecoderSpecificInfo {
                            payload: vec![0x11, 0x90],
                        }),
                    },
                    sl_config_descr: SlConfigDescriptor,
                },
            },
            unknown_boxes: vec![],
        };
        let encoded = mp4a.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Mp4aBox::decode(&encoded)
            .expect("直前にエンコードした有効な Mp4aBox は必ずデコードできる");
        assert_eq!(
            decoded.esds_box.es.dec_config_descr.object_type_indication,
            0x40
        );
    }

    /// Avc1Box: 1080p H.264 Baseline Profile
    #[test]
    fn avc1_box_1080p() {
        let avc1 = Avc1Box {
            visual: VisualSampleEntryFields {
                data_reference_index: NonZeroU16::new(1).expect("1 は非ゼロ"),
                width: 1920,
                height: 1080,
                horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
                vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
                frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
                compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
                depth: VisualSampleEntryFields::DEFAULT_DEPTH,
            },
            avcc_box: AvccBox {
                avc_profile_indication: 66, // Baseline Profile
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
        };
        let encoded = avc1.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Avc1Box::decode(&encoded)
            .expect("直前にエンコードした有効な Avc1Box は必ずデコードできる");
        assert_eq!(decoded.visual.width, 1920);
        assert_eq!(decoded.visual.height, 1080);
    }
}

// ===== pbt/tests/prop_codec_boxes.rs の単体テストから移動 =====

// ===== 境界値テスト =====

mod codec_boxes_boundary_tests {
    use shiguredo_mp4::{
        Decode, Encode, Uint,
        boxes::{Av1cBox, AvccBox, DopsBox, EsdsBox, HvccBox, VpccBox},
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

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
    use shiguredo_mp4::{Decode, Encode, Uint, boxes::AvccBox};

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
    use shiguredo_mp4::{
        Decode, Encode, Uint,
        boxes::{HvccBox, HvccNalUintArray},
    };

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
    use shiguredo_mp4::{Decode, boxes::DflaBox};

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
    use shiguredo_mp4::{Decode, boxes::DopsBox};

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
    use shiguredo_mp4::{Decode, boxes::EsdsBox};

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
