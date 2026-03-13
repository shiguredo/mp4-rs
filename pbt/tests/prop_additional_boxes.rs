//! 追加の Box 構造体の Property-Based Testing
//!
//! proptest_boxes.rs と proptest_codec_boxes.rs でカバーされていない Box のテスト

use std::num::NonZeroU16;

use proptest::prelude::*;
use shiguredo_mp4::{
    BoxSize, BoxType, Decode, Encode, FixedPointNumber, Uint,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, DflaBox, DopsBox, EsdsBox,
        FlacBox, FlacMetadataBlock, FreeBox, Hev1Box, Hvc1Box, HvccBox, MdatBox, Mp4aBox, OpusBox,
        UnknownBox, VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox,
    },
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

// ===== Strategy 定義 =====

/// AudioSampleEntryFields を生成する Strategy
fn arb_audio_sample_entry() -> impl Strategy<Value = AudioSampleEntryFields> {
    (
        1u16..=u16::MAX, // data_reference_index
        1u16..=8u16,     // channelcount
        any::<u16>(),    // samplesize
        any::<u16>(),    // samplerate integer
        any::<u16>(),    // samplerate fraction
    )
        .prop_map(
            |(dri, channelcount, samplesize, sr_int, sr_frac)| AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(dri).unwrap(),
                channelcount,
                samplesize,
                samplerate: FixedPointNumber::new(sr_int, sr_frac),
            },
        )
}

/// VisualSampleEntryFields を生成する Strategy
fn arb_visual_sample_entry() -> impl Strategy<Value = VisualSampleEntryFields> {
    (
        1u16..=u16::MAX,   // data_reference_index
        1u16..=4096u16,    // width
        1u16..=4096u16,    // height
        any::<u16>(),      // horizresolution int
        any::<u16>(),      // horizresolution frac
        any::<u16>(),      // vertresolution int
        any::<u16>(),      // vertresolution frac
        any::<u16>(),      // frame_count
        any::<[u8; 32]>(), // compressorname
        any::<u16>(),      // depth
    )
        .prop_map(
            |(
                dri,
                width,
                height,
                hr_int,
                hr_frac,
                vr_int,
                vr_frac,
                frame_count,
                compressorname,
                depth,
            )| {
                VisualSampleEntryFields {
                    data_reference_index: NonZeroU16::new(dri).unwrap(),
                    width,
                    height,
                    horizresolution: FixedPointNumber::new(hr_int, hr_frac),
                    vertresolution: FixedPointNumber::new(vr_int, vr_frac),
                    frame_count,
                    compressorname,
                    depth,
                }
            },
        )
}

/// DopsBox を生成する Strategy
fn arb_dops_box() -> impl Strategy<Value = DopsBox> {
    (1u8..=8, any::<u16>(), any::<u32>(), any::<i16>()).prop_map(
        |(output_channel_count, pre_skip, input_sample_rate, output_gain)| DopsBox {
            output_channel_count,
            pre_skip,
            input_sample_rate,
            output_gain,
        },
    )
}

/// EsdsBox (AAC) を生成する Strategy
fn arb_esds_box() -> impl Strategy<Value = EsdsBox> {
    (
        1u16..=u16::MAX,
        0u8..32,
        any::<u32>(),
        any::<u32>(),
        prop::option::of(prop::collection::vec(any::<u8>(), 0..20)),
    )
        .prop_map(
            |(es_id, stream_priority, max_bitrate, avg_bitrate, dec_specific_info)| EsdsBox {
                es: EsDescriptor {
                    es_id,
                    stream_priority: Uint::new(stream_priority),
                    depends_on_es_id: None,
                    url_string: None,
                    ocr_es_id: None,
                    dec_config_descr: DecoderConfigDescriptor {
                        object_type_indication: 0x40,
                        stream_type: Uint::new(0x05),
                        up_stream: Uint::new(0),
                        buffer_size_db: Uint::new(0),
                        max_bitrate,
                        avg_bitrate,
                        dec_specific_info: dec_specific_info
                            .map(|payload| DecoderSpecificInfo { payload }),
                    },
                    sl_config_descr: SlConfigDescriptor,
                },
            },
        )
}

/// FlacMetadataBlock (STREAMINFO) を生成する Strategy
fn arb_flac_streaminfo_block() -> impl Strategy<Value = FlacMetadataBlock> {
    // STREAMINFO は 34 バイト固定
    prop::collection::vec(any::<u8>(), 34..=34).prop_map(|block_data| FlacMetadataBlock {
        last_metadata_block_flag: Uint::new(1),
        block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
        block_data,
    })
}

/// DflaBox を生成する Strategy
fn arb_dfla_box() -> impl Strategy<Value = DflaBox> {
    arb_flac_streaminfo_block().prop_map(|streaminfo| DflaBox {
        metadata_blocks: vec![streaminfo],
    })
}

/// AvccBox (Baseline) を生成する Strategy
fn arb_avcc_box() -> impl Strategy<Value = AvccBox> {
    (
        prop_oneof![Just(66u8), Just(77u8), Just(88u8)],
        any::<u8>(),
        any::<u8>(),
        0u8..4,
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..30), 0..3),
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..30), 0..3),
    )
        .prop_map(
            |(profile, compat, level, length_size, sps_list, pps_list)| AvccBox {
                avc_profile_indication: profile,
                profile_compatibility: compat,
                avc_level_indication: level,
                length_size_minus_one: Uint::new(length_size),
                sps_list,
                pps_list,
                chroma_format: None,
                bit_depth_luma_minus8: None,
                bit_depth_chroma_minus8: None,
                sps_ext_list: vec![],
            },
        )
}

/// HvccBox を生成する Strategy
fn arb_hvcc_box() -> impl Strategy<Value = HvccBox> {
    (
        0u8..4,
        any::<bool>(),
        0u8..32,
        any::<u32>(),
        any::<u8>(),
        0u8..4,
    )
        .prop_map(
            |(profile_space, tier_flag, profile_idc, compat_flags, level_idc, length_size)| {
                HvccBox {
                    general_profile_space: Uint::new(profile_space),
                    general_tier_flag: Uint::new(tier_flag as u8),
                    general_profile_idc: Uint::new(profile_idc),
                    general_profile_compatibility_flags: compat_flags,
                    general_constraint_indicator_flags: Uint::new(0),
                    general_level_idc: level_idc,
                    min_spatial_segmentation_idc: Uint::new(0),
                    parallelism_type: Uint::new(0),
                    chroma_format_idc: Uint::new(1),
                    bit_depth_luma_minus8: Uint::new(0),
                    bit_depth_chroma_minus8: Uint::new(0),
                    avg_frame_rate: 0,
                    constant_frame_rate: Uint::new(0),
                    num_temporal_layers: Uint::new(1),
                    temporal_id_nested: Uint::new(1),
                    length_size_minus_one: Uint::new(length_size),
                    nalu_arrays: vec![],
                }
            },
        )
}

/// VpccBox を生成する Strategy
fn arb_vpcc_box() -> impl Strategy<Value = VpccBox> {
    (any::<u8>(), any::<u8>(), 0u8..16, 0u8..8, any::<bool>()).prop_map(
        |(profile, level, bit_depth, chroma_subsampling, full_range)| VpccBox {
            profile,
            level,
            bit_depth: Uint::new(bit_depth),
            chroma_subsampling: Uint::new(chroma_subsampling),
            video_full_range_flag: Uint::new(full_range as u8),
            colour_primaries: 1,
            transfer_characteristics: 1,
            matrix_coefficients: 1,
            codec_initialization_data: vec![],
        },
    )
}

/// Av1cBox を生成する Strategy
fn arb_av1c_box() -> impl Strategy<Value = Av1cBox> {
    (0u8..8, 0u8..32, any::<bool>()).prop_map(|(seq_profile, seq_level_idx_0, seq_tier_0)| {
        Av1cBox {
            seq_profile: Uint::new(seq_profile),
            seq_level_idx_0: Uint::new(seq_level_idx_0),
            seq_tier_0: Uint::new(seq_tier_0 as u8),
            high_bitdepth: Uint::new(0),
            twelve_bit: Uint::new(0),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // ===== 単純な Box のテスト =====

    /// UnknownBox の encode/decode roundtrip
    #[test]
    fn unknown_box_roundtrip(
        box_type in any::<[u8; 4]>(),
        payload in prop::collection::vec(any::<u8>(), 0..100)
    ) {
        let unknown = UnknownBox {
            box_type: BoxType::Normal(box_type),
            box_size: BoxSize::with_payload_size(BoxType::Normal(box_type), payload.len() as u64),
            payload: payload.clone(),
        };
        let encoded = unknown.encode_to_vec().unwrap();
        let (decoded, size) = UnknownBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.payload, payload);
    }

    /// FreeBox の encode/decode roundtrip
    #[test]
    fn free_box_roundtrip(payload in prop::collection::vec(any::<u8>(), 0..100)) {
        let free = FreeBox { payload: payload.clone() };
        let encoded = free.encode_to_vec().unwrap();
        let (decoded, size) = FreeBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.payload, payload);
    }

    /// MdatBox の encode/decode roundtrip
    #[test]
    fn mdat_box_roundtrip(payload in prop::collection::vec(any::<u8>(), 0..100)) {
        let mdat = MdatBox { payload: payload.clone() };
        let encoded = mdat.encode_to_vec().unwrap();
        let (decoded, size) = MdatBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.payload, payload);
    }

    // ===== Audio Sample Entry Box のテスト =====

    /// OpusBox の encode/decode roundtrip
    #[test]
    fn opus_box_roundtrip(
        audio in arb_audio_sample_entry(),
        dops in arb_dops_box()
    ) {
        let opus = OpusBox {
            audio,
            dops_box: dops,
            unknown_boxes: vec![],
        };
        let encoded = opus.encode_to_vec().unwrap();
        let (decoded, size) = OpusBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.audio.channelcount, opus.audio.channelcount);
        prop_assert_eq!(decoded.dops_box.output_channel_count, opus.dops_box.output_channel_count);
    }

    /// Mp4aBox の encode/decode roundtrip
    #[test]
    fn mp4a_box_roundtrip(
        audio in arb_audio_sample_entry(),
        esds in arb_esds_box()
    ) {
        let mp4a = Mp4aBox {
            audio,
            esds_box: esds,
            unknown_boxes: vec![],
        };
        let encoded = mp4a.encode_to_vec().unwrap();
        let (decoded, size) = Mp4aBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.audio.channelcount, mp4a.audio.channelcount);
        prop_assert_eq!(decoded.esds_box.es.es_id, mp4a.esds_box.es.es_id);
    }

    /// FlacBox の encode/decode roundtrip
    #[test]
    fn flac_box_roundtrip(
        audio in arb_audio_sample_entry(),
        dfla in arb_dfla_box()
    ) {
        let flac = FlacBox {
            audio,
            dfla_box: dfla,
            unknown_boxes: vec![],
        };
        let encoded = flac.encode_to_vec().unwrap();
        let (decoded, size) = FlacBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.audio.channelcount, flac.audio.channelcount);
        prop_assert_eq!(decoded.dfla_box.metadata_blocks.len(), 1);
    }

    /// DflaBox の encode/decode roundtrip
    #[test]
    fn dfla_box_roundtrip(dfla in arb_dfla_box()) {
        let encoded = dfla.encode_to_vec().unwrap();
        let (decoded, size) = DflaBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.metadata_blocks.len(), dfla.metadata_blocks.len());
        prop_assert_eq!(decoded.metadata_blocks[0].block_type.get(), 0);
    }

    // ===== Visual Sample Entry Box のテスト =====

    /// Avc1Box の encode/decode roundtrip
    #[test]
    fn avc1_box_roundtrip(
        visual in arb_visual_sample_entry(),
        avcc in arb_avcc_box()
    ) {
        let avc1 = Avc1Box {
            visual,
            avcc_box: avcc,
            unknown_boxes: vec![],
        };
        let encoded = avc1.encode_to_vec().unwrap();
        let (decoded, size) = Avc1Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, avc1.visual.width);
        prop_assert_eq!(decoded.visual.height, avc1.visual.height);
        prop_assert_eq!(decoded.avcc_box.avc_profile_indication, avc1.avcc_box.avc_profile_indication);
    }

    /// Hev1Box の encode/decode roundtrip
    #[test]
    fn hev1_box_roundtrip(
        visual in arb_visual_sample_entry(),
        hvcc in arb_hvcc_box()
    ) {
        let hev1 = Hev1Box {
            visual,
            hvcc_box: hvcc,
            unknown_boxes: vec![],
        };
        let encoded = hev1.encode_to_vec().unwrap();
        let (decoded, size) = Hev1Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, hev1.visual.width);
        prop_assert_eq!(decoded.visual.height, hev1.visual.height);
    }

    /// Hvc1Box の encode/decode roundtrip
    #[test]
    fn hvc1_box_roundtrip(
        visual in arb_visual_sample_entry(),
        hvcc in arb_hvcc_box()
    ) {
        let hvc1 = Hvc1Box {
            visual,
            hvcc_box: hvcc,
            unknown_boxes: vec![],
        };
        let encoded = hvc1.encode_to_vec().unwrap();
        let (decoded, size) = Hvc1Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, hvc1.visual.width);
        prop_assert_eq!(decoded.visual.height, hvc1.visual.height);
    }

    /// Vp08Box の encode/decode roundtrip
    #[test]
    fn vp08_box_roundtrip(
        visual in arb_visual_sample_entry(),
        vpcc in arb_vpcc_box()
    ) {
        let vp08 = Vp08Box {
            visual,
            vpcc_box: vpcc,
            unknown_boxes: vec![],
        };
        let encoded = vp08.encode_to_vec().unwrap();
        let (decoded, size) = Vp08Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, vp08.visual.width);
        prop_assert_eq!(decoded.visual.height, vp08.visual.height);
    }

    /// Vp09Box の encode/decode roundtrip
    #[test]
    fn vp09_box_roundtrip(
        visual in arb_visual_sample_entry(),
        vpcc in arb_vpcc_box()
    ) {
        let vp09 = Vp09Box {
            visual,
            vpcc_box: vpcc,
            unknown_boxes: vec![],
        };
        let encoded = vp09.encode_to_vec().unwrap();
        let (decoded, size) = Vp09Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, vp09.visual.width);
        prop_assert_eq!(decoded.visual.height, vp09.visual.height);
    }

    /// Av01Box の encode/decode roundtrip
    #[test]
    fn av01_box_roundtrip(
        visual in arb_visual_sample_entry(),
        av1c in arb_av1c_box()
    ) {
        let av01 = Av01Box {
            visual,
            av1c_box: av1c,
            unknown_boxes: vec![],
        };
        let encoded = av01.encode_to_vec().unwrap();
        let (decoded, size) = Av01Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.visual.width, av01.visual.width);
        prop_assert_eq!(decoded.visual.height, av01.visual.height);
    }
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// UnknownBox: 空のペイロード
    #[test]
    fn unknown_box_empty_payload() {
        let unknown = UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::with_payload_size(BoxType::Normal(*b"test"), 0),
            payload: vec![],
        };
        let encoded = unknown.encode_to_vec().unwrap();
        let (decoded, _) = UnknownBox::decode(&encoded).unwrap();
        assert!(decoded.payload.is_empty());
    }

    /// FreeBox: 空のペイロード
    #[test]
    fn free_box_empty_payload() {
        let free = FreeBox { payload: vec![] };
        let encoded = free.encode_to_vec().unwrap();
        let (decoded, _) = FreeBox::decode(&encoded).unwrap();
        assert!(decoded.payload.is_empty());
    }

    /// MdatBox: 空のペイロード
    #[test]
    fn mdat_box_empty_payload() {
        let mdat = MdatBox { payload: vec![] };
        let encoded = mdat.encode_to_vec().unwrap();
        let (decoded, _) = MdatBox::decode(&encoded).unwrap();
        assert!(decoded.payload.is_empty());
    }

    /// OpusBox: 最小構成
    #[test]
    fn opus_box_minimal() {
        let opus = OpusBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1).unwrap(),
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
        let encoded = opus.encode_to_vec().unwrap();
        let (decoded, _) = OpusBox::decode(&encoded).unwrap();
        assert_eq!(decoded.audio.channelcount, 2);
        assert_eq!(decoded.dops_box.output_channel_count, 2);
    }

    /// Mp4aBox: AAC-LC 設定
    #[test]
    fn mp4a_box_aac_lc() {
        let mp4a = Mp4aBox {
            audio: AudioSampleEntryFields {
                data_reference_index: NonZeroU16::new(1).unwrap(),
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
        let encoded = mp4a.encode_to_vec().unwrap();
        let (decoded, _) = Mp4aBox::decode(&encoded).unwrap();
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
                data_reference_index: NonZeroU16::new(1).unwrap(),
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
        let encoded = avc1.encode_to_vec().unwrap();
        let (decoded, _) = Avc1Box::decode(&encoded).unwrap();
        assert_eq!(decoded.visual.width, 1920);
        assert_eq!(decoded.visual.height, 1080);
    }
}

// ===== RootBox のテスト =====

mod root_box_tests {
    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Decode, Encode,
        boxes::{Brand, FreeBox, MdatBox, RootBox, UnknownBox},
    };

    /// RootBox::Free の encode/decode roundtrip
    #[test]
    fn root_box_free_roundtrip() {
        let free = FreeBox {
            payload: vec![0u8; 100],
        };
        let root = RootBox::Free(free);

        let encoded = root.encode_to_vec().unwrap();
        let (decoded, size) = RootBox::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Free(_)));
        assert_eq!(decoded.box_type(), FreeBox::TYPE);
        assert!(!decoded.is_unknown_box());

        // children() のテスト
        assert_eq!(decoded.children().count(), 0);
    }

    /// RootBox::Mdat の encode/decode roundtrip
    #[test]
    fn root_box_mdat_roundtrip() {
        let mdat = MdatBox {
            payload: vec![1, 2, 3, 4, 5],
        };
        let root = RootBox::Mdat(mdat);

        let encoded = root.encode_to_vec().unwrap();
        let (decoded, size) = RootBox::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Mdat(_)));
        assert_eq!(decoded.box_type(), MdatBox::TYPE);
        assert!(!decoded.is_unknown_box());
    }

    /// RootBox::Unknown の encode/decode roundtrip
    #[test]
    fn root_box_unknown_roundtrip() {
        let unknown = UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::with_payload_size(BoxType::Normal(*b"test"), 10),
            payload: vec![0u8; 10],
        };
        let root = RootBox::Unknown(unknown);

        let encoded = root.encode_to_vec().unwrap();
        let (decoded, size) = RootBox::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Unknown(_)));
        assert_eq!(decoded.box_type(), BoxType::Normal(*b"test"));
        assert!(decoded.is_unknown_box());
    }

    /// Brand の Debug 実装テスト: 有効な UTF-8
    #[test]
    fn brand_debug_valid_utf8() {
        let brand = Brand::new(*b"isom");
        let debug_str = format!("{:?}", brand);
        assert!(debug_str.contains("isom"));
    }

    /// Brand の Debug 実装テスト: 無効な UTF-8
    #[test]
    fn brand_debug_invalid_utf8() {
        let brand = Brand::new([0xFF, 0xFE, 0x00, 0x01]);
        let debug_str = format!("{:?}", brand);
        // 無効な UTF-8 の場合はバイト配列として表示される
        assert!(debug_str.contains("Brand"));
    }

    /// Brand の各定数のテスト
    #[test]
    fn brand_constants() {
        assert_eq!(Brand::ISOM.get(), *b"isom");
        assert_eq!(Brand::AVC1.get(), *b"avc1");
        assert_eq!(Brand::ISO2.get(), *b"iso2");
        assert_eq!(Brand::MP71.get(), *b"mp71");
        assert_eq!(Brand::ISO3.get(), *b"iso3");
        assert_eq!(Brand::ISO4.get(), *b"iso4");
        assert_eq!(Brand::ISO5.get(), *b"iso5");
        assert_eq!(Brand::ISO6.get(), *b"iso6");
        assert_eq!(Brand::ISO7.get(), *b"iso7");
        assert_eq!(Brand::ISO8.get(), *b"iso8");
        assert_eq!(Brand::ISO9.get(), *b"iso9");
        assert_eq!(Brand::ISOA.get(), *b"isoa");
        assert_eq!(Brand::ISOB.get(), *b"isob");
        assert_eq!(Brand::RELO.get(), *b"relo");
        assert_eq!(Brand::MP41.get(), *b"mp41");
        assert_eq!(Brand::AV01.get(), *b"av01");
    }

    /// Brand の encode/decode roundtrip
    #[test]
    fn brand_roundtrip() {
        let brand = Brand::new(*b"test");
        let encoded = brand.encode_to_vec().unwrap();
        let (decoded, size) = Brand::decode(&encoded).unwrap();

        assert_eq!(size, 4);
        assert_eq!(decoded.get(), *b"test");
    }
}

// ===== SampleEntry のメソッドテスト =====

mod sample_entry_tests {
    use std::num::NonZeroU16;

    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Decode, Encode, FixedPointNumber, Uint,
        boxes::{
            AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, DopsBox, EsdsBox, FlacBox,
            FlacMetadataBlock, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox, SampleEntry,
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

    /// SampleEntry::children() のテスト
    #[test]
    fn sample_entry_children() {
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
}
