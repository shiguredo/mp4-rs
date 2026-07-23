//! 追加の Box 構造体の Property-Based Testing
//!
//! proptest_boxes.rs と proptest_codec_boxes.rs でカバーされていない Box のテスト

use std::num::NonZeroU16;

use proptest::prelude::*;
use shiguredo_mp4::{
    BoxSize, BoxType, Decode, Encode, FixedPointNumber, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, BoxRecord, DflaBox, DopsBox,
        EsdsBox, FlacBox, FlacMetadataBlock, FontRecord, FreeBox, FtabBox, Hev1Box, Hvc1Box,
        HvccBox, MdatBox, Mp4aBox, OpusBox, StppBox, StyleRecord, Tx3gBox, UnknownBox,
        VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox, VttCBox, WvttBox,
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

/// null 文字を含まない任意の UTF-8 文字列を生成する Strategy
///
/// `Utf8String` は null 文字を含む文字列を受け入れないため、null 文字を除外する
/// （`pbt/tests/prop_basic_types.rs:41` の `arb_utf8_string` と同じ正規表現）
fn arb_utf8_string() -> impl Strategy<Value = String> {
    "[^\x00]{0,100}"
}

/// UnknownBox を生成する Strategy
///
/// 必須子ボックスを持たない SampleEntry（例: StppBox）で子ボックス経路を
/// PBT でカバーするために使う
fn arb_unknown_box() -> impl Strategy<Value = UnknownBox> {
    (any::<[u8; 4]>(), prop::collection::vec(any::<u8>(), 0..64)).prop_map(|(box_type, payload)| {
        UnknownBox {
            box_type: BoxType::Normal(box_type),
            box_size: BoxSize::with_payload_size(BoxType::Normal(box_type), payload.len() as u64),
            payload,
        }
    })
}

/// StppBox を生成する Strategy
///
/// `namespace` / `schema_location` / `auxiliary_mime_types` の 3 フィールドを
/// 独立に生成する（それぞれの空・非空パターンを網羅する）。
/// StppBox は必須子ボックスを持たないため、`unknown_boxes` を Strategy 経由で
/// 生成して decode / encode の子ボックス処理経路もカバーする
fn arb_stpp_box() -> impl Strategy<Value = StppBox> {
    (
        1u16..=u16::MAX,                                // data_reference_index
        arb_utf8_string(),                              // namespace
        arb_utf8_string(),                              // schema_location
        arb_utf8_string(),                              // auxiliary_mime_types
        prop::collection::vec(arb_unknown_box(), 0..3), // unknown_boxes
    )
        .prop_map(|(dri, ns, sl, am, unknown_boxes)| StppBox {
            data_reference_index: NonZeroU16::new(dri).unwrap(),
            namespace: Utf8String::new(&ns).expect("null 文字を含まない"),
            schema_location: Utf8String::new(&sl).expect("null 文字を含まない"),
            auxiliary_mime_types: Utf8String::new(&am).expect("null 文字を含まない"),
            unknown_boxes,
        })
}

/// VttCBox の config を生成する Strategy
///
/// interior null と改行を含む任意の valid UTF-8 文字列を生成する。
/// `.` は既定で null を含むが `\n` を除外するため、dotall フラグ `(?s)` を明示して
/// 改行も含める
fn arb_wvtt_config() -> impl Strategy<Value = String> {
    "(?s).{0,100}"
}

/// VttCBox を生成する Strategy
fn arb_vttc_box() -> impl Strategy<Value = VttCBox> {
    arb_wvtt_config().prop_map(|config| VttCBox { config })
}

/// WvttBox を生成する Strategy
///
/// `data_reference_index` と必須子 `vttc_box` に加えて 0-3 個の任意子ボックスを
/// 混ぜて decode / encode の子ボックス処理経路もカバーする
fn arb_wvtt_box() -> impl Strategy<Value = WvttBox> {
    (
        1u16..=u16::MAX,                                // data_reference_index
        arb_vttc_box(),                                 // vttc_box
        prop::collection::vec(arb_unknown_box(), 0..3), // unknown_boxes
    )
        .prop_map(|(dri, vttc_box, unknown_boxes)| WvttBox {
            data_reference_index: NonZeroU16::new(dri).expect("dri は 1u16 以上のため NonZero"),
            vttc_box,
            unknown_boxes,
        })
}

/// BoxRecord を生成する Strategy
///
/// `i16` 全域を許容する（3GPP TS 26.245 は値域を明示していない）
fn arb_box_record() -> impl Strategy<Value = BoxRecord> {
    (any::<i16>(), any::<i16>(), any::<i16>(), any::<i16>()).prop_map(
        |(top, left, bottom, right)| BoxRecord {
            top,
            left,
            bottom,
            right,
        },
    )
}

/// StyleRecord を生成する Strategy
///
/// 各フィールドは仕様上のビットマスク / 値域制限をせず、全域を生成する
fn arb_style_record() -> impl Strategy<Value = StyleRecord> {
    (
        any::<u16>(),     // start_char
        any::<u16>(),     // end_char
        any::<u16>(),     // font_id
        any::<u8>(),      // face_style_flags
        any::<u8>(),      // font_size
        any::<[u8; 4]>(), // text_color_rgba
    )
        .prop_map(
            |(start_char, end_char, font_id, face_style_flags, font_size, text_color_rgba)| {
                StyleRecord {
                    start_char,
                    end_char,
                    font_id,
                    face_style_flags,
                    font_size,
                    text_color_rgba,
                }
            },
        )
}

/// `FontRecord::font_name` を生成する Strategy
///
/// Pascal string の長さ制約（0-255 バイト）に合わせて任意バイト列を生成する
fn arb_font_name() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=255)
}

/// FontRecord を生成する Strategy
fn arb_font_record() -> impl Strategy<Value = FontRecord> {
    (any::<u16>(), arb_font_name())
        .prop_map(|(font_id, font_name)| FontRecord { font_id, font_name })
}

/// FtabBox を生成する Strategy
///
/// エントリー数は combinatorial 爆発回避のため 0-8 個に制限する。
/// 0 個も許容してパーサ堅牢性のエッジケースを含める
fn arb_ftab_box() -> impl Strategy<Value = FtabBox> {
    prop::collection::vec(arb_font_record(), 0..=8).prop_map(|entries| FtabBox { entries })
}

/// Tx3gBox を生成する Strategy
///
/// 本体固定サイズ 30 バイトと必須子 `ftab_box` に加えて 0-3 個の任意子ボックスを
/// 混ぜて decode / encode の子ボックス処理経路もカバーする
fn arb_tx3g_box() -> impl Strategy<Value = Tx3gBox> {
    (
        1u16..=u16::MAX,                                // data_reference_index
        any::<u32>(),                                   // display_flags
        any::<i8>(),                                    // horizontal_justification
        any::<i8>(),                                    // vertical_justification
        any::<[u8; 4]>(),                               // background_color_rgba
        arb_box_record(),                               // default_text_box
        arb_style_record(),                             // default_style
        arb_ftab_box(),                                 // ftab_box
        prop::collection::vec(arb_unknown_box(), 0..3), // unknown_boxes
    )
        .prop_map(
            |(
                dri,
                display_flags,
                horizontal_justification,
                vertical_justification,
                background_color_rgba,
                default_text_box,
                default_style,
                ftab_box,
                unknown_boxes,
            )| Tx3gBox {
                data_reference_index: NonZeroU16::new(dri).expect("dri は 1u16 以上のため NonZero"),
                display_flags,
                horizontal_justification,
                vertical_justification,
                background_color_rgba,
                default_text_box,
                default_style,
                ftab_box,
                unknown_boxes,
            },
        )
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

    // ===== Subtitle Sample Entry Box のテスト =====

    /// StppBox の encode/decode roundtrip
    ///
    /// 3 フィールドすべてに任意の UTF-8 文字列（空文字列も含む）と、
    /// 0-3 個の任意の子ボックスを割り当ててラウンドトリップを検証する
    #[test]
    fn stpp_box_roundtrip(stpp in arb_stpp_box()) {
        let encoded = stpp.encode_to_vec().unwrap();
        let (decoded, size) = StppBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.data_reference_index, stpp.data_reference_index);
        prop_assert_eq!(&decoded.namespace, &stpp.namespace);
        prop_assert_eq!(&decoded.schema_location, &stpp.schema_location);
        prop_assert_eq!(&decoded.auxiliary_mime_types, &stpp.auxiliary_mime_types);
        prop_assert_eq!(&decoded.unknown_boxes, &stpp.unknown_boxes);
    }

    /// VttCBox の encode/decode roundtrip
    ///
    /// config は任意の UTF-8 文字列（空文字列・改行・interior null すべて含む）を
    /// 割り当ててラウンドトリップを検証する
    #[test]
    fn vttc_box_roundtrip(vttc in arb_vttc_box()) {
        let encoded = vttc.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) = VttCBox::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(&decoded.config, &vttc.config);
    }

    /// WvttBox の encode/decode roundtrip
    ///
    /// 必須子 vttC と 0-3 個の任意の子ボックスを割り当ててラウンドトリップを検証する
    #[test]
    fn wvtt_box_roundtrip(wvtt in arb_wvtt_box()) {
        let encoded = wvtt.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) = WvttBox::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.data_reference_index, wvtt.data_reference_index);
        prop_assert_eq!(&decoded.vttc_box, &wvtt.vttc_box);
        prop_assert_eq!(&decoded.unknown_boxes, &wvtt.unknown_boxes);
    }

    /// BoxRecord の encode/decode roundtrip
    ///
    /// `i16` 4 個の 8 バイト固定レコードを検証する
    #[test]
    fn box_record_roundtrip(record in arb_box_record()) {
        let encoded = record.encode_to_vec().expect("encode に失敗しない想定");
        prop_assert_eq!(encoded.len(), 8);
        let (decoded, size) = BoxRecord::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");
        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded, record);
    }

    /// StyleRecord の encode/decode roundtrip
    ///
    /// 12 バイト固定レコードのフィールド全域を検証する
    #[test]
    fn style_record_roundtrip(record in arb_style_record()) {
        let encoded = record.encode_to_vec().expect("encode に失敗しない想定");
        prop_assert_eq!(encoded.len(), 12);
        let (decoded, size) = StyleRecord::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");
        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded, record);
    }

    /// FtabBox の encode/decode roundtrip
    ///
    /// 空エントリー・複数エントリー・font_name の長さ境界（0 / 255）を含めて検証する
    #[test]
    fn ftab_box_roundtrip(ftab in arb_ftab_box()) {
        let encoded = ftab.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) = FtabBox::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");
        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(&decoded.entries, &ftab.entries);
    }

    /// Tx3gBox の encode/decode roundtrip
    ///
    /// 本体固定 30 バイト + 必須子 ftab + 0-3 個の任意子ボックスを割り当てて
    /// ラウンドトリップを検証する
    #[test]
    fn tx3g_box_roundtrip(tx3g in arb_tx3g_box()) {
        let encoded = tx3g.encode_to_vec().expect("encode に失敗しない想定");
        let (decoded, size) = Tx3gBox::decode(&encoded).expect("自前で encode した結果は必ず decode 可能");

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.data_reference_index, tx3g.data_reference_index);
        prop_assert_eq!(decoded.display_flags, tx3g.display_flags);
        prop_assert_eq!(decoded.horizontal_justification, tx3g.horizontal_justification);
        prop_assert_eq!(decoded.vertical_justification, tx3g.vertical_justification);
        prop_assert_eq!(decoded.background_color_rgba, tx3g.background_color_rgba);
        prop_assert_eq!(decoded.default_text_box, tx3g.default_text_box);
        prop_assert_eq!(decoded.default_style, tx3g.default_style);
        prop_assert_eq!(&decoded.ftab_box, &tx3g.ftab_box);
        prop_assert_eq!(&decoded.unknown_boxes, &tx3g.unknown_boxes);
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
        BaseBox, BoxSize, BoxType, Decode, Encode, FixedPointNumber, Uint, Utf8String,
        boxes::{
            AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, BoxRecord, DopsBox,
            EsdsBox, FlacBox, FlacMetadataBlock, FontRecord, FtabBox, Hev1Box, Hvc1Box, HvccBox,
            Mp4aBox, OpusBox, SampleEntry, StppBox, StyleRecord, Tx3gBox, UnknownBox,
            VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox, VttCBox, WvttBox,
        },
        descriptors::{DecoderConfigDescriptor, EsDescriptor, SlConfigDescriptor},
    };

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

    /// entry_count = 0 の ftab がラウンドトリップできることを deterministic に担保する
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

    /// SampleEntry::decode で tx3g box_type を持つ入力が Tx3g バリアントとして取り出されることを検証する
    ///
    /// 型付き Tx3g バリアント追加前は `SampleEntry::Unknown` にフォールバックしていたため、
    /// dispatch の回帰確認として置く
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
