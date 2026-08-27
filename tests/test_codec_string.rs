//! `shiguredo_mp4::codec_string` の決定的テスト
//!
//! 各 `SampleEntry` について仕様例・境界（HEVC constraint 全ゼロ、AAC 欠落）を固定する。
//! 任意入力の不変条件は `pbt/tests/prop_codec_string.rs` が担う。

use std::num::NonZeroU16;

use shiguredo_mp4::{
    BoxSize, BoxType, ErrorKind, FixedPointNumber, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, BoxRecord, DflaBox, DopsBox,
        EsdsBox, FlacBox, FlacMetadataBlock, FtabBox, Hev1Box, Hvc1Box, HvccBox, Mp4aBox, OpusBox,
        SampleEntry, StppBox, StyleRecord, Tx3gBox, UnknownBox, VisualSampleEntryFields, Vp08Box,
        Vp09Box, VpccBox, VttCBox, WvttBox,
    },
    codec_string,
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

fn visual_fields() -> VisualSampleEntryFields {
    VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width: 1920,
        height: 1080,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    }
}

fn audio_fields() -> AudioSampleEntryFields {
    AudioSampleEntryFields {
        data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        channelcount: 2,
        samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
        samplerate: FixedPointNumber::new(48000, 0),
    }
}

fn empty_hvcc() -> HvccBox {
    HvccBox {
        general_profile_space: Uint::new(0),
        general_tier_flag: Uint::new(0),
        general_profile_idc: Uint::new(1),
        general_profile_compatibility_flags: 0x6000_0000,
        general_constraint_indicator_flags: Uint::new(0xb000_0000_0000),
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
    }
}

/// H.264 High Profile Level 4.0 の典型例
#[test]
fn avc1_high_profile_level_40() {
    let entry = SampleEntry::Avc1(Avc1Box {
        visual: visual_fields(),
        avcc_box: AvccBox {
            avc_profile_indication: 100,
            profile_compatibility: 0x00,
            avc_level_indication: 40,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: Some(Uint::new(1)),
            bit_depth_luma_minus8: Some(Uint::new(0)),
            bit_depth_chroma_minus8: Some(Uint::new(0)),
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Avc1 は成功する"),
        "avc1.640028"
    );
}

/// HEVC Main / Main Tier / Level 3.1 の典型例（hev1）
#[test]
fn hev1_main_profile_level_31() {
    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: empty_hvcc(),
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.1.6.L93.B0"
    );
}

/// 同じ hvcc でも hvc1 プレフィックスになること
#[test]
fn hvc1_uses_hvc1_prefix() {
    let entry = SampleEntry::Hvc1(Hvc1Box {
        visual: visual_fields(),
        hvcc_box: empty_hvcc(),
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hvc1 は成功する"),
        "hvc1.1.6.L93.B0"
    );
}

/// HEVC: 末尾ゼロバイトを省略し、途中の非ゼロは残す
#[test]
fn hevc_omits_trailing_zero_constraint_bytes() {
    let mut hvcc = empty_hvcc();
    // 先頭 2 バイト非ゼロ、残りゼロ → `.B0.01` まで残る
    hvcc.general_constraint_indicator_flags = Uint::new(0xb001_0000_0000);

    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: hvcc,
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.1.6.L93.B0.01"
    );
}

/// HEVC: constraint が全ゼロでも最低 1 バイト `00` を残す
#[test]
fn hevc_keeps_one_zero_byte_when_all_constraints_zero() {
    let mut hvcc = empty_hvcc();
    hvcc.general_constraint_indicator_flags = Uint::new(0);

    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: hvcc,
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.1.6.L93.00"
    );
}

/// AV1 Main Profile / Level 0 / Main Tier / 8-bit
#[test]
fn av01_main_8bit() {
    let entry = SampleEntry::Av01(Av01Box {
        visual: visual_fields(),
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

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Av01 は成功する"),
        "av01.0.00M.08"
    );
}

/// AV1: profile 2 以外では twelve_bit を無視し、high_bitdepth のみで 10-bit になる
#[test]
fn av01_bit_depth_ignores_twelve_bit_outside_profile_2() {
    let entry = SampleEntry::Av01(Av01Box {
        visual: visual_fields(),
        av1c_box: Av1cBox {
            seq_profile: Uint::new(0),
            seq_level_idx_0: Uint::new(1),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(1),
            twelve_bit: Uint::new(1),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        },
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Av01 は成功する"),
        "av01.0.01M.10"
    );
}

/// AV1: profile 2 + high_bitdepth + twelve_bit で 12-bit
#[test]
fn av01_profile2_twelve_bit() {
    let entry = SampleEntry::Av01(Av01Box {
        visual: visual_fields(),
        av1c_box: Av1cBox {
            seq_profile: Uint::new(2),
            seq_level_idx_0: Uint::new(4),
            seq_tier_0: Uint::new(1),
            high_bitdepth: Uint::new(1),
            twelve_bit: Uint::new(1),
            monochrome: Uint::new(0),
            chroma_subsampling_x: Uint::new(1),
            chroma_subsampling_y: Uint::new(1),
            chroma_sample_position: Uint::new(0),
            initial_presentation_delay_minus_one: None,
            config_obus: vec![],
        },
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Av01 は成功する"),
        "av01.2.04H.12"
    );
}

/// VP9: 必須形のみ（任意欄は付けない）
#[test]
fn vp09_mandatory_fields_only() {
    let entry = SampleEntry::Vp09(Vp09Box {
        visual: visual_fields(),
        vpcc_box: VpccBox {
            profile: 0,
            level: 31,
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

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Vp09 は成功する"),
        "vp09.00.31.08"
    );
}

/// VP8: ISOBMFF の 4CC `vp08` を使う（`vp8` ではない）
#[test]
fn vp08_uses_isobmff_fourcc() {
    let entry = SampleEntry::Vp08(Vp08Box {
        visual: visual_fields(),
        vpcc_box: VpccBox {
            profile: 0,
            level: 0,
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

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Vp08 は成功する"),
        "vp08.00.00.08"
    );
}

fn mp4a_with_asc(payload: Option<Vec<u8>>) -> SampleEntry {
    SampleEntry::Mp4a(Mp4aBox {
        audio: audio_fields(),
        esds_box: EsdsBox {
            es: EsDescriptor {
                es_id: 1,
                stream_priority: Uint::new(0),
                depends_on_es_id: None,
                url_string: None,
                ocr_es_id: None,
                dec_config_descr: DecoderConfigDescriptor {
                    object_type_indication:
                        DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
                    stream_type: Uint::new(0x05),
                    up_stream: Uint::new(0),
                    buffer_size_db: Uint::new(0),
                    max_bitrate: 128000,
                    avg_bitrate: 128000,
                    dec_specific_info: payload.map(|payload| DecoderSpecificInfo { payload }),
                },
                sl_config_descr: SlConfigDescriptor,
            },
        },
        unknown_boxes: vec![],
    })
}

/// AAC-LC (AOT 2): `mp4a.40.2`
#[test]
fn mp4a_aac_lc() {
    // AAC-LC / 48 kHz / stereo: 0b00010_0011_0010_000
    let entry = mp4a_with_asc(Some(vec![0x11, 0x90]));
    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Mp4a AAC-LC は成功する"),
        "mp4a.40.2"
    );
}

/// AOT 31 エスケープ形式: 先頭 5 bit が 31、続き 6 bit から 32+ext
#[test]
fn mp4a_escaped_audio_object_type() {
    // AOT = 31 のエスケープ、拡張 6 bit = 0 → 結果 AOT 32
    // 先頭バイト: 0b11111_000、次バイト上位 6 bit: 0b000000xx
    let entry = mp4a_with_asc(Some(vec![0xF8, 0x00]));
    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("エスケープ AOT は成功する"),
        "mp4a.40.32"
    );
}

/// OTI が 0x40 以外なら AOT を付けない
#[test]
fn mp4a_non_0x40_oti_without_aot() {
    let entry = SampleEntry::Mp4a(Mp4aBox {
        audio: audio_fields(),
        esds_box: EsdsBox {
            es: EsDescriptor {
                es_id: 1,
                stream_priority: Uint::new(0),
                depends_on_es_id: None,
                url_string: None,
                ocr_es_id: None,
                dec_config_descr: DecoderConfigDescriptor {
                    object_type_indication: 0x6B,
                    stream_type: Uint::new(0x05),
                    up_stream: Uint::new(0),
                    buffer_size_db: Uint::new(0),
                    max_bitrate: 0,
                    avg_bitrate: 0,
                    dec_specific_info: None,
                },
                sl_config_descr: SlConfigDescriptor,
            },
        },
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("非 0x40 OTI は成功する"),
        "mp4a.6b"
    );
}

/// OTI 0x40 で DecoderSpecificInfo 欠落は InvalidData（AAC-LC を仮定しない）
#[test]
fn mp4a_missing_dsi_is_invalid_data() {
    let entry = mp4a_with_asc(None);
    let err = codec_string::from_sample_entry(&entry).expect_err("DSI 欠落はエラー");
    assert_eq!(err.kind, ErrorKind::InvalidData);
}

/// OTI 0x40 で空 payload は InvalidData
#[test]
fn mp4a_empty_asc_is_invalid_data() {
    let entry = mp4a_with_asc(Some(vec![]));
    let err = codec_string::from_sample_entry(&entry).expect_err("空 ASC はエラー");
    assert_eq!(err.kind, ErrorKind::InvalidData);
}

/// OTI 0x40 でエスケープ AOT が途中切れは InvalidData
#[test]
fn mp4a_truncated_escaped_aot_is_invalid_data() {
    // AOT 31 だが 2 バイト目が無い
    let entry = mp4a_with_asc(Some(vec![0xF8]));
    let err = codec_string::from_sample_entry(&entry).expect_err("切り詰めはエラー");
    assert_eq!(err.kind, ErrorKind::InvalidData);
}

/// Opus / FLAC は ISOBMFF の sample entry 4CC を返す
#[test]
fn opus_and_flac_return_registered_fourcc() {
    let opus = SampleEntry::Opus(OpusBox {
        audio: audio_fields(),
        dops_box: DopsBox {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        },
        unknown_boxes: vec![],
    });
    assert_eq!(
        codec_string::from_sample_entry(&opus).expect("Opus は成功する"),
        "Opus"
    );

    let flac = SampleEntry::Flac(FlacBox {
        audio: audio_fields(),
        dfla_box: DflaBox {
            metadata_blocks: vec![FlacMetadataBlock {
                last_metadata_block_flag: Uint::new(1),
                block_type: FlacMetadataBlock::BLOCK_TYPE_STREAMINFO,
                block_data: vec![0; 34],
            }],
        },
        unknown_boxes: vec![],
    });
    assert_eq!(
        codec_string::from_sample_entry(&flac).expect("Flac は成功する"),
        "fLaC"
    );
}

/// 字幕系も登録済み 4CC を返す
#[test]
fn subtitle_entries_return_registered_fourcc() {
    let stpp = SampleEntry::Stpp(StppBox {
        data_reference_index: NonZeroU16::MIN,
        namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null なし"),
        schema_location: Utf8String::EMPTY,
        auxiliary_mime_types: Utf8String::EMPTY,
        unknown_boxes: vec![],
    });
    assert_eq!(
        codec_string::from_sample_entry(&stpp).expect("Stpp は成功する"),
        "stpp"
    );

    let wvtt = SampleEntry::Wvtt(WvttBox {
        data_reference_index: NonZeroU16::MIN,
        vttc_box: VttCBox {
            config: String::from("WEBVTT"),
        },
        unknown_boxes: vec![],
    });
    assert_eq!(
        codec_string::from_sample_entry(&wvtt).expect("Wvtt は成功する"),
        "wvtt"
    );

    let tx3g = SampleEntry::Tx3g(Tx3gBox {
        data_reference_index: NonZeroU16::MIN,
        display_flags: 0,
        horizontal_justification: 0,
        vertical_justification: 0,
        background_color_rgba: [0; 4],
        default_text_box: BoxRecord::default(),
        default_style: StyleRecord::default(),
        ftab_box: FtabBox::default(),
        unknown_boxes: vec![],
    });
    assert_eq!(
        codec_string::from_sample_entry(&tx3g).expect("Tx3g は成功する"),
        "tx3g"
    );
}

/// 未知 sample entry は Unsupported
#[test]
fn unknown_sample_entry_is_unsupported() {
    let entry = SampleEntry::Unknown(UnknownBox {
        box_type: BoxType::Normal(*b"zzzz"),
        box_size: BoxSize::U32(8),
        payload: vec![],
    });
    let err = codec_string::from_sample_entry(&entry).expect_err("Unknown はエラー");
    assert_eq!(err.kind, ErrorKind::Unsupported);
}
