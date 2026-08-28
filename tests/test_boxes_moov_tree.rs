//! `src/boxes_moov_tree.rs` に定義される moov ツリー配下ボックスの境界値・BaseBox 単体テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_boxes.rs` / `pbt/tests/prop_container_boxes.rs`
//! が担う。本ファイルは PBT では安定して狙いにくい境界値（`u32::MAX` を超える
//! 64 ビット系の値など）と、各ボックスの `BaseBox` 実装を固定する。

// ===== boxes_moov_tree.rs 系ボックスの境界値・バリアント違いテスト =====

mod moov_tree_boundary_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        Decode, Encode, FixedPointNumber, LanguageCode, Mp4FileTime, Utf8String,
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
            timescale: NonZeroU32::new(1000).expect("timescale は非ゼロである"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("デコードは失敗しない");
        assert_eq!(decoded.creation_time.as_secs(), u32::MAX as u64 + 1);
    }

    /// MvhdBox: version 1 (64ビット) - modification_time が u32::MAX を超える
    #[test]
    fn mvhd_box_version_1_large_modification_time() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(u32::MAX as u64 + 1),
            timescale: NonZeroU32::new(1000).expect("timescale は非ゼロである"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("デコードは失敗しない");
        assert_eq!(decoded.modification_time.as_secs(), u32::MAX as u64 + 1);
    }

    /// MvhdBox: version 1 (64ビット) - duration が u32::MAX を超える
    #[test]
    fn mvhd_box_version_1_large_duration() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("timescale は非ゼロである"),
            duration: u32::MAX as u64 + 1,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = MvhdBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = tkhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = TkhdBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = tkhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = TkhdBox::decode(&encoded).expect("デコードは失敗しない");
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
            timescale: NonZeroU32::new(48000).expect("timescale は非ゼロである"),
            duration: 0,
            language: LanguageCode::UNDEFINED,
        };
        let encoded = mdhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = MdhdBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = elst.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = ElstBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = elst.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = ElstBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = elst.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = ElstBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = elst.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = ElstBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = edts.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = EdtsBox::decode(&encoded).expect("デコードは失敗しない");
        assert!(decoded.elst_box.is_some());
    }

    /// EdtsBox: elst_box なし
    #[test]
    fn edts_box_without_elst() {
        let edts = EdtsBox {
            elst_box: None,
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = EdtsBox::decode(&encoded).expect("デコードは失敗しない");
        assert!(decoded.elst_box.is_none());
    }

    // ===== UrlBox のテスト =====

    /// UrlBox: location あり
    #[test]
    fn url_box_with_location() {
        let url = UrlBox {
            location: Utf8String::new("http://example.com"),
        };
        let encoded = url.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = UrlBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = url.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = UrlBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = dref.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = DrefBox::decode(&encoded).expect("デコードは失敗しない");
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
                                .expect("data_reference_index は非ゼロである"),
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
        let encoded = minf.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = MinfBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = hdlr.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = HdlrBox::decode(&encoded).expect("デコードは失敗しない");
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
        let encoded = vmhd.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = VmhdBox::decode(&encoded).expect("デコードは失敗しない");
        assert_eq!(decoded.graphicsmode, 100);
        assert_eq!(decoded.opcolor, [255, 128, 64]);
    }

    // ===== StszBox のテスト =====

    /// StszBox: Fixed サイズ
    #[test]
    fn stsz_box_fixed_size() {
        let stsz = StszBox::Fixed {
            sample_size: NonZeroU32::new(1024).expect("sample_size は非ゼロである"),
            sample_count: 100,
        };
        let encoded = stsz.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = StszBox::decode(&encoded).expect("デコードは失敗しない");
        match decoded {
            StszBox::Fixed {
                sample_size,
                sample_count,
            } => {
                assert_eq!(sample_size.get(), 1024);
                assert_eq!(sample_count, 100);
            }
            _ => panic!("Fixed バリアントを期待した"),
        }
    }

    // ===== Co64Box のテスト =====

    /// Co64Box: 大きなオフセット値
    #[test]
    fn co64_box_large_offsets() {
        let co64 = Co64Box {
            chunk_offsets: vec![u32::MAX as u64 + 1, u64::MAX / 2],
        };
        let encoded = co64.encode_to_vec().expect("エンコードは失敗しない");
        let (decoded, _) = Co64Box::decode(&encoded).expect("デコードは失敗しない");
        assert_eq!(decoded.chunk_offsets.len(), 2);
        assert_eq!(decoded.chunk_offsets[0], u32::MAX as u64 + 1);
    }
}

// ===== BaseBox トレイトの実装テスト (boxes_moov_tree.rs 系) =====

mod moov_tree_base_box_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        BaseBox, BoxType, Either, FixedPointNumber, LanguageCode, Mp4FileTime,
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
            timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
            duration: 1000,
            language: LanguageCode::UNDEFINED,
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
            timescale: NonZeroU32::new(1000).expect("timescale は非ゼロである"),
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
                timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
                duration: 1000,
                language: LanguageCode::UNDEFINED,
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

// ===== pbt/tests/prop_boxes.rs の boundary_tests から移動 =====

// ===== 境界値テスト =====

mod prop_boxes_boundary_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        Decode, Encode, FixedPointNumber, LanguageCode, Mp4FileTime,
        boxes::{
            Brand, Co64Box, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry, FtypBox, HdlrBox,
            MdhdBox, MvhdBox, SmhdBox, StcoBox, StscBox, StscEntry, StssBox, SttsBox, SttsEntry,
            TkhdBox, UrlBox, VmhdBox,
        },
    };

    /// SttsBox: 空のエントリリスト
    #[test]
    fn stts_box_empty() {
        let stts = SttsBox { entries: vec![] };
        let encoded = stts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SttsBox::decode(&encoded)
            .expect("直前にエンコードした有効な SttsBox は必ずデコードできる");
        assert!(decoded.entries.is_empty());
    }

    /// StcoBox: 空のオフセットリスト
    #[test]
    fn stco_box_empty() {
        let stco = StcoBox {
            chunk_offsets: vec![],
        };
        let encoded = stco.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = StcoBox::decode(&encoded)
            .expect("直前にエンコードした有効な StcoBox は必ずデコードできる");
        assert!(decoded.chunk_offsets.is_empty());
    }

    /// Co64Box: 空のオフセットリスト
    #[test]
    fn co64_box_empty() {
        let co64 = Co64Box {
            chunk_offsets: vec![],
        };
        let encoded = co64.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Co64Box::decode(&encoded)
            .expect("直前にエンコードした有効な Co64Box は必ずデコードできる");
        assert!(decoded.chunk_offsets.is_empty());
    }

    /// ElstBox: 空のエントリリスト
    #[test]
    fn elst_box_empty() {
        let elst = ElstBox { entries: vec![] };
        let encoded = elst.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = ElstBox::decode(&encoded)
            .expect("直前にエンコードした有効な ElstBox は必ずデコードできる");
        assert!(decoded.entries.is_empty());
    }

    /// SttsEntry: 最大値
    #[test]
    fn stts_entry_max_values() {
        let stts = SttsBox {
            entries: vec![SttsEntry {
                sample_count: u32::MAX,
                sample_delta: u32::MAX,
            }],
        };
        let encoded = stts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SttsBox::decode(&encoded)
            .expect("直前にエンコードした有効な SttsBox は必ずデコードできる");
        assert_eq!(decoded.entries[0].sample_count, u32::MAX);
        assert_eq!(decoded.entries[0].sample_delta, u32::MAX);
    }

    /// StscEntry: 最小値 (NonZeroU32 の制約)
    #[test]
    fn stsc_entry_min_values() {
        let stsc = StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::new(1).expect("1 は非ゼロ"),
                sample_per_chunk: 0,
                sample_description_index: NonZeroU32::new(1).expect("1 は非ゼロ"),
            }],
        };
        let encoded = stsc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = StscBox::decode(&encoded)
            .expect("直前にエンコードした有効な StscBox は必ずデコードできる");
        assert_eq!(decoded.entries[0].first_chunk.get(), 1);
        assert_eq!(decoded.entries[0].sample_per_chunk, 0);
        assert_eq!(decoded.entries[0].sample_description_index.get(), 1);
    }

    /// Co64Box: u64 最大値
    #[test]
    fn co64_box_max_offset() {
        let co64 = Co64Box {
            chunk_offsets: vec![u64::MAX],
        };
        let encoded = co64.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = Co64Box::decode(&encoded)
            .expect("直前にエンコードした有効な Co64Box は必ずデコードできる");
        assert_eq!(decoded.chunk_offsets[0], u64::MAX);
    }

    /// ElstEntry: version 0 と version 1 の境界
    #[test]
    fn elst_entry_version_boundary() {
        // version 0 の最大値
        let elst_v0 = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: u32::MAX as u64,
                media_time: i32::MAX as i64,
                media_rate: FixedPointNumber::new(i16::MAX, i16::MAX),
            }],
        };
        let encoded_v0 = elst_v0
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        let (decoded_v0, _) = ElstBox::decode(&encoded_v0)
            .expect("直前にエンコードした v0 の有効な ElstBox は必ずデコードできる");
        assert_eq!(decoded_v0.entries[0].edit_duration, u32::MAX as u64);

        // version 1 が必要な値
        let elst_v1 = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: (u32::MAX as u64) + 1,
                media_time: (i32::MAX as i64) + 1,
                media_rate: FixedPointNumber::new(0, 0),
            }],
        };
        let encoded_v1 = elst_v1
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        let (decoded_v1, _) = ElstBox::decode(&encoded_v1)
            .expect("直前にエンコードした v1 の有効な ElstBox は必ずデコードできる");
        assert_eq!(decoded_v1.entries[0].edit_duration, (u32::MAX as u64) + 1);
    }

    /// FtypBox: ブランドの境界値
    #[test]
    fn ftyp_box_brand_boundary() {
        let ftyp = FtypBox {
            major_brand: Brand::new([0x00, 0x00, 0x00, 0x00]),
            minor_version: 0,
            compatible_brands: vec![Brand::new([0xFF, 0xFF, 0xFF, 0xFF])],
        };
        let encoded = ftyp.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = FtypBox::decode(&encoded)
            .expect("直前にエンコードした有効な FtypBox は必ずデコードできる");
        assert_eq!(decoded.major_brand.get(), [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(decoded.compatible_brands[0].get(), [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    /// MvhdBox: デフォルト値
    #[test]
    fn mvhd_box_defaults() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("1000 は非ゼロ"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MvhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvhdBox は必ずデコードできる");
        assert_eq!(decoded.rate.integer, 1);
        assert_eq!(decoded.rate.fraction, 0);
        assert_eq!(decoded.volume.integer, 1);
        assert_eq!(decoded.volume.fraction, 0);
        assert_eq!(decoded.matrix, MvhdBox::DEFAULT_MATRIX);
    }

    /// TkhdBox: フラグの組み合わせ
    #[test]
    fn tkhd_box_flags() {
        let tkhd = TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: true,
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            track_id: 1,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(1920, 0),
            height: FixedPointNumber::new(1080, 0),
        };
        let encoded = tkhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = TkhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TkhdBox は必ずデコードできる");
        assert!(decoded.flag_track_enabled);
        assert!(decoded.flag_track_in_movie);
        assert!(!decoded.flag_track_in_preview);
        assert!(decoded.flag_track_size_is_aspect_ratio);
    }

    /// MdhdBox: `LanguageCode` の受理境界と代表値の encode/decode
    ///
    /// `0x60` / `0x7F` は 5 ビットパックの下限・上限（code = 0 / 31）。
    /// `0x61` / `0x7A` は ISO-639-2/T の文字集合（`a-z`）の端。
    #[test]
    fn mdhd_box_language_boundary() {
        fn roundtrip(language: LanguageCode) {
            let mdhd = MdhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(48000).expect("48000 は非ゼロ"),
                duration: 0,
                language,
            };
            let encoded = mdhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = MdhdBox::decode(&encoded)
                .expect("直前にエンコードした有効な MdhdBox は必ずデコードできる");
            assert_eq!(decoded.language, language);
        }

        // LanguageCode の下限（5 ビット code = 0）
        roundtrip(LanguageCode::new([0x60, 0x60, 0x60]).expect("0x60 は範囲内"));
        // LanguageCode の上限（5 ビット code = 31）
        roundtrip(LanguageCode::new([0x7F, 0x7F, 0x7F]).expect("0x7F は範囲内"));
        // ISO-639-2/T の文字集合の下限 'aaa'
        roundtrip(LanguageCode::new([0x61, 0x61, 0x61]).expect("0x61 は範囲内"));
        // ISO-639-2/T の文字集合の上限 'zzz'
        roundtrip(LanguageCode::new([0x7A, 0x7A, 0x7A]).expect("0x7A は範囲内"));
        roundtrip(LanguageCode::UNDEFINED);
    }

    /// HdlrBox: 空の name
    #[test]
    fn hdlr_box_empty_name() {
        let hdlr = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: vec![],
        };
        let encoded = hdlr.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = HdlrBox::decode(&encoded)
            .expect("直前にエンコードした有効な HdlrBox は必ずデコードできる");
        assert_eq!(decoded.handler_type, *b"vide");
        assert!(decoded.name.is_empty());
    }

    /// HdlrBox: ハンドラータイプ
    #[test]
    fn hdlr_box_handler_types() {
        for handler_type in [HdlrBox::HANDLER_TYPE_SOUN, HdlrBox::HANDLER_TYPE_VIDE] {
            let hdlr = HdlrBox {
                handler_type,
                name: b"test\0".to_vec(),
            };
            let encoded = hdlr.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = HdlrBox::decode(&encoded)
                .expect("直前にエンコードした有効な HdlrBox は必ずデコードできる");
            assert_eq!(decoded.handler_type, handler_type);
        }
    }

    /// SmhdBox: デフォルト値
    #[test]
    fn smhd_box_default() {
        let smhd = SmhdBox {
            balance: SmhdBox::DEFAULT_BALANCE,
        };
        let encoded = smhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SmhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な SmhdBox は必ずデコードできる");
        assert_eq!(decoded.balance.integer, 0);
        assert_eq!(decoded.balance.fraction, 0);
    }

    /// VmhdBox: デフォルト値
    #[test]
    fn vmhd_box_default() {
        let vmhd = VmhdBox {
            graphicsmode: VmhdBox::DEFAULT_GRAPHICSMODE,
            opcolor: VmhdBox::DEFAULT_OPCOLOR,
        };
        let encoded = vmhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = VmhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な VmhdBox は必ずデコードできる");
        assert_eq!(decoded.graphicsmode, 0);
        assert_eq!(decoded.opcolor, [0, 0, 0]);
    }

    /// StssBox: 空のリスト
    #[test]
    fn stss_box_empty() {
        let stss = StssBox {
            sample_numbers: vec![],
        };
        let encoded = stss.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = StssBox::decode(&encoded)
            .expect("直前にエンコードした有効な StssBox は必ずデコードできる");
        assert!(decoded.sample_numbers.is_empty());
    }

    /// StssBox: 最大値
    #[test]
    fn stss_box_max_value() {
        let stss = StssBox {
            sample_numbers: vec![NonZeroU32::MAX],
        };
        let encoded = stss.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = StssBox::decode(&encoded)
            .expect("直前にエンコードした有効な StssBox は必ずデコードできる");
        assert_eq!(decoded.sample_numbers[0], NonZeroU32::MAX);
    }

    /// UrlBox: ローカルファイル
    #[test]
    fn url_box_local_file() {
        let url = UrlBox::LOCAL_FILE;
        let encoded = url.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = UrlBox::decode(&encoded)
            .expect("直前にエンコードした有効な UrlBox は必ずデコードできる");
        assert!(decoded.location.is_none());
    }

    /// DrefBox: ローカルファイル
    #[test]
    fn dref_box_local_file() {
        let dref = DrefBox::LOCAL_FILE;
        let encoded = dref.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DrefBox::decode(&encoded)
            .expect("直前にエンコードした有効な DrefBox は必ずデコードできる");
        assert!(decoded.url_box.is_some());
        assert!(
            decoded
                .url_box
                .expect("直前の is_some 検証で Some であることを確認済み")
                .location
                .is_none()
        );
    }

    /// DinfBox: ローカルファイル
    #[test]
    fn dinf_box_local_file() {
        let dinf = DinfBox::LOCAL_FILE;
        let encoded = dinf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DinfBox::decode(&encoded)
            .expect("直前にエンコードした有効な DinfBox は必ずデコードできる");
        assert!(decoded.dref_box.url_box.is_some());
        assert!(decoded.unknown_boxes.is_empty());
    }

    /// EdtsBox: 空
    #[test]
    fn edts_box_empty() {
        let edts = EdtsBox {
            elst_box: None,
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = EdtsBox::decode(&encoded)
            .expect("直前にエンコードした有効な EdtsBox は必ずデコードできる");
        assert!(decoded.elst_box.is_none());
        assert!(decoded.unknown_boxes.is_empty());
    }
}

// ===== pbt/tests/prop_container_boxes.rs の boundary_tests から移動 =====

mod prop_container_boundary_tests {
    use std::num::{NonZeroU16, NonZeroU32};

    use shiguredo_mp4::{
        BoxSize, BoxType, Decode, Either, Encode, FixedPointNumber, LanguageCode, Mp4FileTime,
        SampleFlags, TrackKind, Utf8String,
        boxes::{
            AudioSampleEntryFields, BoxRecord, Brand, DinfBox, DopsBox, FtabBox, FtypBox, HdlrBox,
            MdhdBox, MdiaBox, MediaHeader, MinfBox, MoovBox, MvexBox, MvhdBox, NmhdBox, OpusBox,
            SampleEntry, SmhdBox, StblBox, StcoBox, SthdBox, StppBox, StscBox, StsdBox, StszBox,
            SttsBox, StyleRecord, TkhdBox, TrakBox, TrexBox, Tx3gBox, UnknownBox, VmhdBox, VttCBox,
            WvttBox,
        },
        demux::{Fmp4FileDemuxer, Fmp4SegmentDemuxer, Input, Mp4FileDemuxer},
        mux::{Fmp4SegmentMuxer, Mp4FileMuxer, Sample},
    };

    // ===== 最小限の構成を生成する関数 =====

    /// 最小限の MvhdBox を生成
    fn minimal_mvhd_box() -> MvhdBox {
        MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).expect("1000 は非ゼロ"),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        }
    }

    /// 最小限の TkhdBox を生成
    fn minimal_tkhd_box(track_id: u32) -> TkhdBox {
        TkhdBox {
            flag_track_enabled: true,
            flag_track_in_movie: true,
            flag_track_in_preview: false,
            flag_track_size_is_aspect_ratio: false,
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            track_id,
            duration: 0,
            layer: TkhdBox::DEFAULT_LAYER,
            alternate_group: TkhdBox::DEFAULT_ALTERNATE_GROUP,
            volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
            matrix: TkhdBox::DEFAULT_MATRIX,
            width: FixedPointNumber::new(0, 0),
            height: FixedPointNumber::new(0, 0),
        }
    }

    /// 最小限の MdhdBox を生成
    fn minimal_mdhd_box() -> MdhdBox {
        MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).expect("48000 は非ゼロ"),
            duration: 0,
            language: LanguageCode::UNDEFINED,
        }
    }

    /// 最小限の HdlrBox (audio) を生成
    fn minimal_hdlr_box_audio() -> HdlrBox {
        HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_SOUN,
            name: vec![],
        }
    }

    /// 最小限の SmhdBox を生成
    fn minimal_smhd_box() -> SmhdBox {
        SmhdBox {
            balance: SmhdBox::DEFAULT_BALANCE,
        }
    }

    /// 最小限の DinfBox を生成
    fn minimal_dinf_box() -> DinfBox {
        DinfBox::LOCAL_FILE
    }

    /// 最小限の SttsBox を生成
    fn minimal_stts_box() -> SttsBox {
        SttsBox { entries: vec![] }
    }

    /// 最小限の StscBox を生成
    fn minimal_stsc_box() -> StscBox {
        StscBox { entries: vec![] }
    }

    /// 最小限の StszBox を生成
    fn minimal_stsz_box() -> StszBox {
        StszBox::Variable {
            entry_sizes: vec![],
        }
    }

    /// 最小限の StcoBox を生成
    fn minimal_stco_box() -> StcoBox {
        StcoBox {
            chunk_offsets: vec![],
        }
    }

    /// 最小限の OpusBox を生成
    fn minimal_opus_box() -> OpusBox {
        OpusBox {
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
        }
    }

    /// 最小限の StsdBox (audio) を生成
    fn minimal_stsd_box_audio() -> StsdBox {
        StsdBox {
            entries: vec![SampleEntry::Opus(minimal_opus_box())],
        }
    }

    /// 最小限の StblBox (audio) を生成
    fn minimal_stbl_box_audio() -> StblBox {
        StblBox {
            stsd_box: minimal_stsd_box_audio(),
            stts_box: minimal_stts_box(),
            ctts_box: None,
            cslg_box: None,
            stsc_box: minimal_stsc_box(),
            stsz_box: minimal_stsz_box(),
            stco_or_co64_box: Either::A(minimal_stco_box()),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: vec![],
        }
    }

    /// 最小限の MinfBox (audio) を生成
    fn minimal_minf_box_audio() -> MinfBox {
        MinfBox {
            media_header: Some(MediaHeader::Smhd(minimal_smhd_box())),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        }
    }

    /// 最小限の MdiaBox (audio) を生成
    fn minimal_mdia_box_audio() -> MdiaBox {
        MdiaBox {
            mdhd_box: minimal_mdhd_box(),
            hdlr_box: minimal_hdlr_box_audio(),
            minf_box: minimal_minf_box_audio(),
            unknown_boxes: vec![],
        }
    }

    /// 最小限の TrakBox (audio) を生成
    fn minimal_trak_box_audio(track_id: u32) -> TrakBox {
        TrakBox {
            tkhd_box: minimal_tkhd_box(track_id),
            edts_box: None,
            mdia_box: minimal_mdia_box_audio(),
            unknown_boxes: vec![],
        }
    }

    /// 最小限の MoovBox を生成
    fn minimal_moov_box() -> MoovBox {
        MoovBox {
            mvhd_box: minimal_mvhd_box(),
            trak_boxes: vec![minimal_trak_box_audio(1)],
            mvex_box: None,
            unknown_boxes: vec![],
        }
    }

    /// 最小限の HdlrBox (subtitle) を生成
    ///
    /// `handler_type` に `subt` / `text` を渡すことで stpp / wvtt / tx3g 相当の
    /// トラックに対応する HdlrBox を作れる
    fn minimal_hdlr_box_subtitle(handler_type: [u8; 4]) -> HdlrBox {
        HdlrBox {
            handler_type,
            name: vec![],
        }
    }

    /// 最小限の StsdBox (subtitle) を生成
    ///
    /// stsd 内に SampleEntry を 1 つ持つ。stpp / wvtt / tx3g は型付き実装のため
    /// それぞれ `SampleEntry::Stpp` / `SampleEntry::Wvtt` / `SampleEntry::Tx3g` の最小構成を使う。
    /// `sample_entry_box_type` に `stpp` / `wvtt` / `tx3g` を渡して切り替える。
    /// それ以外の box_type は `SampleEntry::Unknown` フォールバック経路として扱う
    fn minimal_stsd_box_subtitle(sample_entry_box_type: [u8; 4]) -> StsdBox {
        let entry = if sample_entry_box_type == *b"stpp" {
            SampleEntry::Stpp(StppBox {
                data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
                namespace: Utf8String::EMPTY,
                schema_location: Utf8String::EMPTY,
                auxiliary_mime_types: Utf8String::EMPTY,
                unknown_boxes: vec![],
            })
        } else if sample_entry_box_type == *b"wvtt" {
            SampleEntry::Wvtt(WvttBox {
                data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
                vttc_box: VttCBox {
                    config: String::from("WEBVTT"),
                },
                unknown_boxes: vec![],
            })
        } else if sample_entry_box_type == *b"tx3g" {
            SampleEntry::Tx3g(Tx3gBox {
                data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
                display_flags: 0,
                horizontal_justification: 0,
                vertical_justification: 0,
                background_color_rgba: [0, 0, 0, 0],
                default_text_box: BoxRecord::default(),
                default_style: StyleRecord::default(),
                ftab_box: FtabBox::default(),
                unknown_boxes: vec![],
            })
        } else {
            SampleEntry::Unknown(UnknownBox {
                box_type: BoxType::Normal(sample_entry_box_type),
                box_size: BoxSize::U32(8),
                payload: vec![],
            })
        };
        StsdBox {
            entries: vec![entry],
        }
    }

    /// 最小限の StblBox (subtitle) を生成
    fn minimal_stbl_box_subtitle(sample_entry_box_type: [u8; 4]) -> StblBox {
        StblBox {
            stsd_box: minimal_stsd_box_subtitle(sample_entry_box_type),
            stts_box: minimal_stts_box(),
            ctts_box: None,
            cslg_box: None,
            stsc_box: minimal_stsc_box(),
            stsz_box: minimal_stsz_box(),
            stco_or_co64_box: Either::A(minimal_stco_box()),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: vec![],
        }
    }

    /// 最小限の MinfBox (subtitle) を生成
    ///
    /// Media Header には `SthdBox` を使う（`Fmp4SegmentMuxer` の暫定選択と同じ）。
    /// 方式固有 SampleEntry の実装時に必要に応じて `NmhdBox` に切り替える形へリファクタする想定
    fn minimal_minf_box_subtitle(sample_entry_box_type: [u8; 4]) -> MinfBox {
        MinfBox {
            media_header: Some(MediaHeader::Sthd(SthdBox)),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_subtitle(sample_entry_box_type),
            unknown_boxes: vec![],
        }
    }

    /// 最小限の MdiaBox (subtitle) を生成
    fn minimal_mdia_box_subtitle(handler_type: [u8; 4], sample_entry_box_type: [u8; 4]) -> MdiaBox {
        MdiaBox {
            mdhd_box: minimal_mdhd_box(),
            hdlr_box: minimal_hdlr_box_subtitle(handler_type),
            minf_box: minimal_minf_box_subtitle(sample_entry_box_type),
            unknown_boxes: vec![],
        }
    }

    /// 最小限の TrakBox (subtitle) を生成
    ///
    /// - `track_id`: TkhdBox に設定するトラック ID
    /// - `handler_type`: `subt` (stpp 用) または `text` (wvtt / tx3g 用) の 4 バイト
    /// - `sample_entry_box_type`: stsd 内 Unknown SampleEntry の box_type（`stpp` / `wvtt` / `tx3g`）
    fn minimal_trak_box_subtitle(
        track_id: u32,
        handler_type: [u8; 4],
        sample_entry_box_type: [u8; 4],
    ) -> TrakBox {
        TrakBox {
            tkhd_box: minimal_tkhd_box(track_id),
            edts_box: None,
            mdia_box: minimal_mdia_box_subtitle(handler_type, sample_entry_box_type),
            unknown_boxes: vec![],
        }
    }

    // ===== 境界値テスト =====

    mod boundary_tests {
        use super::*;

        /// MoovBox: 最小構成
        #[test]
        fn moov_box_minimal() {
            let moov = minimal_moov_box();
            let encoded = moov.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = MoovBox::decode(&encoded)
                .expect("直前にエンコードした有効な MoovBox は必ずデコードできる");
            assert_eq!(decoded.trak_boxes.len(), 1);
        }

        /// SthdBox: encode/decode roundtrip
        ///
        /// SthdBox はペイロードを持たないため、encode→decode で同一の値が復元されることのみを確認する
        #[test]
        fn sthd_box_roundtrip() {
            let sthd = SthdBox;
            let encoded = sthd.encode_to_vec().expect("sthd の encode に失敗した");
            let (decoded, size) = SthdBox::decode(&encoded).expect("sthd の decode に失敗した");
            assert_eq!(size, encoded.len());
            assert_eq!(decoded, sthd);
        }

        /// NmhdBox: encode/decode roundtrip
        ///
        /// NmhdBox もペイロードを持たないため、encode→decode で同一の値が復元されることのみを確認する
        #[test]
        fn nmhd_box_roundtrip() {
            let nmhd = NmhdBox;
            let encoded = nmhd.encode_to_vec().expect("nmhd の encode に失敗した");
            let (decoded, size) = NmhdBox::decode(&encoded).expect("nmhd の decode に失敗した");
            assert_eq!(size, encoded.len());
            assert_eq!(decoded, nmhd);
        }

        /// `HdlrBox` の字幕用ハンドラー種別定数が仕様通りのバイト列であることを検証する
        ///
        /// テスト側の合成 MP4 では生バイト列 `b"subt"` / `b"text"` を渡す形になっており、
        /// 定数側が誤って書き換わっても demuxer 側だけ壊れて他のテストは pass するリスクがある。
        /// spec 値との一致を明示的にアサートしておく
        #[test]
        fn hdlr_box_subtitle_handler_type_constants() {
            assert_eq!(HdlrBox::HANDLER_TYPE_SUBT, *b"subt");
            assert_eq!(HdlrBox::HANDLER_TYPE_TEXT, *b"text");
        }

        /// `MediaHeader::decode` に既知でない box_type を渡すとエラーを返すことを検証する
        ///
        /// `MediaHeader` は `SampleEntry` のような Unknown フォールバックを持たない。
        /// smhd / vmhd / sthd / nmhd 以外の box_type が来た場合、防衛的にエラーを返す
        /// 挙動が意図した設計。回帰で誤ってフォールバックに戻す変更を検出できるよう明示テストする
        #[test]
        fn media_header_decode_rejects_unknown_box_type() {
            // 4 種のいずれでもない box_type ("hdlr") を持つ最小 box を組み立てる
            // BoxHeader レイアウト: size (u32, big-endian, 8 bytes) + type ([u8; 4] = "hdlr")
            let bytes: [u8; 8] = [0, 0, 0, 8, b'h', b'd', b'l', b'r'];
            let result = MediaHeader::decode(&bytes);
            assert!(
                result.is_err(),
                "MediaHeader::decode は未知の box_type に対して Err を返すべきだが Ok が返った"
            );
        }

        /// `MinfBox::decode` は Media Header が複数現れた場合、最初のものを採用する
        ///
        /// 仕様上 minf 直下には Media Header は 1 種類しか出ないが、
        /// 異常入力（vmhd → smhd の順で 2 個並ぶ等）が来た場合、
        /// 実装は「最初に見つかったもの」を採用し、後続は `unknown_boxes` に落とすことを担保する。
        /// 旧実装の「smhd 優先」から挙動が変わっているため、この宣言的挙動をテストで固定する
        #[test]
        fn minf_box_decode_uses_first_media_header() {
            // 個別に box をエンコードして minf の内容として直列に並べる
            let vmhd_bytes = VmhdBox::default()
                .encode_to_vec()
                .expect("vmhd の encode に失敗した");
            let smhd_bytes = SmhdBox::default()
                .encode_to_vec()
                .expect("smhd の encode に失敗した");
            let dinf_bytes = minimal_dinf_box()
                .encode_to_vec()
                .expect("dinf の encode に失敗した");
            let stbl_bytes = minimal_stbl_box_audio()
                .encode_to_vec()
                .expect("stbl の encode に失敗した");

            // minf 内容: vmhd → smhd → dinf → stbl（vmhd が Media Header として最初）
            let mut inner = Vec::new();
            inner.extend_from_slice(&vmhd_bytes);
            inner.extend_from_slice(&smhd_bytes);
            inner.extend_from_slice(&dinf_bytes);
            inner.extend_from_slice(&stbl_bytes);

            // minf ヘッダー (size u32 + type "minf") を先頭に付ける
            let box_size = u32::try_from(8 + inner.len()).expect("minf サイズは u32 に収まる");
            let mut bytes = Vec::new();
            bytes.extend_from_slice(&box_size.to_be_bytes());
            bytes.extend_from_slice(b"minf");
            bytes.extend_from_slice(&inner);

            let (decoded, _) = MinfBox::decode(&bytes).expect("minf の decode に失敗した");

            // 最初に現れた vmhd が採用される
            assert!(
                matches!(decoded.media_header, Some(MediaHeader::Vmhd(_))),
                "最初に現れた vmhd が採用されるべきだが media_header = {:?}",
                decoded.media_header,
            );

            // 後続の smhd は unknown_boxes に落ちる
            assert!(
                decoded
                    .unknown_boxes
                    .iter()
                    .any(|b| b.box_type == BoxType::Normal(*b"smhd")),
                "後続の smhd が unknown_boxes に落ちていない",
            );
        }

        /// MinfBox (subtitle, sthd Media Header) の encode/decode roundtrip
        #[test]
        fn minf_box_subtitle_sthd_roundtrip() {
            let minf = MinfBox {
                media_header: Some(MediaHeader::Sthd(SthdBox)),
                dinf_box: minimal_dinf_box(),
                stbl_box: minimal_stbl_box_subtitle(*b"stpp"),
                unknown_boxes: vec![],
            };
            let encoded = minf
                .encode_to_vec()
                .expect("minf (sthd) の encode に失敗した");
            let (decoded, size) =
                MinfBox::decode(&encoded).expect("minf (sthd) の decode に失敗した");
            assert_eq!(size, encoded.len());
            assert!(matches!(
                decoded.media_header,
                Some(MediaHeader::Sthd(SthdBox))
            ));
        }

        /// MinfBox (subtitle, nmhd Media Header) の encode/decode roundtrip
        #[test]
        fn minf_box_subtitle_nmhd_roundtrip() {
            let minf = MinfBox {
                media_header: Some(MediaHeader::Nmhd(NmhdBox)),
                dinf_box: minimal_dinf_box(),
                stbl_box: minimal_stbl_box_subtitle(*b"tx3g"),
                unknown_boxes: vec![],
            };
            let encoded = minf
                .encode_to_vec()
                .expect("minf (nmhd) の encode に失敗した");
            let (decoded, size) =
                MinfBox::decode(&encoded).expect("minf (nmhd) の decode に失敗した");
            assert_eq!(size, encoded.len());
            assert!(matches!(
                decoded.media_header,
                Some(MediaHeader::Nmhd(NmhdBox))
            ));
        }

        // ===== 字幕トラックの demux roundtrip テスト =====
        //
        // 対応表:
        //   stpp サンプルエントリー → handler_type "subt"
        //   wvtt サンプルエントリー → handler_type "text"
        //   tx3g サンプルエントリー → handler_type "text"
        //
        // 上記 3 組 × 3 種のデマルチプレクサ (Mp4FileDemuxer / Fmp4FileDemuxer /
        // Fmp4SegmentDemuxer) の計 9 通りで、字幕トラックが skip されず
        // TrackKind::Subtitle として取り出せることを検証する。

        /// 対応表を返す (handler_type, sample_entry_box_type) のタプル配列
        fn subtitle_scheme_matrix() -> [([u8; 4], [u8; 4]); 3] {
            [
                (*b"subt", *b"stpp"),
                (*b"text", *b"wvtt"),
                (*b"text", *b"tx3g"),
            ]
        }

        /// 字幕トラックを 1 本だけ含む Mp4File 相当のバイト列を組み立てる
        ///
        /// ftyp + moov (subtitle trak 含む) の連結。Mp4FileDemuxer 用。
        /// mdat は無くても Mp4FileDemuxer の tracks() 取得までは進むため省略する
        fn build_mp4_file_bytes_with_subtitle(
            handler_type: [u8; 4],
            sample_entry_box_type: [u8; 4],
        ) -> Vec<u8> {
            let ftyp = FtypBox {
                major_brand: Brand::ISOM,
                minor_version: 512,
                compatible_brands: vec![Brand::ISOM],
            };
            let moov = MoovBox {
                mvhd_box: minimal_mvhd_box(),
                trak_boxes: vec![minimal_trak_box_subtitle(
                    1,
                    handler_type,
                    sample_entry_box_type,
                )],
                mvex_box: None,
                unknown_boxes: vec![],
            };
            let mut bytes = ftyp.encode_to_vec().expect("ftyp の encode に失敗した");
            bytes.extend_from_slice(&moov.encode_to_vec().expect("moov の encode に失敗した"));
            bytes
        }

        /// 字幕トラックを 1 本だけ含む fMP4 init segment 相当のバイト列を組み立てる
        ///
        /// ftyp + moov (subtitle trak + mvex/trex 含む) の連結。
        /// Fmp4FileDemuxer / Fmp4SegmentDemuxer 用
        fn build_fmp4_init_segment_bytes_with_subtitle(
            handler_type: [u8; 4],
            sample_entry_box_type: [u8; 4],
        ) -> Vec<u8> {
            let ftyp = FtypBox {
                major_brand: Brand::ISOM,
                minor_version: 512,
                compatible_brands: vec![Brand::ISOM],
            };
            let mvex = MvexBox {
                mehd_box: None,
                trex_boxes: vec![TrexBox {
                    track_id: 1,
                    default_sample_description_index: 1,
                    default_sample_duration: 0,
                    default_sample_size: 0,
                    default_sample_flags: SampleFlags::empty(),
                }],
                unknown_boxes: vec![],
            };
            let moov = MoovBox {
                mvhd_box: minimal_mvhd_box(),
                trak_boxes: vec![minimal_trak_box_subtitle(
                    1,
                    handler_type,
                    sample_entry_box_type,
                )],
                mvex_box: Some(mvex),
                unknown_boxes: vec![],
            };
            let mut bytes = ftyp.encode_to_vec().expect("ftyp の encode に失敗した");
            bytes.extend_from_slice(&moov.encode_to_vec().expect("moov の encode に失敗した"));
            bytes
        }

        /// Mp4FileDemuxer 経由で対応表 3 組すべての字幕トラックが Subtitle として取り出せる
        #[test]
        fn subtitle_track_via_mp4_file_demuxer() {
            for (handler_type, sample_entry_box_type) in subtitle_scheme_matrix() {
                let bytes = build_mp4_file_bytes_with_subtitle(handler_type, sample_entry_box_type);
                let input = Input {
                    position: 0,
                    data: &bytes,
                };
                let mut demuxer = Mp4FileDemuxer::new();
                demuxer.handle_input(input);
                let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
                assert_eq!(
                    tracks.len(),
                    1,
                    "handler_type={:?} sample_entry={:?} のトラック数が想定と異なる",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
                assert!(
                    matches!(tracks[0].kind, TrackKind::Subtitle),
                    "handler_type={:?} sample_entry={:?} が Subtitle として取り出せない",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
            }
        }

        /// Fmp4FileDemuxer 経由で対応表 3 組すべての字幕トラックが Subtitle として取り出せる
        ///
        /// Fmp4FileDemuxer は `required_input()` で段階的にデータを要求するため、
        /// バッファ全体を渡すのではなく要求に応じて `handle_input()` を繰り返す
        #[test]
        fn subtitle_track_via_fmp4_file_demuxer() {
            for (handler_type, sample_entry_box_type) in subtitle_scheme_matrix() {
                let bytes = build_fmp4_init_segment_bytes_with_subtitle(
                    handler_type,
                    sample_entry_box_type,
                );
                let mut demuxer = Fmp4FileDemuxer::new();
                while let Some(required) = demuxer.required_input() {
                    let start = required.position as usize;
                    let end = start
                        .saturating_add(required.size.unwrap_or(bytes.len().saturating_sub(start)));
                    demuxer.handle_input(Input {
                        position: required.position,
                        data: bytes.get(start..end).unwrap_or(&[]),
                    });
                }
                let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
                assert_eq!(
                    tracks.len(),
                    1,
                    "handler_type={:?} sample_entry={:?} のトラック数が想定と異なる",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
                assert!(
                    matches!(tracks[0].kind, TrackKind::Subtitle),
                    "handler_type={:?} sample_entry={:?} が Subtitle として取り出せない",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
            }
        }

        /// Fmp4SegmentDemuxer 経由で対応表 3 組すべての字幕トラックが Subtitle として取り出せる
        #[test]
        fn subtitle_track_via_fmp4_segment_demuxer() {
            for (handler_type, sample_entry_box_type) in subtitle_scheme_matrix() {
                let init_bytes = build_fmp4_init_segment_bytes_with_subtitle(
                    handler_type,
                    sample_entry_box_type,
                );
                let mut demuxer = Fmp4SegmentDemuxer::new();
                demuxer
                    .handle_init_segment(&init_bytes)
                    .expect("init セグメントの処理に失敗した");
                let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
                assert_eq!(
                    tracks.len(),
                    1,
                    "handler_type={:?} sample_entry={:?} のトラック数が想定と異なる",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
                assert!(
                    matches!(tracks[0].kind, TrackKind::Subtitle),
                    "handler_type={:?} sample_entry={:?} が Subtitle として取り出せない",
                    core::str::from_utf8(&handler_type),
                    core::str::from_utf8(&sample_entry_box_type),
                );
            }
        }

        /// Fmp4SegmentMuxer 経由で字幕トラックの init/メディアセグメントを生成し tkhd 属性を確認する
        ///
        /// Fmp4SegmentMuxer に TrackKind::Subtitle の Sample を渡して init segment を生成し、
        /// 生成された moov 内 trak の tkhd を検証する:
        /// - volume == 0 (DEFAULT_VIDEO_VOLUME)
        /// - width == 0
        /// - height == 0
        #[test]
        fn subtitle_track_mux_tkhd_via_fmp4_segment_muxer() {
            // tkhd 検証のため sample_entry の型は問わない。型付き Stpp バリアントを直接使う
            let subtitle_sample_entry = SampleEntry::Stpp(StppBox {
                data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
                namespace: Utf8String::EMPTY,
                schema_location: Utf8String::EMPTY,
                auxiliary_mime_types: Utf8String::EMPTY,
                unknown_boxes: vec![],
            });
            let sample_payload = b"hello subtitle";
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: Some(subtitle_sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(1000).expect("非ゼロである"),
                duration: 1000,
                composition_time_offset: None,
                data_offset: 0,
                data_size: sample_payload.len(),
            };

            let mut muxer = Fmp4SegmentMuxer::new().expect("muxer の作成に失敗した");
            // メディアセグメントを生成して muxer にトラック情報を蓄積させる
            // （`init_segment_bytes` は tracks が空だと `EmptyTracks` エラーになるため）
            let media_segment = muxer
                .create_media_segment_metadata(std::slice::from_ref(&sample))
                .expect("media セグメントの作成に失敗した");
            assert!(
                !media_segment.is_empty(),
                "メディアセグメントのバイト列が空になっている"
            );
            // sample_payload は本テストの tkhd 検証には不要のため付加しない

            let init_bytes = muxer
                .init_segment_bytes()
                .expect("init セグメントの構築に失敗した");

            // init segment 内の trak を検証（ftyp のあとに moov が続く前提）
            let (_ftyp, ftyp_size) =
                FtypBox::decode(&init_bytes).expect("ftyp のデコードに失敗した");
            let (moov, _moov_size) =
                MoovBox::decode(&init_bytes[ftyp_size..]).expect("moov のデコードに失敗した");

            assert_eq!(moov.trak_boxes.len(), 1);
            let trak = &moov.trak_boxes[0];

            // ハンドラー種別と Media Header の暫定選択（subt + sthd）を確認
            assert_eq!(
                trak.mdia_box.hdlr_box.handler_type,
                HdlrBox::HANDLER_TYPE_SUBT
            );
            assert!(matches!(
                trak.mdia_box.minf_box.media_header,
                Some(MediaHeader::Sthd(SthdBox))
            ));

            // tkhd の volume / width / height が字幕トラック用の値 (0, 0, 0) になっていることを確認
            assert_eq!(trak.tkhd_box.volume, TkhdBox::DEFAULT_VIDEO_VOLUME);
            assert_eq!(trak.tkhd_box.width, FixedPointNumber::new(0, 0));
            assert_eq!(trak.tkhd_box.height, FixedPointNumber::new(0, 0));
        }

        // ===== Stpp 正常経路担保テスト =====

        /// 字幕トラック用の Sample を Fmp4SegmentMuxer 経由で組み立てて
        /// init segment とメディアセグメントのバイト列を返す共通ヘルパー
        ///
        /// 既存 `pbt/tests/prop_fmp4_segment_mux_demux.rs` の `build_complete_media_segment`
        /// と同じ形の組み立て（integration test の性質上、直接再利用できないためコピー）
        fn build_subtitle_fmp4_segments(
            sample_entry: SampleEntry,
            sample_payload: &[u8],
        ) -> (Vec<u8>, Vec<u8>) {
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: Some(sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(1000).expect("非ゼロである"),
                duration: 1000,
                composition_time_offset: None,
                data_offset: 0,
                data_size: sample_payload.len(),
            };

            let mut muxer = Fmp4SegmentMuxer::new().expect("muxer の作成に失敗した");
            // メディアセグメントのメタデータを生成した後、サンプル payload を連結する
            let mut media_segment = muxer
                .create_media_segment_metadata(std::slice::from_ref(&sample))
                .expect("media セグメントメタデータの作成に失敗した");
            media_segment.extend_from_slice(sample_payload);

            let init_bytes = muxer
                .init_segment_bytes()
                .expect("init セグメントの構築に失敗した");

            (init_bytes, media_segment)
        }

        /// stpp サンプルエントリーを持つ Sample の init/メディアセグメントを組み立てる
        fn build_stpp_fmp4_segments() -> (Vec<u8>, Vec<u8>) {
            let stpp_sample_entry = SampleEntry::Stpp(StppBox {
                data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
                namespace: Utf8String::new("http://www.w3.org/ns/ttml")
                    .expect("null 文字を含まない"),
                schema_location: Utf8String::EMPTY,
                auxiliary_mime_types: Utf8String::EMPTY,
                unknown_boxes: vec![],
            });
            // TTML の最小 XML をサンプルデータとして使う
            build_subtitle_fmp4_segments(
                stpp_sample_entry,
                b"<tt xmlns=\"http://www.w3.org/ns/ttml\"/>",
            )
        }

        /// Fmp4FileDemuxer 経由で stpp サンプルエントリーが `SampleEntry::Stpp(_)` として取り出せる
        ///
        /// `TrackInfo` に `sample_entries` フィールドが無いため、`Sample.sample_entry` 経由で検証する。
        /// init segment + メディアセグメント + サンプルデータの合成バイト列を組み立てて
        /// `Fmp4FileDemuxer::next_sample()` から取り出したサンプルの `sample_entry` を検証する
        #[test]
        fn stpp_sample_entry_via_fmp4_file_demuxer() {
            let (init_bytes, media_segment) = build_stpp_fmp4_segments();
            let mut fmp4_bytes = init_bytes;
            fmp4_bytes.extend_from_slice(&media_segment);

            // Fmp4FileDemuxer は required_input()/handle_input() で段階的にデータを消費する
            let mut demuxer = Fmp4FileDemuxer::new();
            while let Some(required) = demuxer.required_input() {
                let start = required.position as usize;
                let end = start
                    .saturating_add(
                        required
                            .size
                            .unwrap_or(fmp4_bytes.len().saturating_sub(start)),
                    )
                    .min(fmp4_bytes.len());
                demuxer.handle_input(Input {
                    position: required.position,
                    data: fmp4_bytes.get(start..end).unwrap_or(&[]),
                });
            }

            // 最初のサンプルを取り出して sample_entry が Stpp バリアントであることを検証する
            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Fmp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Stpp(_)),
                "stpp サンプルエントリーが型付きで取り出せること"
            );
        }

        /// Fmp4SegmentDemuxer 経由で stpp サンプルエントリーが `SampleEntry::Stpp(_)` として取り出せる
        ///
        /// init segment を `handle_init_segment` に渡し、続いてメディアセグメントを
        /// `handle_media_segment` に渡す。戻り値の各 `Sample.sample_entry` を検証する
        #[test]
        fn stpp_sample_entry_via_fmp4_segment_demuxer() {
            let (init_bytes, media_segment) = build_stpp_fmp4_segments();

            let mut demuxer = Fmp4SegmentDemuxer::new();
            demuxer
                .handle_init_segment(&init_bytes)
                .expect("init セグメントの処理に失敗した");
            let samples = demuxer
                .handle_media_segment(&media_segment)
                .expect("media セグメントの処理に失敗した");
            assert!(
                !samples.is_empty(),
                "メディアセグメントから少なくとも 1 サンプル取り出せる"
            );
            let entry = samples[0]
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Stpp(_)),
                "stpp サンプルエントリーが型付きで取り出せること"
            );
        }

        /// 字幕トラックの `trak` が持つべき共通属性を検証する
        ///
        /// `handler_type` と `media_header` はサンプルエントリー種別ごとに異なるため引数で受け取る。
        /// tkhd の volume / width / height は 3 形式とも 0 で共通
        fn assert_subtitle_trak(
            moov: &MoovBox,
            expected_handler_type: [u8; 4],
            expected_media_header: MediaHeader,
        ) {
            assert_eq!(moov.trak_boxes.len(), 1, "字幕トラックが 1 本だけ存在する");
            let trak = &moov.trak_boxes[0];

            assert_eq!(
                trak.mdia_box.hdlr_box.handler_type, expected_handler_type,
                "ハンドラー種別が対応表どおりであること"
            );
            assert_eq!(
                trak.mdia_box.minf_box.media_header,
                Some(expected_media_header),
                "メディアヘッダーが対応表どおりであること"
            );

            // 字幕トラックの tkhd は volume / width / height がいずれも 0
            assert_eq!(trak.tkhd_box.volume, TkhdBox::DEFAULT_VIDEO_VOLUME);
            assert_eq!(trak.tkhd_box.width, FixedPointNumber::new(0, 0));
            assert_eq!(trak.tkhd_box.height, FixedPointNumber::new(0, 0));
        }

        /// 字幕トラック用 Sample を Mp4FileMuxer で 1 本マルチプレックスして
        /// MP4 バイト列と生成された moov を返す共通ヘルパー
        ///
        /// Mp4FileMuxer の initial_boxes_bytes / append_sample / finalize の流れ全体を
        /// この 1 関数にまとめる。stpp / wvtt / tx3g それぞれの構築ヘルパーから呼び出される
        fn build_subtitle_mp4_file_bytes(
            sample_entry: SampleEntry,
            payload: &[u8],
        ) -> (Vec<u8>, MoovBox) {
            let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
            let mut output: Vec<u8> = muxer.initial_boxes_bytes().to_vec();
            let data_offset = output.len() as u64;
            output.extend_from_slice(payload);

            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: Some(sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(1000).expect("非ゼロである"),
                duration: 1000,
                composition_time_offset: None,
                data_offset,
                data_size: payload.len(),
            };
            muxer
                .append_sample(&sample)
                .expect("sample の追加に失敗した");
            let finalized = muxer.finalize().expect("finalize に失敗した");
            let moov_box = finalized.moov_box().clone();

            // moov などの書き戻し範囲を先に計算して output を事前拡張する
            let max_end = finalized
                .offset_and_bytes_pairs()
                .map(|(offset, bytes)| offset as usize + bytes.len())
                .max()
                .unwrap_or(output.len());
            if max_end > output.len() {
                output.resize(max_end, 0);
            }
            for (offset, bytes) in finalized.offset_and_bytes_pairs() {
                let start = offset as usize;
                output[start..start + bytes.len()].copy_from_slice(bytes);
            }
            (output, moov_box)
        }

        /// stpp サンプルエントリーを持つ Sample を Mp4FileMuxer で 1 本マルチプレックスする
        fn build_stpp_mp4_file_bytes() -> (Vec<u8>, MoovBox) {
            let sample_entry = SampleEntry::Stpp(StppBox {
                data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
                namespace: Utf8String::new("http://www.w3.org/ns/ttml")
                    .expect("null 文字を含まない"),
                schema_location: Utf8String::EMPTY,
                auxiliary_mime_types: Utf8String::EMPTY,
                unknown_boxes: vec![],
            });
            let payload: &[u8] = b"<tt xmlns=\"http://www.w3.org/ns/ttml\"/>";
            build_subtitle_mp4_file_bytes(sample_entry, payload)
        }

        /// Mp4FileMuxer が stpp 用の trak を組み立て、Mp4FileDemuxer で型付きに取り出せる
        ///
        /// サンプルエントリーのバリアントだけを見ると、`hdlr` / `minf.media_header` が
        /// 壊れていても Mp4FileDemuxer は `subt` / `text` を区別せず字幕として復元してしまうため、
        /// muxer が生成した moov 側の属性も検証する
        #[test]
        fn stpp_sample_entry_via_mp4_file_demuxer() {
            let (mp4_bytes, moov_box) = build_stpp_mp4_file_bytes();

            assert_subtitle_trak(
                &moov_box,
                HdlrBox::HANDLER_TYPE_SUBT,
                MediaHeader::Sthd(SthdBox),
            );

            let mut demuxer = Mp4FileDemuxer::new();
            demuxer.handle_input(Input {
                position: 0,
                data: &mp4_bytes,
            });

            let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
            assert_eq!(tracks.len(), 1);
            assert!(
                matches!(tracks[0].kind, TrackKind::Subtitle),
                "字幕トラックとして復元されること"
            );

            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Mp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Stpp(_)),
                "stpp サンプルエントリーが型付きで取り出せること"
            );
        }

        // ===== Wvtt 正常経路担保テスト =====

        /// Fmp4SegmentMuxer 経由で wvtt トラックの init/メディアセグメントを生成し tkhd 属性を確認する
        ///
        /// wvtt は ISO/IEC 14496-30 の対応表で handler_type = `text`（stpp の `subt` と異なる）と規定されるため、
        /// `derive_trak_attributes` の Wvtt arm が正しく動作するかの回帰テストとしても機能する。
        /// 併せて Media Header が `sthd`、tkhd 属性が字幕トラック用の (0, 0, 0) になっていることも確認する
        #[test]
        fn subtitle_track_mux_tkhd_via_fmp4_segment_muxer_wvtt() {
            let wvtt_sample_entry = SampleEntry::Wvtt(WvttBox {
                data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
                vttc_box: VttCBox {
                    config: String::from("WEBVTT"),
                },
                unknown_boxes: vec![],
            });
            let sample_payload = b"hello subtitle";
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: Some(wvtt_sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(1000).expect("非ゼロである"),
                duration: 1000,
                composition_time_offset: None,
                data_offset: 0,
                data_size: sample_payload.len(),
            };

            let mut muxer = Fmp4SegmentMuxer::new().expect("muxer の作成に失敗した");
            // メディアセグメントを生成して muxer にトラック情報を蓄積させる
            let media_segment = muxer
                .create_media_segment_metadata(std::slice::from_ref(&sample))
                .expect("media セグメントの作成に失敗した");
            assert!(
                !media_segment.is_empty(),
                "メディアセグメントのバイト列が空になっている"
            );

            let init_bytes = muxer
                .init_segment_bytes()
                .expect("init セグメントの構築に失敗した");

            let (_ftyp, ftyp_size) =
                FtypBox::decode(&init_bytes).expect("ftyp のデコードに失敗した");
            let (moov, _moov_size) =
                MoovBox::decode(&init_bytes[ftyp_size..]).expect("moov のデコードに失敗した");

            assert_eq!(moov.trak_boxes.len(), 1);
            let trak = &moov.trak_boxes[0];

            // wvtt はハンドラー種別 `text` + `sthd` が対応表
            assert_eq!(
                trak.mdia_box.hdlr_box.handler_type,
                HdlrBox::HANDLER_TYPE_TEXT
            );
            assert!(matches!(
                trak.mdia_box.minf_box.media_header,
                Some(MediaHeader::Sthd(SthdBox))
            ));

            // tkhd は字幕トラック用の (0, 0, 0) で stpp 版と同じ
            assert_eq!(trak.tkhd_box.volume, TkhdBox::DEFAULT_VIDEO_VOLUME);
            assert_eq!(trak.tkhd_box.width, FixedPointNumber::new(0, 0));
            assert_eq!(trak.tkhd_box.height, FixedPointNumber::new(0, 0));
        }

        /// wvtt サンプルエントリーを持つ Sample の init/メディアセグメントを組み立てる
        ///
        /// sample payload は任意のバイト列で、Fmp4SegmentMuxer は payload 内部構造を検証しない
        /// （既存 stpp テストも TTML 断片を任意バイト列扱い）
        fn build_wvtt_fmp4_segments() -> (Vec<u8>, Vec<u8>) {
            let wvtt_sample_entry = SampleEntry::Wvtt(WvttBox {
                data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
                vttc_box: VttCBox {
                    config: String::from("WEBVTT"),
                },
                unknown_boxes: vec![],
            });
            build_subtitle_fmp4_segments(wvtt_sample_entry, b"WEBVTT-cue-payload-placeholder")
        }

        /// Fmp4FileDemuxer 経由で wvtt サンプルエントリーが `SampleEntry::Wvtt(_)` として取り出せる
        #[test]
        fn wvtt_sample_entry_via_fmp4_file_demuxer() {
            let (init_bytes, media_segment) = build_wvtt_fmp4_segments();
            let mut fmp4_bytes = init_bytes;
            fmp4_bytes.extend_from_slice(&media_segment);

            let mut demuxer = Fmp4FileDemuxer::new();
            while let Some(required) = demuxer.required_input() {
                let start = required.position as usize;
                let end = start
                    .saturating_add(
                        required
                            .size
                            .unwrap_or(fmp4_bytes.len().saturating_sub(start)),
                    )
                    .min(fmp4_bytes.len());
                demuxer.handle_input(Input {
                    position: required.position,
                    data: fmp4_bytes.get(start..end).unwrap_or(&[]),
                });
            }

            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Fmp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Wvtt(_)),
                "wvtt サンプルエントリーが型付きで取り出せること"
            );
        }

        /// Fmp4SegmentDemuxer 経由で wvtt サンプルエントリーが `SampleEntry::Wvtt(_)` として取り出せる
        #[test]
        fn wvtt_sample_entry_via_fmp4_segment_demuxer() {
            let (init_bytes, media_segment) = build_wvtt_fmp4_segments();

            let mut demuxer = Fmp4SegmentDemuxer::new();
            demuxer
                .handle_init_segment(&init_bytes)
                .expect("init セグメントの処理に失敗した");
            let samples = demuxer
                .handle_media_segment(&media_segment)
                .expect("media セグメントの処理に失敗した");
            assert!(
                !samples.is_empty(),
                "メディアセグメントから少なくとも 1 サンプル取り出せる"
            );
            let entry = samples[0]
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Wvtt(_)),
                "wvtt サンプルエントリーが型付きで取り出せること"
            );
        }

        /// wvtt サンプルエントリーを持つ Sample を Mp4FileMuxer で 1 本マルチプレックスする
        fn build_wvtt_mp4_file_bytes() -> (Vec<u8>, MoovBox) {
            let sample_entry = SampleEntry::Wvtt(WvttBox {
                data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
                vttc_box: VttCBox {
                    config: String::from("WEBVTT"),
                },
                unknown_boxes: vec![],
            });
            let payload: &[u8] = b"WEBVTT-cue-payload-placeholder";
            build_subtitle_mp4_file_bytes(sample_entry, payload)
        }

        /// Mp4FileMuxer が wvtt 用の trak を組み立て、Mp4FileDemuxer で型付きに取り出せる
        ///
        /// wvtt は stpp と違ってハンドラー種別が `text` になる。
        /// Mp4FileDemuxer は `subt` と `text` を区別しないため、moov 側の属性も検証する
        #[test]
        fn wvtt_sample_entry_via_mp4_file_demuxer() {
            let (mp4_bytes, moov_box) = build_wvtt_mp4_file_bytes();

            assert_subtitle_trak(
                &moov_box,
                HdlrBox::HANDLER_TYPE_TEXT,
                MediaHeader::Sthd(SthdBox),
            );

            let mut demuxer = Mp4FileDemuxer::new();
            demuxer.handle_input(Input {
                position: 0,
                data: &mp4_bytes,
            });

            let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
            assert_eq!(tracks.len(), 1);
            assert!(
                matches!(tracks[0].kind, TrackKind::Subtitle),
                "字幕トラックとして復元されること"
            );

            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Mp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Wvtt(_)),
                "wvtt サンプルエントリーが型付きで取り出せること"
            );
        }

        // ===== Tx3g 正常経路担保テスト =====

        /// Fmp4SegmentMuxer 経由で tx3g トラックの init/メディアセグメントを生成し tkhd 属性を確認する
        ///
        /// tx3g は 3GPP TS 26.245 の対応表で handler_type = `text` + Media Header = `nmhd`
        /// と規定される。stpp / wvtt は `sthd` のため、tx3g のみ `nmhd` に切り替わる点が本テストの
        /// 決定的な差。`derive_trak_attributes` の Tx3g arm が正しく動作するかの回帰テストとして機能する
        #[test]
        fn subtitle_track_mux_tkhd_via_fmp4_segment_muxer_tx3g() {
            let tx3g_sample_entry = SampleEntry::Tx3g(Tx3gBox {
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
            // tx3g のサンプルデータは text_length(u16 BE) + テキスト + 任意 modifier boxes。
            // 本テストは sample_entry の型解決のみを検証するため、text_length = 5 + "HELLO" の
            // 最小構成を渡す（Fmp4SegmentMuxer は payload 内部構造を検証しない）
            let sample_payload = b"\x00\x05HELLO";
            let sample = Sample {
                track_kind: TrackKind::Subtitle,
                sample_entry: Some(tx3g_sample_entry),
                keyframe: true,
                timescale: NonZeroU32::new(1000).expect("非ゼロである"),
                duration: 1000,
                composition_time_offset: None,
                data_offset: 0,
                data_size: sample_payload.len(),
            };

            let mut muxer = Fmp4SegmentMuxer::new().expect("muxer の作成に失敗した");
            let media_segment = muxer
                .create_media_segment_metadata(std::slice::from_ref(&sample))
                .expect("media セグメントの作成に失敗した");
            assert!(
                !media_segment.is_empty(),
                "メディアセグメントのバイト列が空になっている"
            );

            let init_bytes = muxer
                .init_segment_bytes()
                .expect("init セグメントの構築に失敗した");

            let (_ftyp, ftyp_size) =
                FtypBox::decode(&init_bytes).expect("ftyp のデコードに失敗した");
            let (moov, _moov_size) =
                MoovBox::decode(&init_bytes[ftyp_size..]).expect("moov のデコードに失敗した");

            assert_eq!(moov.trak_boxes.len(), 1);
            let trak = &moov.trak_boxes[0];

            // tx3g はハンドラー種別 `text` + `nmhd` が対応表
            assert_eq!(
                trak.mdia_box.hdlr_box.handler_type,
                HdlrBox::HANDLER_TYPE_TEXT
            );
            assert!(matches!(
                trak.mdia_box.minf_box.media_header,
                Some(MediaHeader::Nmhd(NmhdBox))
            ));

            // tkhd は字幕トラック用の (0, 0, 0) で stpp / wvtt 版と同じ
            assert_eq!(trak.tkhd_box.volume, TkhdBox::DEFAULT_VIDEO_VOLUME);
            assert_eq!(trak.tkhd_box.width, FixedPointNumber::new(0, 0));
            assert_eq!(trak.tkhd_box.height, FixedPointNumber::new(0, 0));
        }

        /// tx3g サンプルエントリーを持つ Sample の init/メディアセグメントを組み立てる
        ///
        /// sample payload は text_length(u16 BE) + テキスト の最小構成で、
        /// Fmp4SegmentMuxer は payload 内部構造を検証しない
        fn build_tx3g_fmp4_segments() -> (Vec<u8>, Vec<u8>) {
            let tx3g_sample_entry = SampleEntry::Tx3g(Tx3gBox {
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
            build_subtitle_fmp4_segments(tx3g_sample_entry, b"\x00\x05HELLO")
        }

        /// Fmp4FileDemuxer 経由で tx3g サンプルエントリーが `SampleEntry::Tx3g(_)` として取り出せる
        #[test]
        fn tx3g_sample_entry_via_fmp4_file_demuxer() {
            let (init_bytes, media_segment) = build_tx3g_fmp4_segments();
            let mut fmp4_bytes = init_bytes;
            fmp4_bytes.extend_from_slice(&media_segment);

            let mut demuxer = Fmp4FileDemuxer::new();
            while let Some(required) = demuxer.required_input() {
                let start = required.position as usize;
                let end = start
                    .saturating_add(
                        required
                            .size
                            .unwrap_or(fmp4_bytes.len().saturating_sub(start)),
                    )
                    .min(fmp4_bytes.len());
                demuxer.handle_input(Input {
                    position: required.position,
                    data: fmp4_bytes.get(start..end).unwrap_or(&[]),
                });
            }

            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Fmp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Tx3g(_)),
                "tx3g サンプルエントリーが型付きで取り出せること"
            );
        }

        /// Fmp4SegmentDemuxer 経由で tx3g サンプルエントリーが `SampleEntry::Tx3g(_)` として取り出せる
        #[test]
        fn tx3g_sample_entry_via_fmp4_segment_demuxer() {
            let (init_bytes, media_segment) = build_tx3g_fmp4_segments();

            let mut demuxer = Fmp4SegmentDemuxer::new();
            demuxer
                .handle_init_segment(&init_bytes)
                .expect("init セグメントの処理に失敗した");
            let samples = demuxer
                .handle_media_segment(&media_segment)
                .expect("media セグメントの処理に失敗した");
            assert!(
                !samples.is_empty(),
                "メディアセグメントから少なくとも 1 サンプル取り出せる"
            );
            let entry = samples[0]
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Tx3g(_)),
                "tx3g サンプルエントリーが型付きで取り出せること"
            );
        }

        /// tx3g サンプルエントリーを持つ Sample を Mp4FileMuxer で 1 本マルチプレックスする
        fn build_tx3g_mp4_file_bytes() -> (Vec<u8>, MoovBox) {
            let sample_entry = SampleEntry::Tx3g(Tx3gBox {
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
            let payload: &[u8] = b"\x00\x05HELLO";
            build_subtitle_mp4_file_bytes(sample_entry, payload)
        }

        /// Mp4FileMuxer が tx3g 用の trak を組み立て、Mp4FileDemuxer で型付きに取り出せる
        ///
        /// tx3g だけはメディアヘッダーが `sthd` ではなく `nmhd` になる。
        /// この違いは demux 経路では観測できないため、moov 側の属性で検証する
        #[test]
        fn tx3g_sample_entry_via_mp4_file_demuxer() {
            let (mp4_bytes, moov_box) = build_tx3g_mp4_file_bytes();

            assert_subtitle_trak(
                &moov_box,
                HdlrBox::HANDLER_TYPE_TEXT,
                MediaHeader::Nmhd(NmhdBox),
            );

            let mut demuxer = Mp4FileDemuxer::new();
            demuxer.handle_input(Input {
                position: 0,
                data: &mp4_bytes,
            });

            let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
            assert_eq!(tracks.len(), 1);
            assert!(
                matches!(tracks[0].kind, TrackKind::Subtitle),
                "字幕トラックとして復元されること"
            );

            let sample = demuxer
                .next_sample()
                .expect("next_sample の取得に失敗した")
                .expect("Mp4FileDemuxer から sample が返らなかった");
            let entry = sample
                .sample_entry
                .expect("最初のサンプルは SampleEntry を持つ");
            assert!(
                matches!(entry, SampleEntry::Tx3g(_)),
                "tx3g サンプルエントリーが型付きで取り出せること"
            );
        }

        /// MoovBox: 複数トラック
        #[test]
        fn moov_box_multiple_tracks() {
            let moov = MoovBox {
                mvhd_box: minimal_mvhd_box(),
                trak_boxes: vec![
                    minimal_trak_box_audio(1),
                    minimal_trak_box_audio(2),
                    minimal_trak_box_audio(3),
                ],
                mvex_box: None,
                unknown_boxes: vec![],
            };
            let encoded = moov.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = MoovBox::decode(&encoded)
                .expect("直前にエンコードした有効な MoovBox は必ずデコードできる");
            assert_eq!(decoded.trak_boxes.len(), 3);
            assert_eq!(decoded.trak_boxes[0].tkhd_box.track_id, 1);
            assert_eq!(decoded.trak_boxes[1].tkhd_box.track_id, 2);
            assert_eq!(decoded.trak_boxes[2].tkhd_box.track_id, 3);
        }

        /// TrakBox: 最小構成
        #[test]
        fn trak_box_minimal() {
            let trak = minimal_trak_box_audio(1);
            let encoded = trak.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = TrakBox::decode(&encoded)
                .expect("直前にエンコードした有効な TrakBox は必ずデコードできる");
            assert_eq!(decoded.tkhd_box.track_id, 1);
            assert!(decoded.edts_box.is_none());
        }

        /// MdiaBox: 最小構成
        #[test]
        fn mdia_box_minimal() {
            let mdia = minimal_mdia_box_audio();
            let encoded = mdia.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = MdiaBox::decode(&encoded)
                .expect("直前にエンコードした有効な MdiaBox は必ずデコードできる");
            assert_eq!(decoded.hdlr_box.handler_type, HdlrBox::HANDLER_TYPE_SOUN);
        }

        /// MinfBox: audio 構成
        #[test]
        fn minf_box_audio_minimal() {
            let minf = minimal_minf_box_audio();
            let encoded = minf.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = MinfBox::decode(&encoded)
                .expect("直前にエンコードした有効な MinfBox は必ずデコードできる");
            assert!(matches!(decoded.media_header, Some(MediaHeader::Smhd(_))));
        }

        /// StblBox: 空の sample table
        #[test]
        fn stbl_box_empty_samples() {
            let stbl = minimal_stbl_box_audio();
            let encoded = stbl.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = StblBox::decode(&encoded)
                .expect("直前にエンコードした有効な StblBox は必ずデコードできる");
            assert!(decoded.stts_box.entries.is_empty());
            assert!(decoded.stsc_box.entries.is_empty());
            match &decoded.stco_or_co64_box {
                Either::A(stco) => assert!(stco.chunk_offsets.is_empty()),
                Either::B(_) => panic!("StcoBox を期待した"),
            }
        }

        /// StsdBox: 複数のエントリ
        #[test]
        fn stsd_box_multiple_entries() {
            let stsd = StsdBox {
                entries: vec![
                    SampleEntry::Opus(minimal_opus_box()),
                    SampleEntry::Opus(minimal_opus_box()),
                ],
            };
            let encoded = stsd.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = StsdBox::decode(&encoded)
                .expect("直前にエンコードした有効な StsdBox は必ずデコードできる");
            assert_eq!(decoded.entries.len(), 2);
        }
    }
}
