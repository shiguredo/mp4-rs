//! `shiguredo_mp4::codec_string` の決定的テスト
//!
//! 各 `SampleEntry` について仕様例・境界（HEVC constraint 全ゼロ、AAC 欠落）を固定する。
//! 任意入力の不変条件は `pbt/tests/prop_codec_string.rs` が担う。

use std::num::NonZeroU16;

use shiguredo_mp4::{
    BoxSize, BoxType, Decode, Encode, ErrorKind, FixedPointNumber, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Av01Box, Av1cBox, Avc1Box, AvccBox, BoxRecord, DflaBox, DopsBox,
        EncaBox, EncvBox, EsdsBox, FlacBox, FlacMetadataBlock, FrmaBox, FtabBox, Hev1Box, Hvc1Box,
        HvccBox, Mp4aBox, OpusBox, SampleEntry, SchiBox, SchmBox, SinfBox, StppBox, StyleRecord,
        TencBox, Tx3gBox, UnknownBox, VisualSampleEntryFields, Vp08Box, Vp09Box, VpccBox, VttCBox,
        WvttBox,
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

/// HEVC: profile_space と High tier が文字列に反映される
#[test]
fn hevc_profile_space_and_high_tier() {
    let mut hvcc = empty_hvcc();
    hvcc.general_profile_space = Uint::new(1);
    hvcc.general_tier_flag = Uint::new(1);
    hvcc.general_profile_idc = Uint::new(2);

    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: hvcc,
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.A2.6.H93.B0"
    );
}

/// HEVC: constraint 中間のゼロバイトは省略せず残す
#[test]
fn hevc_keeps_middle_zero_constraint_bytes() {
    let mut hvcc = empty_hvcc();
    // B0, 00, 01 の 3 バイト目まで非ゼロ末尾 → 末尾ゼロのみ省略
    hvcc.general_constraint_indicator_flags = Uint::new(0x0000_B000_0100_0000);

    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: hvcc,
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.1.6.L93.B0.00.01"
    );
}

/// HEVC: `general_profile_compatibility_flags` が 0 なら hex は `0`（欄ごと省略しない）
#[test]
fn hevc_zero_compatibility_flags_emits_zero_hex() {
    let mut hvcc = empty_hvcc();
    hvcc.general_profile_compatibility_flags = 0;

    let entry = SampleEntry::Hev1(Hev1Box {
        visual: visual_fields(),
        hvcc_box: hvcc,
        unknown_boxes: vec![],
    });

    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("Hev1 は成功する"),
        "hev1.1.0.L93.B0"
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

/// AV1 High Profile / 10-bit
#[test]
fn av01_high_profile_10bit() {
    let entry = SampleEntry::Av01(Av01Box {
        visual: visual_fields(),
        av1c_box: Av1cBox {
            seq_profile: Uint::new(1),
            seq_level_idx_0: Uint::new(0),
            seq_tier_0: Uint::new(0),
            high_bitdepth: Uint::new(1),
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
        "av01.1.00M.10"
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

/// AV1: profile 2 + high_bitdepth + twelve_bit=0 で 10-bit（内側 else）
#[test]
fn av01_profile2_ten_bit() {
    let entry = SampleEntry::Av01(Av01Box {
        visual: visual_fields(),
        av1c_box: Av1cBox {
            seq_profile: Uint::new(2),
            seq_level_idx_0: Uint::new(4),
            seq_tier_0: Uint::new(1),
            high_bitdepth: Uint::new(1),
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
        "av01.2.04H.10"
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

/// AOT 31 エスケープの非ゼロ ext: 下位 3 bit と次バイト上位 3 bit を両方踏む
#[test]
fn mp4a_escaped_audio_object_type_nonzero_ext() {
    // 0xF9 = 0b11111_001、0x20 = 0b00100000 → ext = 0b001_001 = 9 → AOT 41
    let entry = mp4a_with_asc(Some(vec![0xF9, 0x20]));
    assert_eq!(
        codec_string::from_sample_entry(&entry).expect("非ゼロ ext のエスケープ AOT は成功する"),
        "mp4a.40.41"
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

// ===== CENC で保護されたサンプルエントリーのテスト =====

/// CENC の保護スキーム情報を持つ `sinf` を組み立てる
fn cenc_sinf_box(data_format: [u8; 4]) -> SinfBox {
    SinfBox {
        frma_box: FrmaBox { data_format },
        schm_box: Some(SchmBox {
            scheme_type: *b"cenc",
            scheme_version: 0x0001_0000,
            scheme_uri: None,
        }),
        schi_box: Some(SchiBox {
            tenc_box: Some(TencBox {
                default_is_protected: 1,
                default_per_sample_iv_size: 8,
                default_kid: [0x22; 16],
                default_constant_iv: None,
                default_crypt_byte_block: None,
                default_skip_byte_block: None,
            }),
            unknown_boxes: vec![],
        }),
        unknown_boxes: vec![],
    }
}

/// 設定ボックスを `unknown_boxes` 用の `UnknownBox` に変換する
///
/// `EncvBox` / `EncaBox` はコーデック設定ボックスを生バイトの `UnknownBox` として保持する。
/// ここでは設定ボックスを encode した結果をそのまま `UnknownBox` としてデコードする。
fn config_as_unknown_box<T: Encode>(config: &T) -> UnknownBox {
    let encoded = config
        .encode_to_vec()
        .expect("テスト用のコーデック設定ボックスは encode できるはず");
    let (unknown, size) = UnknownBox::decode(&encoded)
        .expect("直前に encode したコーデック設定ボックスは UnknownBox としてデコードできるはず");
    assert_eq!(size, encoded.len());
    unknown
}

/// 保護された映像サンプルエントリーを組み立てる
fn encv_entry(frma: [u8; 4], unknown_boxes: Vec<UnknownBox>) -> SampleEntry {
    SampleEntry::Encv(EncvBox {
        visual: visual_fields(),
        sinf_box: cenc_sinf_box(frma),
        unknown_boxes,
    })
}

/// 保護された音声サンプルエントリーを組み立てる
fn enca_entry(frma: [u8; 4], unknown_boxes: Vec<UnknownBox>) -> SampleEntry {
    SampleEntry::Enca(EncaBox {
        audio: audio_fields(),
        sinf_box: cenc_sinf_box(frma),
        unknown_boxes,
    })
}

/// 保護された映像サンプルエントリーから期待どおりのコーデック文字列が得られることを確認する
fn assert_encv_codec_string(frma: [u8; 4], config: Option<UnknownBox>, expected: &str) {
    let entry = encv_entry(frma, config.into_iter().collect());
    assert_eq!(
        codec_string::from_sample_entry(&entry)
            .expect("保護された映像サンプルエントリーは成功する"),
        expected
    );
}

/// 保護された音声サンプルエントリーから期待どおりのコーデック文字列が得られることを確認する
fn assert_enca_codec_string(frma: [u8; 4], config: Option<UnknownBox>, expected: &str) {
    let entry = enca_entry(frma, config.into_iter().collect());
    assert_eq!(
        codec_string::from_sample_entry(&entry)
            .expect("保護された音声サンプルエントリーは成功する"),
        expected
    );
}

/// `vp08` / `vp09` 用の `vpcC` を組み立てる
fn vpcc_box(level: u8) -> VpccBox {
    VpccBox {
        profile: 0,
        level,
        bit_depth: Uint::new(8),
        chroma_subsampling: Uint::new(1),
        video_full_range_flag: Uint::new(0),
        colour_primaries: 1,
        transfer_characteristics: 1,
        matrix_coefficients: 1,
        codec_initialization_data: vec![],
    }
}

/// AAC-LC (AOT 2, 48 kHz, ステレオ) の `esds` を組み立てる
fn aac_esds_box() -> EsdsBox {
    EsdsBox {
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
                dec_specific_info: Some(DecoderSpecificInfo {
                    payload: vec![0x11, 0x90],
                }),
            },
            sl_config_descr: SlConfigDescriptor,
        },
    }
}

/// `encv` + `avc1`: `unknown_boxes` の `avcC` からコーデック文字列を生成する
#[test]
fn encv_avc1_codec_string() {
    let avcc = AvccBox {
        avc_profile_indication: 100,
        profile_compatibility: 0x00,
        avc_level_indication: 40,
        length_size_minus_one: Uint::new(3),
        sps_list: vec![],
        pps_list: vec![],
        // H.264 High Profile は 66 / 77 / 88 以外のため、chroma_format とビット深度が必須になる
        chroma_format: Some(Uint::new(1)),
        bit_depth_luma_minus8: Some(Uint::new(0)),
        bit_depth_chroma_minus8: Some(Uint::new(0)),
        sps_ext_list: vec![],
    };
    assert_encv_codec_string(*b"avc1", Some(config_as_unknown_box(&avcc)), "avc1.640028");
}

/// `encv` + `hev1`: `unknown_boxes` の `hvcC` から `hev1` のコーデック文字列を生成する
#[test]
fn encv_hev1_codec_string() {
    assert_encv_codec_string(
        *b"hev1",
        Some(config_as_unknown_box(&empty_hvcc())),
        "hev1.1.6.L93.B0",
    );
}

/// `encv` + `hvc1`: `unknown_boxes` の `hvcC` から `hvc1` のコーデック文字列を生成する
#[test]
fn encv_hvc1_codec_string() {
    assert_encv_codec_string(
        *b"hvc1",
        Some(config_as_unknown_box(&empty_hvcc())),
        "hvc1.1.6.L93.B0",
    );
}

/// `encv` + `vp08`: `unknown_boxes` の `vpcC` から `vp08` のコーデック文字列を生成する
#[test]
fn encv_vp08_codec_string() {
    assert_encv_codec_string(
        *b"vp08",
        Some(config_as_unknown_box(&vpcc_box(0))),
        "vp08.00.00.08",
    );
}

/// `encv` + `vp09`: `unknown_boxes` の `vpcC` から `vp09` のコーデック文字列を生成する
#[test]
fn encv_vp09_codec_string() {
    assert_encv_codec_string(
        *b"vp09",
        Some(config_as_unknown_box(&vpcc_box(31))),
        "vp09.00.31.08",
    );
}

/// `encv` + `av01`: `unknown_boxes` の `av1C` から `av01` のコーデック文字列を生成する
#[test]
fn encv_av01_codec_string() {
    let av1c = Av1cBox {
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
    };
    assert_encv_codec_string(
        *b"av01",
        Some(config_as_unknown_box(&av1c)),
        "av01.0.00M.08",
    );
}

/// `enca` + `mp4a`: `unknown_boxes` の `esds` から `mp4a` のコーデック文字列を生成する
#[test]
fn enca_mp4a_codec_string() {
    assert_enca_codec_string(
        *b"mp4a",
        Some(config_as_unknown_box(&aac_esds_box())),
        "mp4a.40.2",
    );
}

/// `enca` + `Opus`: 設定ボックスがなくても 4CC をそのまま返す
#[test]
fn enca_opus_codec_string() {
    assert_enca_codec_string(*b"Opus", None, "Opus");
}

/// `enca` + `fLaC`: 設定ボックスがなくても 4CC をそのまま返す
#[test]
fn enca_flac_codec_string() {
    assert_enca_codec_string(*b"fLaC", None, "fLaC");
}

/// `encv` の `frma` が未知の 4CC なら Unsupported になる
#[test]
fn encv_unknown_frma_is_unsupported() {
    let entry = encv_entry(*b"zzzz", vec![]);
    let err = codec_string::from_sample_entry(&entry).expect_err("未知の frma はエラー");
    assert_eq!(err.kind, ErrorKind::Unsupported);
}

/// `encv` の `frma` が対応する映像コーデックでも、設定ボックスがなければ Unsupported になる
#[test]
fn encv_missing_codec_config_box_is_unsupported() {
    for frma in [*b"avc1", *b"hev1", *b"hvc1", *b"vp08", *b"vp09", *b"av01"] {
        let entry = encv_entry(frma, vec![]);
        let err = match codec_string::from_sample_entry(&entry) {
            Ok(codec) => panic!(
                "frma={} で設定ボックスがない場合はエラーになるはず (実際は {codec})",
                BoxType::Normal(frma)
            ),
            Err(e) => e,
        };
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "frma={} で設定ボックスがない場合のエラー種別が Unsupported ではない (実際は {:?})",
            BoxType::Normal(frma),
            err.kind,
        );
    }
}

/// `enca` の `frma` が `mp4a` でも、`esds` がなければ Unsupported になる
#[test]
fn enca_missing_codec_config_box_is_unsupported() {
    let entry = enca_entry(*b"mp4a", vec![]);
    let err = codec_string::from_sample_entry(&entry).expect_err("esds を持たない mp4a はエラー");
    assert_eq!(err.kind, ErrorKind::Unsupported);
}
