//! `src/boxes_moov_tree.rs` に定義される moov ツリー配下ボックスの Property-Based Testing

mod moov_tree_error_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{Encode, Mp4FileTime, boxes::MdhdBox};

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
}

// ===== boxes_moov_tree.rs 系ボックスの境界値・バリアント違いテスト =====

mod moov_tree_boundary_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        Decode, Encode, FixedPointNumber, Mp4FileTime, Utf8String,
        boxes::{
            Co64Box, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry, HdlrBox, MdhdBox, MinfBox,
            MvhdBox, StblBox, StcoBox, StscBox, StsdBox, StszBox, SttsBox, TkhdBox, UrlBox,
            VmhdBox,
        },
    };

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

    /// MinfBox: media_header なし
    #[test]
    fn minf_box_without_media_header() {
        use shiguredo_mp4::Either;
        use shiguredo_mp4::boxes::{AudioSampleEntryFields, DopsBox, OpusBox, SampleEntry};
        use std::num::NonZeroU16;

        let minf = MinfBox {
            media_header: None,
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
        assert!(decoded.media_header.is_none());
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

// ===== BaseBox トレイトの実装テスト (boxes_moov_tree.rs 系) =====

mod moov_tree_base_box_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        BaseBox, BoxType, Either, FixedPointNumber, Mp4FileTime,
        boxes::{
            Co64Box, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry, HdlrBox, MdhdBox, MdiaBox,
            MediaHeader, MinfBox, MoovBox, MvhdBox, SmhdBox, StblBox, StcoBox, StscBox, StsdBox,
            StszBox, SttsBox, TkhdBox, TrakBox, UrlBox, VmhdBox,
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
            media_header: Some(MediaHeader::Vmhd(VmhdBox {
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
}
