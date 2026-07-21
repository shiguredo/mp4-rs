//! コンテナ Box の Property-Based Testing
//!
//! MoovBox, TrakBox, MdiaBox, MinfBox, StblBox のテスト

use std::num::{NonZeroU16, NonZeroU32};

use proptest::prelude::*;
use shiguredo_mp4::{
    BoxSize, BoxType, Decode, Either, Encode, FixedPointNumber, Mp4FileTime, SampleFlags,
    TrackKind,
    boxes::{
        AudioSampleEntryFields, Brand, Co64Box, DinfBox, DopsBox, FtypBox, HdlrBox, MdhdBox,
        MdiaBox, MediaHeader, MinfBox, MoovBox, MvexBox, MvhdBox, NmhdBox, OpusBox, SampleEntry,
        SmhdBox, StblBox, StcoBox, SthdBox, StscBox, StscEntry, StsdBox, StssBox, StszBox, SttsBox,
        SttsEntry, TkhdBox, TrakBox, TrexBox, UnknownBox, VmhdBox,
    },
    demux::{Fmp4FileDemuxer, Fmp4SegmentDemuxer, Input, Mp4FileDemuxer},
    mux::{Fmp4SegmentMuxer, Sample},
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
/// stsd 内に `SampleEntry::Unknown` を 1 つ持つ。0042 の時点では方式固有の
/// SampleEntry（Stpp / Wvtt / Tx3g）は未実装のため、Unknown フォールバックを利用する。
/// `sample_entry_box_type` に `stpp` / `wvtt` / `tx3g` を渡して切り替える
fn minimal_stsd_box_subtitle(sample_entry_box_type: [u8; 4]) -> StsdBox {
    StsdBox {
        entries: vec![SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(sample_entry_box_type),
            box_size: BoxSize::U32(8),
            payload: vec![],
        })],
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
/// Media Header には `SthdBox` を使う（0042 の暫定選択と同じ）。
/// 0043-0045 で必要に応じて Nmhd に切り替える形にリファクタする想定
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
            media_header: Some(MediaHeader::Smhd(SmhdBox {
                balance: FixedPointNumber::new(balance_int, balance_frac),
            })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        match &decoded.media_header {
            Some(MediaHeader::Smhd(_smhd)) => {}
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
            media_header: Some(MediaHeader::Vmhd(VmhdBox { graphicsmode, opcolor })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        match &decoded.media_header {
            Some(MediaHeader::Vmhd(vmhd)) => prop_assert_eq!(vmhd.graphicsmode, graphicsmode),
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

    /// SthdBox: encode/decode roundtrip
    ///
    /// SthdBox はペイロードを持たないため、encode→decode で同一の値が復元されることのみを確認する
    #[test]
    fn sthd_box_roundtrip() {
        let sthd = SthdBox;
        let encoded = sthd.encode_to_vec().unwrap();
        let (decoded, size) = SthdBox::decode(&encoded).unwrap();
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sthd);
    }

    /// NmhdBox: encode/decode roundtrip
    ///
    /// NmhdBox もペイロードを持たないため、encode→decode で同一の値が復元されることのみを確認する
    #[test]
    fn nmhd_box_roundtrip() {
        let nmhd = NmhdBox;
        let encoded = nmhd.encode_to_vec().unwrap();
        let (decoded, size) = NmhdBox::decode(&encoded).unwrap();
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, nmhd);
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
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();
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
        let encoded = minf.encode_to_vec().unwrap();
        let (decoded, size) = MinfBox::decode(&encoded).unwrap();
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
        let mut bytes = ftyp.encode_to_vec().unwrap();
        bytes.extend_from_slice(&moov.encode_to_vec().unwrap());
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
        let mut bytes = ftyp.encode_to_vec().unwrap();
        bytes.extend_from_slice(&moov.encode_to_vec().unwrap());
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
            let tracks = demuxer.tracks().expect("failed to get tracks");
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
            let bytes =
                build_fmp4_init_segment_bytes_with_subtitle(handler_type, sample_entry_box_type);
            let mut demuxer = Fmp4FileDemuxer::new();
            while let Some(required) = demuxer.required_input() {
                let start = required.position as usize;
                let end = start.saturating_add(required.size.unwrap_or(bytes.len() - start));
                demuxer.handle_input(Input {
                    position: required.position,
                    data: bytes.get(start..end).unwrap_or(&[]),
                });
            }
            let tracks = demuxer.tracks().expect("failed to get tracks");
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
            let init_bytes =
                build_fmp4_init_segment_bytes_with_subtitle(handler_type, sample_entry_box_type);
            let mut demuxer = Fmp4SegmentDemuxer::new();
            demuxer
                .handle_init_segment(&init_bytes)
                .expect("failed to handle init segment");
            let tracks = demuxer.tracks().expect("failed to get tracks");
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

    /// Fmp4SegmentMuxer 経由で字幕トラックの init/media segment を生成し tkhd 属性を確認する
    ///
    /// Fmp4SegmentMuxer に TrackKind::Subtitle の Sample を渡して init segment を生成し、
    /// 生成された moov 内 trak の tkhd を検証する:
    /// - volume == 0 (DEFAULT_VIDEO_VOLUME)
    /// - width == 0
    /// - height == 0
    #[test]
    fn subtitle_track_mux_tkhd_via_fmp4_segment_muxer() {
        let subtitle_sample_entry = SampleEntry::Unknown(UnknownBox {
            box_type: BoxType::Normal(*b"stpp"),
            box_size: BoxSize::U32(8),
            payload: vec![],
        });
        let sample_payload = b"hello subtitle";
        let sample = Sample {
            track_kind: TrackKind::Subtitle,
            sample_entry: Some(subtitle_sample_entry),
            keyframe: true,
            timescale: NonZeroU32::new(1000).expect("non-zero"),
            duration: 1000,
            composition_time_offset: None,
            data_offset: 0,
            data_size: sample_payload.len(),
        };

        let mut muxer = Fmp4SegmentMuxer::new().expect("failed to create muxer");
        let mut media_segment = muxer
            .create_media_segment_metadata(std::slice::from_ref(&sample))
            .expect("failed to create media segment");
        media_segment.extend_from_slice(sample_payload);

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("failed to build init segment");

        // init segment 内の trak を検証（ftyp のあとに moov が続く前提）
        let (_ftyp, ftyp_size) = FtypBox::decode(&init_bytes).expect("failed to decode ftyp");
        let (moov, _moov_size) =
            MoovBox::decode(&init_bytes[ftyp_size..]).expect("failed to decode moov");

        assert_eq!(moov.trak_boxes.len(), 1);
        let trak = &moov.trak_boxes[0];

        // handler type と Media Header の暫定選択（subt + sthd）を確認
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
        assert!(matches!(decoded.media_header, Some(MediaHeader::Smhd(_))));
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
