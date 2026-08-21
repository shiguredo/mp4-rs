//! コンテナ Box の Property-Based Testing
//!
//! MoovBox, TrakBox, MdiaBox, MinfBox, StblBox のテスト

use std::num::{NonZeroU16, NonZeroU32};

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Either, Encode, FixedPointNumber, LanguageCode, Mp4FileTime,
    boxes::{
        AudioSampleEntryFields, Co64Box, DinfBox, DopsBox, HdlrBox, MdhdBox, MdiaBox, MediaHeader,
        MinfBox, MoovBox, MvhdBox, OpusBox, SampleEntry, SmhdBox, StblBox, StcoBox, StscBox,
        StscEntry, StsdBox, StssBox, StszBox, SttsBox, SttsEntry, TkhdBox, TrakBox, VmhdBox,
    },
};

// ===== 最小限の構成を生成する関数 =====

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

// ===== サンプラー定義 =====

/// このファイルの主要 PBT ケース数（旧 `with_cases(50)` を維持）
const CASES: usize = 50;

/// noprop の `sample_usize_in` で長さを引いてから要素を生成するベクタサンプラー
fn sample_vec<T>(
    ctx: &mut TestCaseContext,
    range: std::ops::Range<usize>,
    mut elem: impl FnMut(&mut TestCaseContext) -> T,
) -> Vec<T> {
    let len = noprop::sample_usize_in(ctx, range);
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(elem(ctx));
    }
    result
}

/// SttsEntry を生成する
fn arb_stts_entry(ctx: &mut TestCaseContext) -> SttsEntry {
    SttsEntry {
        sample_count: noprop::sample_u32(ctx),
        sample_delta: noprop::sample_u32(ctx),
    }
}

/// StscEntry を生成する
fn arb_stsc_entry(ctx: &mut TestCaseContext) -> StscEntry {
    let first_chunk = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
    let sample_per_chunk = noprop::sample_u32(ctx);
    let sample_description_index = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
    StscEntry {
        first_chunk: NonZeroU32::new(first_chunk).expect("サンプル値域が 1 以上なので非ゼロ"),
        sample_per_chunk,
        sample_description_index: NonZeroU32::new(sample_description_index)
            .expect("サンプル値域が 1 以上なので非ゼロ"),
    }
}

/// ISO-639-2/T の言語コード (a-z の 3 文字) を生成する
fn arb_language_code_lower(ctx: &mut TestCaseContext) -> LanguageCode {
    let bytes = [
        noprop::sample_u64_in(ctx, 0x61..=0x7A) as u8,
        noprop::sample_u64_in(ctx, 0x61..=0x7A) as u8,
        noprop::sample_u64_in(ctx, 0x61..=0x7A) as u8,
    ];
    LanguageCode::new(bytes).expect("サンプル値域は有効な言語コード")
}

// ===== StblBox のテスト =====

/// StblBox の encode/decode roundtrip
#[test]
fn stbl_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let stts_entries = sample_vec(ctx, 0..10, arb_stts_entry);
        let stsc_entries = sample_vec(ctx, 0..10, arb_stsc_entry);
        let stco_offsets = sample_vec(ctx, 0..10, noprop::sample_u32);
        let stss_numbers = sample_vec(ctx, 0..10, |ctx| {
            noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32
        });
        let stbl = StblBox {
            stsd_box: minimal_stsd_box_audio(),
            stts_box: SttsBox {
                entries: stts_entries.clone(),
            },
            ctts_box: None,
            cslg_box: None,
            stsc_box: StscBox {
                entries: stsc_entries.clone(),
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: stco_offsets.clone(),
            }),
            stss_box: if stss_numbers.is_empty() {
                None
            } else {
                Some(StssBox {
                    sample_numbers: stss_numbers
                        .iter()
                        .map(|&n| NonZeroU32::new(n).expect("サンプル値域が 1 以上なので非ゼロ"))
                        .collect(),
                })
            },
            sdtp_box: None,
            unknown_boxes: vec![],
        };
        let encoded = stbl.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = StblBox::decode(&encoded)
            .expect("直前にエンコードした有効な StblBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.stts_box.entries.len(), stts_entries.len());
        assert_eq!(decoded.stsc_box.entries.len(), stsc_entries.len());
        match &decoded.stco_or_co64_box {
            Either::A(stco) => assert_eq!(stco.chunk_offsets.clone(), stco_offsets),
            Either::B(_) => panic!("StcoBox を期待したが Co64Box だった"),
        }
        Ok(())
    })?;
    Ok(())
}

/// StblBox with Co64Box roundtrip
#[test]
fn stbl_box_co64_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let co64_offsets = sample_vec(ctx, 0..10, noprop::sample_u64);
        let stbl = StblBox {
            stsd_box: minimal_stsd_box_audio(),
            stts_box: minimal_stts_box(),
            ctts_box: None,
            cslg_box: None,
            stsc_box: minimal_stsc_box(),
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::B(Co64Box {
                chunk_offsets: co64_offsets.clone(),
            }),
            stss_box: None,
            sdtp_box: None,
            unknown_boxes: vec![],
        };
        let encoded = stbl.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = StblBox::decode(&encoded)
            .expect("直前にエンコードした有効な StblBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        match &decoded.stco_or_co64_box {
            Either::A(_) => panic!("Co64Box を期待したが StcoBox だった"),
            Either::B(co64) => assert_eq!(co64.chunk_offsets.clone(), co64_offsets),
        }
        Ok(())
    })?;
    Ok(())
}

// ===== MinfBox のテスト =====

/// MinfBox (audio) の encode/decode roundtrip
#[test]
fn minf_box_audio_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let balance_int = noprop::sample_u8(ctx);
        let balance_frac = noprop::sample_u8(ctx);
        let minf = MinfBox {
            media_header: Some(MediaHeader::Smhd(SmhdBox {
                balance: FixedPointNumber::new(balance_int, balance_frac),
            })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MinfBox::decode(&encoded)
            .expect("直前にエンコードした有効な MinfBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        match &decoded.media_header {
            Some(MediaHeader::Smhd(_smhd)) => {}
            _ => panic!("SmhdBox を期待した"),
        }
        Ok(())
    })?;
    Ok(())
}

/// MinfBox (video) の encode/decode roundtrip
#[test]
fn minf_box_video_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let graphicsmode = noprop::sample_u16(ctx);
        let opcolor = [
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
        ];
        let minf = MinfBox {
            media_header: Some(MediaHeader::Vmhd(VmhdBox {
                graphicsmode,
                opcolor,
            })),
            dinf_box: minimal_dinf_box(),
            stbl_box: minimal_stbl_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = minf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MinfBox::decode(&encoded)
            .expect("直前にエンコードした有効な MinfBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        match &decoded.media_header {
            Some(MediaHeader::Vmhd(vmhd)) => assert_eq!(vmhd.graphicsmode, graphicsmode),
            _ => panic!("VmhdBox を期待した"),
        }
        Ok(())
    })?;
    Ok(())
}

// ===== MdiaBox のテスト =====

/// MdiaBox の encode/decode roundtrip
#[test]
fn mdia_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64(ctx);
        let language = arb_language_code_lower(ctx);
        let mdia = MdiaBox {
            mdhd_box: MdhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
                duration,
                language,
            },
            hdlr_box: minimal_hdlr_box_audio(),
            minf_box: minimal_minf_box_audio(),
            unknown_boxes: vec![],
        };
        let encoded = mdia.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MdiaBox::decode(&encoded)
            .expect("直前にエンコードした有効な MdiaBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.mdhd_box.timescale.get(), timescale);
        assert_eq!(decoded.mdhd_box.duration, duration);
        assert_eq!(decoded.mdhd_box.language, language);
        Ok(())
    })?;
    Ok(())
}

// ===== TrakBox のテスト =====

/// TrakBox の encode/decode roundtrip
#[test]
fn trak_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let track_id = noprop::sample_u32(ctx);
        let duration = noprop::sample_u64(ctx);
        let layer = noprop::sample_i16(ctx);
        let alternate_group = noprop::sample_i16(ctx);
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
        let encoded = trak.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TrakBox::decode(&encoded)
            .expect("直前にエンコードした有効な TrakBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.tkhd_box.track_id, track_id);
        assert_eq!(decoded.tkhd_box.duration, duration);
        assert_eq!(decoded.tkhd_box.layer, layer);
        assert_eq!(decoded.tkhd_box.alternate_group, alternate_group);
        Ok(())
    })?;
    Ok(())
}

// ===== MoovBox のテスト =====

/// MoovBox の encode/decode roundtrip
#[test]
fn moov_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64(ctx);
        let next_track_id = noprop::sample_u32(ctx);
        let track_count = noprop::sample_usize_in(ctx, 1..=3);
        let trak_boxes: Vec<TrakBox> = (1..=track_count)
            .map(|i| minimal_trak_box_audio(i as u32))
            .collect();

        let moov = MoovBox {
            mvhd_box: MvhdBox {
                creation_time: Mp4FileTime::from_secs(0),
                modification_time: Mp4FileTime::from_secs(0),
                timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
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
        let encoded = moov.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MoovBox::decode(&encoded)
            .expect("直前にエンコードした有効な MoovBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.mvhd_box.timescale.get(), timescale);
        assert_eq!(decoded.mvhd_box.duration, duration);
        assert_eq!(decoded.mvhd_box.next_track_id, next_track_id);
        assert_eq!(decoded.trak_boxes.len(), track_count);
        Ok(())
    })?;
    Ok(())
}
