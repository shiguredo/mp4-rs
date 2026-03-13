//! コンテナ Box の Property-Based Testing
//!
//! MoovBox, TrakBox, MdiaBox, MinfBox, StblBox のテスト

use std::num::{NonZeroU16, NonZeroU32};

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Either, Encode, FixedPointNumber, Mp4FileTime,
    boxes::{
        AudioSampleEntryFields, Co64Box, DinfBox, DopsBox, HdlrBox, MdhdBox, MdiaBox, MinfBox,
        MoovBox, MvhdBox, OpusBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox, StscEntry,
        StsdBox, StssBox, StszBox, SttsBox, SttsEntry, TkhdBox, TrakBox, VmhdBox,
    },
};

// ===== 最小限の構成を生成する関数 =====

/// 最小限の MvhdBox を生成
fn minimal_mvhd_box() -> MvhdBox {
    MvhdBox {
        creation_time: Mp4FileTime::from_secs(0),
        modification_time: Mp4FileTime::from_secs(0),
        timescale: NonZeroU32::new(1000).unwrap(),
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
        timescale: NonZeroU32::new(48000).unwrap(),
        duration: 0,
        language: MdhdBox::LANGUAGE_UNDEFINED,
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
        smhd_or_vmhd_box: Some(Either::A(minimal_smhd_box())),
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

// ===== Strategy 定義 =====

/// SttsEntry を生成する Strategy
fn arb_stts_entry() -> impl Strategy<Value = SttsEntry> {
    (any::<u32>(), any::<u32>()).prop_map(|(sample_count, sample_delta)| SttsEntry {
        sample_count,
        sample_delta,
    })
}

/// StscEntry を生成する Strategy
fn arb_stsc_entry() -> impl Strategy<Value = StscEntry> {
    (1u32..=u32::MAX, any::<u32>(), 1u32..=u32::MAX).prop_map(
        |(first_chunk, sample_per_chunk, sample_description_index)| StscEntry {
            first_chunk: NonZeroU32::new(first_chunk).unwrap(),
            sample_per_chunk,
            sample_description_index: NonZeroU32::new(sample_description_index).unwrap(),
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    // ===== StblBox のテスト =====

    /// StblBox の encode/decode roundtrip
    #[test]
    fn stbl_box_roundtrip(
        stts_entries in prop::collection::vec(arb_stts_entry(), 0..10),
        stsc_entries in prop::collection::vec(arb_stsc_entry(), 0..10),
        stco_offsets in prop::collection::vec(any::<u32>(), 0..10),
        stss_numbers in prop::collection::vec(1u32..=u32::MAX, 0..10)
    ) {
        let stbl = StblBox {
            stsd_box: minimal_stsd_box_audio(),
            stts_box: SttsBox { entries: stts_entries.clone() },
            ctts_box: None,
            cslg_box: None,
            stsc_box: StscBox { entries: stsc_entries.clone() },
            stsz_box: StszBox::Variable { entry_sizes: vec![] },
            stco_or_co64_box: Either::A(StcoBox { chunk_offsets: stco_offsets.clone() }),
            stss_box: if stss_numbers.is_empty() {
                None
            } else {
                Some(StssBox {
                    sample_numbers: stss_numbers.iter().map(|&n| NonZeroU32::new(n).unwrap()).collect(),
                })
            },
            sdtp_box: None,
            unknown_boxes: vec![],
        };
        let encoded = stbl.encode_to_vec().unwrap();
        let (decoded, size) = StblBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.stts_box.entries.len(), stts_entries.len());
        prop_assert_eq!(decoded.stsc_box.entries.len(), stsc_entries.len());
        match &decoded.stco_or_co64_box {
            Either::A(stco) => prop_assert_eq!(stco.chunk_offsets.clone(), stco_offsets),
            Either::B(_) => prop_assert!(false, "Expected StcoBox, got Co64Box"),
        }
    }

    /// StblBox with Co64Box roundtrip
    #[test]
    fn stbl_box_co64_roundtrip(
        co64_offsets in prop::collection::vec(any::<u64>(), 0..10)
    ) {
        let stbl = StblBox {
            stsd_box: minimal_stsd_box_audio(),
            stts_box: minimal_stts_box(),
            ctts_box: None,
            cslg_box: None,
            stsc_box: minimal_stsc_box(),
            stsz_box: StszBox::Variable { entry_sizes: vec![] },
            stco_or_co64_box: Either::B(Co64Box { chunk_offsets: co64_offsets.clone() }),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: vec![],
        };
        let encoded = stbl.encode_to_vec().unwrap();
        let (decoded, size) = StblBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        match &decoded.stco_or_co64_box {
            Either::A(_) => prop_assert!(false, "Expected Co64Box, got StcoBox"),
            Either::B(co64) => prop_assert_eq!(co64.chunk_offsets.clone(), co64_offsets),
        }
    }

    // ===== MinfBox のテスト =====

    /// MinfBox (audio) の encode/decode roundtrip
    #[test]
    fn minf_box_audio_roundtrip(
        balance_int in any::<u8>(),
        balance_frac in any::<u8>()
    ) {
        let minf = MinfBox {
            smhd_or_vmhd_box: Some(Either::A(SmhdBox {
                balance: FixedPointNumber::new(balance_int, balance_frac),
            })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        match &decoded.smhd_or_vmhd_box {
            Some(Either::A(_smhd)) => {}
            _ => prop_assert!(false, "Expected SmhdBox"),
        }
    }

    /// MinfBox (video) の encode/decode roundtrip
    #[test]
    fn minf_box_video_roundtrip(
        graphicsmode in any::<u16>(),
        opcolor in any::<[u16; 3]>()
    ) {
        let minf = MinfBox {
            smhd_or_vmhd_box: Some(Either::B(VmhdBox { graphicsmode, opcolor })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        match &decoded.smhd_or_vmhd_box {
            Some(Either::B(vmhd)) => prop_assert_eq!(vmhd.graphicsmode, graphicsmode),
            _ => prop_assert!(false, "Expected VmhdBox"),
        }
    }

    // ===== MdiaBox のテスト =====

    /// MdiaBox の encode/decode roundtrip
    #[test]
    fn mdia_box_roundtrip(
        timescale in 1u32..=u32::MAX,
        duration in any::<u64>(),
        language in prop::array::uniform3(0x61u8..=0x7Au8)
    ) {
        let mdia = MdiaBox {
            mdhd_box: MdhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(timescale).unwrap(),
                duration,
                language,
            },
            hdlr_box: minimal_hdlr_box_audio(),
            minf_box: minimal_minf_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = mdia.encode_to_vec().unwrap();
        let (decoded, size) = MdiaBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mdhd_box.timescale.get(), timescale);
        prop_assert_eq!(decoded.mdhd_box.duration, duration);
        prop_assert_eq!(decoded.mdhd_box.language, language);
    }

    // ===== TrakBox のテスト =====

    /// TrakBox の encode/decode roundtrip
    #[test]
    fn trak_box_roundtrip(
        track_id in any::<u32>(),
        duration in any::<u64>(),
        layer in any::<i16>(),
        alternate_group in any::<i16>()
    ) {
        let trak = TrakBox {
            tkhd_box: TkhdBox {
                flag_track_enabled: true,
                flag_track_in_movie: true,
                flag_track_in_preview: false,
                flag_track_size_is_aspect_ratio: false,
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                track_id,
                duration,
                layer,
                alternate_group,
                volume: TkhdBox::DEFAULT_AUDIO_VOLUME,
                matrix: TkhdBox::DEFAULT_MATRIX,
                width: FixedPointNumber::new(0, 0),
                height: FixedPointNumber::new(0, 0),
            },
            edts_box: None,
            mdia_box: minimal_mdia_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = trak.encode_to_vec().unwrap();
        let (decoded, size) = TrakBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.tkhd_box.track_id, track_id);
        prop_assert_eq!(decoded.tkhd_box.duration, duration);
        prop_assert_eq!(decoded.tkhd_box.layer, layer);
        prop_assert_eq!(decoded.tkhd_box.alternate_group, alternate_group);
    }

    // ===== MoovBox のテスト =====

    /// MoovBox の encode/decode roundtrip
    #[test]
    fn moov_box_roundtrip(
        timescale in 1u32..=u32::MAX,
        duration in any::<u64>(),
        next_track_id in any::<u32>(),
        track_count in 1usize..=3
    ) {
        let trak_boxes: Vec<TrakBox> = (1..=track_count)
            .map(|i| minimal_trak_box_audio(i as u32))
            .collect();

        let moov = MoovBox {
            mvhd_box: MvhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(timescale).unwrap(),
                duration,
                rate: MvhdBox::DEFAULT_RATE,
                volume: MvhdBox::DEFAULT_VOLUME,
                matrix: MvhdBox::DEFAULT_MATRIX,
                next_track_id,
            },
            trak_boxes,
            mvex_box: None,
            unknown_boxes: vec![],
        };
        let encoded = moov.encode_to_vec().unwrap();
        let (decoded, size) = MoovBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.mvhd_box.timescale.get(), timescale);
        prop_assert_eq!(decoded.mvhd_box.duration, duration);
        prop_assert_eq!(decoded.mvhd_box.next_track_id, next_track_id);
        prop_assert_eq!(decoded.trak_boxes.len(), track_count);
    }
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// MoovBox: 最小構成
    #[test]
    fn moov_box_minimal() {
        let moov = minimal_moov_box();
        let encoded = moov.encode_to_vec().unwrap();
        let (decoded, _) = MoovBox::decode(&encoded).unwrap();
        assert_eq!(decoded.trak_boxes.len(), 1);
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
        let encoded = moov.encode_to_vec().unwrap();
        let (decoded, _) = MoovBox::decode(&encoded).unwrap();
        assert_eq!(decoded.trak_boxes.len(), 3);
        assert_eq!(decoded.trak_boxes[0].tkhd_box.track_id, 1);
        assert_eq!(decoded.trak_boxes[1].tkhd_box.track_id, 2);
        assert_eq!(decoded.trak_boxes[2].tkhd_box.track_id, 3);
    }

    /// TrakBox: 最小構成
    #[test]
    fn trak_box_minimal() {
        let trak = minimal_trak_box_audio(1);
        let encoded = trak.encode_to_vec().unwrap();
        let (decoded, _) = TrakBox::decode(&encoded).unwrap();
        assert_eq!(decoded.tkhd_box.track_id, 1);
        assert!(decoded.edts_box.is_none());
    }

    /// MdiaBox: 最小構成
    #[test]
    fn mdia_box_minimal() {
        let mdia = minimal_mdia_box_audio();
        let encoded = mdia.encode_to_vec().unwrap();
        let (decoded, _) = MdiaBox::decode(&encoded).unwrap();
        assert_eq!(decoded.hdlr_box.handler_type, HdlrBox::HANDLER_TYPE_SOUN);
    }

    /// MinfBox: audio 構成
    #[test]
    fn minf_box_audio_minimal() {
        let minf = minimal_minf_box_audio();
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, _) = MinfBox::decode(&encoded).unwrap();
        assert!(matches!(decoded.smhd_or_vmhd_box, Some(Either::A(_))));
    }

    /// StblBox: 空の sample table
    #[test]
    fn stbl_box_empty_samples() {
        let stbl = minimal_stbl_box_audio();
        let encoded = stbl.encode_to_vec().unwrap();
        let (decoded, _) = StblBox::decode(&encoded).unwrap();
        assert!(decoded.stts_box.entries.is_empty());
        assert!(decoded.stsc_box.entries.is_empty());
        match &decoded.stco_or_co64_box {
            Either::A(stco) => assert!(stco.chunk_offsets.is_empty()),
            Either::B(_) => panic!("Expected StcoBox"),
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
        let encoded = stsd.encode_to_vec().unwrap();
        let (decoded, _) = StsdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.entries.len(), 2);
    }
}
