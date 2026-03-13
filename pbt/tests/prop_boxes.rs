//! ボックス構造体の Property-Based Testing

use std::num::NonZeroU32;

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, Mp4FileTime, Utf8String,
    boxes::{
        Brand, Co64Box, CslgBox, CttsBox, CttsEntry, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry,
        FtypBox, HdlrBox, MdhdBox, MvhdBox, SdtpBox, SdtpSampleFlags, SmhdBox, StcoBox, StscBox,
        StscEntry, StssBox, SttsBox, SttsEntry, TkhdBox, UrlBox, VmhdBox,
    },
};

/// SttsEntry を生成する Strategy
fn arb_stts_entry() -> impl Strategy<Value = SttsEntry> {
    (any::<u32>(), any::<u32>()).prop_map(|(sample_count, sample_delta)| SttsEntry {
        sample_count,
        sample_delta,
    })
}

/// CttsEntry (version 0 互換) を生成する Strategy
fn arb_ctts_entry_v0() -> impl Strategy<Value = CttsEntry> {
    (any::<u32>(), any::<u32>()).prop_map(|(sample_count, sample_offset)| CttsEntry {
        sample_count,
        sample_offset: sample_offset as i64,
    })
}

/// CttsEntry (version 1) を生成する Strategy
fn arb_ctts_entry_v1() -> impl Strategy<Value = CttsEntry> {
    (any::<u32>(), any::<i32>()).prop_map(|(sample_count, sample_offset)| CttsEntry {
        sample_count,
        sample_offset: sample_offset as i64,
    })
}

/// SdtpSampleFlags を生成する Strategy
fn arb_sdtp_sample_flags() -> impl Strategy<Value = SdtpSampleFlags> {
    (0u8..4, 0u8..4, 0u8..4, 0u8..4).prop_map(
        |(is_leading, sample_depends_on, sample_is_depended_on, sample_has_redundancy)| {
            SdtpSampleFlags::from_fields(
                is_leading,
                sample_depends_on,
                sample_is_depended_on,
                sample_has_redundancy,
            )
        },
    )
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

/// ElstEntry (version 0 互換) を生成する Strategy
fn arb_elst_entry_v0() -> impl Strategy<Value = ElstEntry> {
    (
        0u64..=(u32::MAX as u64),
        (i32::MIN as i64)..=(i32::MAX as i64),
        any::<i16>(),
        any::<i16>(),
    )
        .prop_map(
            |(edit_duration, media_time, rate_int, rate_frac)| ElstEntry {
                edit_duration,
                media_time,
                media_rate: FixedPointNumber::new(rate_int, rate_frac),
            },
        )
}

/// ElstEntry (version 1) を生成する Strategy
fn arb_elst_entry_v1() -> impl Strategy<Value = ElstEntry> {
    (any::<u64>(), any::<i64>(), any::<i16>(), any::<i16>()).prop_map(
        |(edit_duration, media_time, rate_int, rate_frac)| ElstEntry {
            edit_duration,
            media_time,
            media_rate: FixedPointNumber::new(rate_int, rate_frac),
        },
    )
}

/// 4 文字のブランド名を生成する Strategy
fn arb_brand() -> impl Strategy<Value = Brand> {
    prop::array::uniform4(0x20u8..=0x7Eu8).prop_map(Brand::new)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    // ===== SttsBox のテスト =====

    /// SttsBox の encode/decode roundtrip
    #[test]
    fn stts_box_roundtrip(entries in prop::collection::vec(arb_stts_entry(), 0..50)) {
        let stts = SttsBox { entries: entries.clone() };
        let encoded = stts.encode_to_vec().unwrap();
        let (decoded, size) = SttsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            prop_assert_eq!(orig.sample_count, dec.sample_count);
            prop_assert_eq!(orig.sample_delta, dec.sample_delta);
        }
    }

    /// SttsBox::from_sample_deltas の不変条件: 連続する同じ delta は集約される
    #[test]
    fn stts_from_sample_deltas_invariant(deltas in prop::collection::vec(any::<u32>(), 0..100)) {
        let stts = SttsBox::from_sample_deltas(deltas.iter().cloned());

        // 隣接エントリは異なる sample_delta を持つ
        for window in stts.entries.windows(2) {
            prop_assert_ne!(window[0].sample_delta, window[1].sample_delta,
                "隣接エントリが同じ sample_delta を持っている");
        }

        // sample_count の合計が元の deltas 数と一致
        let total_count: u32 = stts.entries.iter().map(|e| e.sample_count).sum();
        prop_assert_eq!(total_count as usize, deltas.len());
    }

    // ===== CttsBox のテスト =====

    /// CttsBox (version 0) の encode/decode roundtrip
    #[test]
    fn ctts_box_v0_roundtrip(entries in prop::collection::vec(arb_ctts_entry_v0(), 0..50)) {
        let ctts = CttsBox {
            version: 0,
            entries: entries.clone(),
        };
        let encoded = ctts.encode_to_vec().unwrap();
        let (decoded, size) = CttsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.version, 0);
        prop_assert_eq!(decoded.entries, entries);
    }

    /// CttsBox (version 1) の encode/decode roundtrip
    #[test]
    fn ctts_box_v1_roundtrip(entries in prop::collection::vec(arb_ctts_entry_v1(), 0..50)) {
        let ctts = CttsBox {
            version: 1,
            entries: entries.clone(),
        };
        let encoded = ctts.encode_to_vec().unwrap();
        let (decoded, size) = CttsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.version, 1);
        prop_assert_eq!(decoded.entries, entries);
    }

    /// CttsBox: version が 2 以上の場合はデコードエラー
    #[test]
    fn ctts_box_invalid_version_decode_error(version in 2u8..=u8::MAX) {
        let ctts = CttsBox {
            version: 1,
            entries: vec![CttsEntry {
                sample_count: 1,
                sample_offset: 0,
            }],
        };
        let mut encoded = ctts
            .encode_to_vec()
            .expect("ctts test fixture must be encodable");
        encoded[8] = version; // full box version
        prop_assert!(CttsBox::decode(&encoded).is_err());
    }

    /// CttsBox: version 0 で負の sample_offset をエンコードするとエラー
    #[test]
    fn ctts_box_v0_negative_offset_error(
        sample_count in any::<u32>(),
        sample_offset in i64::MIN..0i64
    ) {
        let ctts = CttsBox {
            version: 0,
            entries: vec![CttsEntry {
                sample_count,
                sample_offset,
            }],
        };
        prop_assert!(ctts.encode_to_vec().is_err());
    }

    // ===== CslgBox のテスト =====

    /// CslgBox (version 0) の encode/decode roundtrip
    #[test]
    fn cslg_box_v0_roundtrip(
        composition_to_dts_shift in any::<i32>(),
        least_decode_to_display_delta in any::<i32>(),
        greatest_decode_to_display_delta in any::<i32>(),
        composition_start_time in any::<i32>(),
        composition_end_time in any::<i32>()
    ) {
        let cslg = CslgBox {
            version: 0,
            composition_to_dts_shift: composition_to_dts_shift as i64,
            least_decode_to_display_delta: least_decode_to_display_delta as i64,
            greatest_decode_to_display_delta: greatest_decode_to_display_delta as i64,
            composition_start_time: composition_start_time as i64,
            composition_end_time: composition_end_time as i64,
        };
        let encoded = cslg.encode_to_vec().unwrap();
        let (decoded, size) = CslgBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded, cslg);
    }

    /// CslgBox (version 1) の encode/decode roundtrip
    #[test]
    fn cslg_box_v1_roundtrip(
        composition_to_dts_shift in any::<i64>(),
        least_decode_to_display_delta in any::<i64>(),
        greatest_decode_to_display_delta in any::<i64>(),
        composition_start_time in any::<i64>(),
        composition_end_time in any::<i64>()
    ) {
        let cslg = CslgBox {
            version: 1,
            composition_to_dts_shift,
            least_decode_to_display_delta,
            greatest_decode_to_display_delta,
            composition_start_time,
            composition_end_time,
        };
        let encoded = cslg.encode_to_vec().unwrap();
        let (decoded, size) = CslgBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded, cslg);
    }

    /// CslgBox: version が 2 以上の場合はデコードエラー
    #[test]
    fn cslg_box_invalid_version_decode_error(version in 2u8..=u8::MAX) {
        let cslg = CslgBox {
            version: 1,
            composition_to_dts_shift: 0,
            least_decode_to_display_delta: 0,
            greatest_decode_to_display_delta: 0,
            composition_start_time: 0,
            composition_end_time: 0,
        };
        let mut encoded = cslg
            .encode_to_vec()
            .expect("cslg test fixture must be encodable");
        encoded[8] = version; // full box version
        prop_assert!(CslgBox::decode(&encoded).is_err());
    }

    // ===== SdtpBox のテスト =====

    /// SdtpBox の encode/decode roundtrip
    #[test]
    fn sdtp_box_roundtrip(entries in prop::collection::vec(arb_sdtp_sample_flags(), 0..100)) {
        let sdtp = SdtpBox {
            entries: entries.clone(),
        };
        let encoded = sdtp.encode_to_vec().unwrap();
        let (decoded, size) = SdtpBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.entries, entries);
    }

    /// SdtpBox: version が 0 以外の場合はデコードエラー
    #[test]
    fn sdtp_box_invalid_version_decode_error(
        entries in prop::collection::vec(arb_sdtp_sample_flags(), 0..100),
        version in 1u8..=u8::MAX
    ) {
        let sdtp = SdtpBox { entries };
        let mut encoded = sdtp
            .encode_to_vec()
            .expect("sdtp test fixture must be encodable");
        encoded[8] = version; // full box version
        prop_assert!(SdtpBox::decode(&encoded).is_err());
    }

    // ===== StscBox のテスト =====

    /// StscBox の encode/decode roundtrip
    #[test]
    fn stsc_box_roundtrip(entries in prop::collection::vec(arb_stsc_entry(), 0..50)) {
        let stsc = StscBox { entries: entries.clone() };
        let encoded = stsc.encode_to_vec().unwrap();
        let (decoded, size) = StscBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            prop_assert_eq!(orig.first_chunk, dec.first_chunk);
            prop_assert_eq!(orig.sample_per_chunk, dec.sample_per_chunk);
            prop_assert_eq!(orig.sample_description_index, dec.sample_description_index);
        }
    }

    // ===== StcoBox のテスト =====

    /// StcoBox の encode/decode roundtrip
    #[test]
    fn stco_box_roundtrip(offsets in prop::collection::vec(any::<u32>(), 0..100)) {
        let stco = StcoBox { chunk_offsets: offsets.clone() };
        let encoded = stco.encode_to_vec().unwrap();
        let (decoded, size) = StcoBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.chunk_offsets, offsets);
    }

    // ===== Co64Box のテスト =====

    /// Co64Box の encode/decode roundtrip
    #[test]
    fn co64_box_roundtrip(offsets in prop::collection::vec(any::<u64>(), 0..100)) {
        let co64 = Co64Box { chunk_offsets: offsets.clone() };
        let encoded = co64.encode_to_vec().unwrap();
        let (decoded, size) = Co64Box::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.chunk_offsets, offsets);
    }

    // ===== ElstBox のテスト =====

    /// ElstBox (version 0) の encode/decode roundtrip
    #[test]
    fn elst_box_v0_roundtrip(entries in prop::collection::vec(arb_elst_entry_v0(), 0..20)) {
        let elst = ElstBox { entries: entries.clone() };
        let encoded = elst.encode_to_vec().unwrap();
        let (decoded, size) = ElstBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            prop_assert_eq!(orig.edit_duration, dec.edit_duration);
            prop_assert_eq!(orig.media_time, dec.media_time);
            prop_assert_eq!(orig.media_rate.integer, dec.media_rate.integer);
            prop_assert_eq!(orig.media_rate.fraction, dec.media_rate.fraction);
        }
    }

    /// ElstBox (version 1) の encode/decode roundtrip
    #[test]
    fn elst_box_v1_roundtrip(entries in prop::collection::vec(arb_elst_entry_v1(), 0..20)) {
        let elst = ElstBox { entries: entries.clone() };
        let encoded = elst.encode_to_vec().unwrap();
        let (decoded, size) = ElstBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            prop_assert_eq!(orig.edit_duration, dec.edit_duration);
            prop_assert_eq!(orig.media_time, dec.media_time);
            prop_assert_eq!(orig.media_rate.integer, dec.media_rate.integer);
            prop_assert_eq!(orig.media_rate.fraction, dec.media_rate.fraction);
        }
    }

    // ===== FtypBox のテスト =====

    /// FtypBox の encode/decode roundtrip
    #[test]
    fn ftyp_box_roundtrip(
        major_brand in arb_brand(),
        minor_version in any::<u32>(),
        compatible_brands in prop::collection::vec(arb_brand(), 0..10)
    ) {
        let ftyp = FtypBox {
            major_brand,
            minor_version,
            compatible_brands: compatible_brands.clone(),
        };
        let encoded = ftyp.encode_to_vec().unwrap();
        let (decoded, size) = FtypBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.major_brand.get(), major_brand.get());
        prop_assert_eq!(decoded.minor_version, minor_version);
        prop_assert_eq!(decoded.compatible_brands.len(), compatible_brands.len());
        for (orig, dec) in compatible_brands.iter().zip(decoded.compatible_brands.iter()) {
            prop_assert_eq!(orig.get(), dec.get());
        }
    }

    // ===== MvhdBox のテスト =====

    /// MvhdBox (version 0) の encode/decode roundtrip
    #[test]
    fn mvhd_box_v0_roundtrip(
        creation_time in 0u64..=(u32::MAX as u64),
        modification_time in 0u64..=(u32::MAX as u64),
        timescale in 1u32..=u32::MAX,
        duration in 0u64..=(u32::MAX as u64),
        rate_int in any::<i16>(),
        rate_frac in any::<u16>(),
        volume_int in any::<i8>(),
        volume_frac in any::<u8>(),
        matrix in any::<[i32; 9]>(),
        next_track_id in any::<u32>()
    ) {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).unwrap(),
            duration,
            rate: FixedPointNumber::new(rate_int, rate_frac),
            volume: FixedPointNumber::new(volume_int, volume_frac),
            matrix,
            next_track_id,
        };
        let encoded = mvhd.encode_to_vec().unwrap();
        let (decoded, size) = MvhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.creation_time.as_secs(), creation_time);
        prop_assert_eq!(decoded.modification_time.as_secs(), modification_time);
        prop_assert_eq!(decoded.timescale.get(), timescale);
        prop_assert_eq!(decoded.duration, duration);
        prop_assert_eq!(decoded.rate.integer, rate_int);
        prop_assert_eq!(decoded.rate.fraction, rate_frac);
        prop_assert_eq!(decoded.volume.integer, volume_int);
        prop_assert_eq!(decoded.volume.fraction, volume_frac);
        prop_assert_eq!(decoded.matrix, matrix);
        prop_assert_eq!(decoded.next_track_id, next_track_id);
    }

    /// MvhdBox (version 1) の encode/decode roundtrip
    #[test]
    fn mvhd_box_v1_roundtrip(
        creation_time in any::<u64>(),
        modification_time in any::<u64>(),
        timescale in 1u32..=u32::MAX,
        duration in any::<u64>(),
        rate_int in any::<i16>(),
        rate_frac in any::<u16>(),
        volume_int in any::<i8>(),
        volume_frac in any::<u8>(),
        matrix in any::<[i32; 9]>(),
        next_track_id in any::<u32>()
    ) {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).unwrap(),
            duration,
            rate: FixedPointNumber::new(rate_int, rate_frac),
            volume: FixedPointNumber::new(volume_int, volume_frac),
            matrix,
            next_track_id,
        };
        let encoded = mvhd.encode_to_vec().unwrap();
        let (decoded, size) = MvhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.creation_time.as_secs(), creation_time);
        prop_assert_eq!(decoded.modification_time.as_secs(), modification_time);
        prop_assert_eq!(decoded.timescale.get(), timescale);
        prop_assert_eq!(decoded.duration, duration);
        prop_assert_eq!(decoded.rate.integer, rate_int);
        prop_assert_eq!(decoded.rate.fraction, rate_frac);
        prop_assert_eq!(decoded.volume.integer, volume_int);
        prop_assert_eq!(decoded.volume.fraction, volume_frac);
        prop_assert_eq!(decoded.matrix, matrix);
        prop_assert_eq!(decoded.next_track_id, next_track_id);
    }

    // ===== TkhdBox のテスト =====

    /// TkhdBox (version 0) の encode/decode roundtrip
    #[test]
    fn tkhd_box_v0_roundtrip(
        flag_track_enabled in any::<bool>(),
        flag_track_in_movie in any::<bool>(),
        flag_track_in_preview in any::<bool>(),
        flag_track_size_is_aspect_ratio in any::<bool>(),
        creation_time in 0u64..=(u32::MAX as u64),
        modification_time in 0u64..=(u32::MAX as u64),
        track_id in any::<u32>(),
        duration in 0u64..=(u32::MAX as u64),
        layer in any::<i16>(),
        alternate_group in any::<i16>(),
        volume_int in any::<i8>(),
        volume_frac in any::<u8>(),
        matrix in any::<[i32; 9]>(),
        width_int in any::<i16>(),
        width_frac in any::<u16>(),
        height_int in any::<i16>(),
        height_frac in any::<u16>()
    ) {
        let tkhd = TkhdBox {
            flag_track_enabled,
            flag_track_in_movie,
            flag_track_in_preview,
            flag_track_size_is_aspect_ratio,
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            track_id,
            duration,
            layer,
            alternate_group,
            volume: FixedPointNumber::new(volume_int, volume_frac),
            matrix,
            width: FixedPointNumber::new(width_int, width_frac),
            height: FixedPointNumber::new(height_int, height_frac),
        };
        let encoded = tkhd.encode_to_vec().unwrap();
        let (decoded, size) = TkhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.flag_track_enabled, flag_track_enabled);
        prop_assert_eq!(decoded.flag_track_in_movie, flag_track_in_movie);
        prop_assert_eq!(decoded.flag_track_in_preview, flag_track_in_preview);
        prop_assert_eq!(decoded.flag_track_size_is_aspect_ratio, flag_track_size_is_aspect_ratio);
        prop_assert_eq!(decoded.creation_time.as_secs(), creation_time);
        prop_assert_eq!(decoded.modification_time.as_secs(), modification_time);
        prop_assert_eq!(decoded.track_id, track_id);
        prop_assert_eq!(decoded.duration, duration);
        prop_assert_eq!(decoded.layer, layer);
        prop_assert_eq!(decoded.alternate_group, alternate_group);
        prop_assert_eq!(decoded.volume.integer, volume_int);
        prop_assert_eq!(decoded.volume.fraction, volume_frac);
        prop_assert_eq!(decoded.matrix, matrix);
        prop_assert_eq!(decoded.width.integer, width_int);
        prop_assert_eq!(decoded.width.fraction, width_frac);
        prop_assert_eq!(decoded.height.integer, height_int);
        prop_assert_eq!(decoded.height.fraction, height_frac);
    }

    // ===== MdhdBox のテスト =====

    /// MdhdBox (version 0) の encode/decode roundtrip
    #[test]
    fn mdhd_box_v0_roundtrip(
        creation_time in 0u64..=(u32::MAX as u64),
        modification_time in 0u64..=(u32::MAX as u64),
        timescale in 1u32..=u32::MAX,
        duration in 0u64..=(u32::MAX as u64),
        language in prop::array::uniform3(0x61u8..=0x7Au8)
    ) {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).unwrap(),
            duration,
            language,
        };
        let encoded = mdhd.encode_to_vec().unwrap();
        let (decoded, size) = MdhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.creation_time.as_secs(), creation_time);
        prop_assert_eq!(decoded.modification_time.as_secs(), modification_time);
        prop_assert_eq!(decoded.timescale.get(), timescale);
        prop_assert_eq!(decoded.duration, duration);
        prop_assert_eq!(decoded.language, language);
    }

    /// MdhdBox (version 1) の encode/decode roundtrip
    #[test]
    fn mdhd_box_v1_roundtrip(
        creation_time in any::<u64>(),
        modification_time in any::<u64>(),
        timescale in 1u32..=u32::MAX,
        duration in any::<u64>(),
        language in prop::array::uniform3(0x61u8..=0x7Au8)
    ) {
        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).unwrap(),
            duration,
            language,
        };
        let encoded = mdhd.encode_to_vec().unwrap();
        let (decoded, size) = MdhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.creation_time.as_secs(), creation_time);
        prop_assert_eq!(decoded.modification_time.as_secs(), modification_time);
        prop_assert_eq!(decoded.timescale.get(), timescale);
        prop_assert_eq!(decoded.duration, duration);
        prop_assert_eq!(decoded.language, language);
    }

    // ===== HdlrBox のテスト =====

    /// HdlrBox の encode/decode roundtrip
    #[test]
    fn hdlr_box_roundtrip(
        handler_type in any::<[u8; 4]>(),
        name in prop::collection::vec(any::<u8>(), 0..100)
    ) {
        let hdlr = HdlrBox {
            handler_type,
            name: name.clone(),
        };
        let encoded = hdlr.encode_to_vec().unwrap();
        let (decoded, size) = HdlrBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.handler_type, handler_type);
        prop_assert_eq!(decoded.name, name);
    }

    // ===== SmhdBox のテスト =====

    /// SmhdBox の encode/decode roundtrip
    #[test]
    fn smhd_box_roundtrip(
        balance_int in any::<u8>(),
        balance_frac in any::<u8>()
    ) {
        let smhd = SmhdBox {
            balance: FixedPointNumber::new(balance_int, balance_frac),
        };
        let encoded = smhd.encode_to_vec().unwrap();
        let (decoded, size) = SmhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.balance.integer, balance_int);
        prop_assert_eq!(decoded.balance.fraction, balance_frac);
    }

    // ===== VmhdBox のテスト =====

    /// VmhdBox の encode/decode roundtrip
    #[test]
    fn vmhd_box_roundtrip(
        graphicsmode in any::<u16>(),
        opcolor in any::<[u16; 3]>()
    ) {
        let vmhd = VmhdBox {
            graphicsmode,
            opcolor,
        };
        let encoded = vmhd.encode_to_vec().unwrap();
        let (decoded, size) = VmhdBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.graphicsmode, graphicsmode);
        prop_assert_eq!(decoded.opcolor, opcolor);
    }

    // ===== StssBox のテスト =====

    /// StssBox の encode/decode roundtrip
    #[test]
    fn stss_box_roundtrip(sample_numbers in prop::collection::vec(1u32..=u32::MAX, 0..100)) {
        let stss = StssBox {
            sample_numbers: sample_numbers.iter().map(|&n| NonZeroU32::new(n).unwrap()).collect(),
        };
        let encoded = stss.encode_to_vec().unwrap();
        let (decoded, size) = StssBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.sample_numbers.len(), sample_numbers.len());
        for (orig, dec) in sample_numbers.iter().zip(decoded.sample_numbers.iter()) {
            prop_assert_eq!(*orig, dec.get());
        }
    }

    // ===== UrlBox のテスト =====

    /// UrlBox (ローカルファイル) の encode/decode roundtrip
    #[test]
    fn url_box_local_roundtrip(_dummy in Just(())) {
        let url = UrlBox::LOCAL_FILE;
        let encoded = url.encode_to_vec().unwrap();
        let (decoded, size) = UrlBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert!(decoded.location.is_none());
    }

    /// UrlBox (リモート URL) の encode/decode roundtrip
    #[test]
    fn url_box_remote_roundtrip(location in "[a-zA-Z0-9:/._-]{1,100}") {
        let url = UrlBox {
            location: Some(Utf8String::new(&location).unwrap()),
        };
        let encoded = url.encode_to_vec().unwrap();
        let (decoded, size) = UrlBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert_eq!(decoded.location.as_ref().map(|s| s.get()), Some(location.as_str()));
    }

    // ===== DrefBox のテスト =====

    /// DrefBox の encode/decode roundtrip
    #[test]
    fn dref_box_roundtrip(_dummy in Just(())) {
        // DrefBox::LOCAL_FILE は UrlBox::LOCAL_FILE を持つ
        let dref = DrefBox::LOCAL_FILE;
        let encoded = dref.encode_to_vec().unwrap();
        let (decoded, size) = DrefBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert!(decoded.url_box.is_some());
    }

    // ===== DinfBox のテスト =====

    /// DinfBox の encode/decode roundtrip
    #[test]
    fn dinf_box_roundtrip(_dummy in Just(())) {
        let dinf = DinfBox::LOCAL_FILE;
        let encoded = dinf.encode_to_vec().unwrap();
        let (decoded, size) = DinfBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert!(decoded.dref_box.url_box.is_some());
    }

    // ===== EdtsBox のテスト =====

    /// EdtsBox (空) の encode/decode roundtrip
    #[test]
    fn edts_box_empty_roundtrip(_dummy in Just(())) {
        let edts = EdtsBox {
            elst_box: None,
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().unwrap();
        let (decoded, size) = EdtsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert!(decoded.elst_box.is_none());
    }

    /// EdtsBox (ElstBox 付き) の encode/decode roundtrip
    #[test]
    fn edts_box_with_elst_roundtrip(entries in prop::collection::vec(arb_elst_entry_v0(), 0..10)) {
        let edts = EdtsBox {
            elst_box: Some(ElstBox { entries: entries.clone() }),
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().unwrap();
        let (decoded, size) = EdtsBox::decode(&encoded).unwrap();

        prop_assert_eq!(size, encoded.len());
        prop_assert!(decoded.elst_box.is_some());
        prop_assert_eq!(decoded.elst_box.unwrap().entries.len(), entries.len());
    }
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// SttsBox: 空のエントリリスト
    #[test]
    fn stts_box_empty() {
        let stts = SttsBox { entries: vec![] };
        let encoded = stts.encode_to_vec().unwrap();
        let (decoded, _) = SttsBox::decode(&encoded).unwrap();
        assert!(decoded.entries.is_empty());
    }

    /// StcoBox: 空のオフセットリスト
    #[test]
    fn stco_box_empty() {
        let stco = StcoBox {
            chunk_offsets: vec![],
        };
        let encoded = stco.encode_to_vec().unwrap();
        let (decoded, _) = StcoBox::decode(&encoded).unwrap();
        assert!(decoded.chunk_offsets.is_empty());
    }

    /// Co64Box: 空のオフセットリスト
    #[test]
    fn co64_box_empty() {
        let co64 = Co64Box {
            chunk_offsets: vec![],
        };
        let encoded = co64.encode_to_vec().unwrap();
        let (decoded, _) = Co64Box::decode(&encoded).unwrap();
        assert!(decoded.chunk_offsets.is_empty());
    }

    /// ElstBox: 空のエントリリスト
    #[test]
    fn elst_box_empty() {
        let elst = ElstBox { entries: vec![] };
        let encoded = elst.encode_to_vec().unwrap();
        let (decoded, _) = ElstBox::decode(&encoded).unwrap();
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
        let encoded = stts.encode_to_vec().unwrap();
        let (decoded, _) = SttsBox::decode(&encoded).unwrap();
        assert_eq!(decoded.entries[0].sample_count, u32::MAX);
        assert_eq!(decoded.entries[0].sample_delta, u32::MAX);
    }

    /// StscEntry: 最小値 (NonZeroU32 の制約)
    #[test]
    fn stsc_entry_min_values() {
        let stsc = StscBox {
            entries: vec![StscEntry {
                first_chunk: NonZeroU32::new(1).unwrap(),
                sample_per_chunk: 0,
                sample_description_index: NonZeroU32::new(1).unwrap(),
            }],
        };
        let encoded = stsc.encode_to_vec().unwrap();
        let (decoded, _) = StscBox::decode(&encoded).unwrap();
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
        let encoded = co64.encode_to_vec().unwrap();
        let (decoded, _) = Co64Box::decode(&encoded).unwrap();
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
        let encoded_v0 = elst_v0.encode_to_vec().unwrap();
        let (decoded_v0, _) = ElstBox::decode(&encoded_v0).unwrap();
        assert_eq!(decoded_v0.entries[0].edit_duration, u32::MAX as u64);

        // version 1 が必要な値
        let elst_v1 = ElstBox {
            entries: vec![ElstEntry {
                edit_duration: (u32::MAX as u64) + 1,
                media_time: (i32::MAX as i64) + 1,
                media_rate: FixedPointNumber::new(0, 0),
            }],
        };
        let encoded_v1 = elst_v1.encode_to_vec().unwrap();
        let (decoded_v1, _) = ElstBox::decode(&encoded_v1).unwrap();
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
        let encoded = ftyp.encode_to_vec().unwrap();
        let (decoded, _) = FtypBox::decode(&encoded).unwrap();
        assert_eq!(decoded.major_brand.get(), [0x00, 0x00, 0x00, 0x00]);
        assert_eq!(decoded.compatible_brands[0].get(), [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    /// MvhdBox: デフォルト値
    #[test]
    fn mvhd_box_defaults() {
        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(1000).unwrap(),
            duration: 0,
            rate: MvhdBox::DEFAULT_RATE,
            volume: MvhdBox::DEFAULT_VOLUME,
            matrix: MvhdBox::DEFAULT_MATRIX,
            next_track_id: 1,
        };
        let encoded = mvhd.encode_to_vec().unwrap();
        let (decoded, _) = MvhdBox::decode(&encoded).unwrap();
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
        let encoded = tkhd.encode_to_vec().unwrap();
        let (decoded, _) = TkhdBox::decode(&encoded).unwrap();
        assert!(decoded.flag_track_enabled);
        assert!(decoded.flag_track_in_movie);
        assert!(!decoded.flag_track_in_preview);
        assert!(decoded.flag_track_size_is_aspect_ratio);
    }

    /// MdhdBox: 言語コードの境界
    #[test]
    fn mdhd_box_language_boundary() {
        // 最小値 'aaa' (0x61)
        let mdhd_min = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).unwrap(),
            duration: 0,
            language: [0x61, 0x61, 0x61],
        };
        let encoded = mdhd_min.encode_to_vec().unwrap();
        let (decoded, _) = MdhdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.language, [0x61, 0x61, 0x61]);

        // 最大値 'zzz' (0x7A)
        let mdhd_max = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).unwrap(),
            duration: 0,
            language: [0x7A, 0x7A, 0x7A],
        };
        let encoded = mdhd_max.encode_to_vec().unwrap();
        let (decoded, _) = MdhdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.language, [0x7A, 0x7A, 0x7A]);

        // 標準的な "und" (undefined)
        let mdhd_und = MdhdBox {
            creation_time: Mp4FileTime::from_secs(0),
            modification_time: Mp4FileTime::from_secs(0),
            timescale: NonZeroU32::new(48000).unwrap(),
            duration: 0,
            language: MdhdBox::LANGUAGE_UNDEFINED,
        };
        let encoded = mdhd_und.encode_to_vec().unwrap();
        let (decoded, _) = MdhdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.language, *b"und");
    }

    /// HdlrBox: 空の name
    #[test]
    fn hdlr_box_empty_name() {
        let hdlr = HdlrBox {
            handler_type: HdlrBox::HANDLER_TYPE_VIDE,
            name: vec![],
        };
        let encoded = hdlr.encode_to_vec().unwrap();
        let (decoded, _) = HdlrBox::decode(&encoded).unwrap();
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
            let encoded = hdlr.encode_to_vec().unwrap();
            let (decoded, _) = HdlrBox::decode(&encoded).unwrap();
            assert_eq!(decoded.handler_type, handler_type);
        }
    }

    /// SmhdBox: デフォルト値
    #[test]
    fn smhd_box_default() {
        let smhd = SmhdBox {
            balance: SmhdBox::DEFAULT_BALANCE,
        };
        let encoded = smhd.encode_to_vec().unwrap();
        let (decoded, _) = SmhdBox::decode(&encoded).unwrap();
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
        let encoded = vmhd.encode_to_vec().unwrap();
        let (decoded, _) = VmhdBox::decode(&encoded).unwrap();
        assert_eq!(decoded.graphicsmode, 0);
        assert_eq!(decoded.opcolor, [0, 0, 0]);
    }

    /// StssBox: 空のリスト
    #[test]
    fn stss_box_empty() {
        let stss = StssBox {
            sample_numbers: vec![],
        };
        let encoded = stss.encode_to_vec().unwrap();
        let (decoded, _) = StssBox::decode(&encoded).unwrap();
        assert!(decoded.sample_numbers.is_empty());
    }

    /// StssBox: 最大値
    #[test]
    fn stss_box_max_value() {
        let stss = StssBox {
            sample_numbers: vec![NonZeroU32::MAX],
        };
        let encoded = stss.encode_to_vec().unwrap();
        let (decoded, _) = StssBox::decode(&encoded).unwrap();
        assert_eq!(decoded.sample_numbers[0], NonZeroU32::MAX);
    }

    /// UrlBox: ローカルファイル
    #[test]
    fn url_box_local_file() {
        let url = UrlBox::LOCAL_FILE;
        let encoded = url.encode_to_vec().unwrap();
        let (decoded, _) = UrlBox::decode(&encoded).unwrap();
        assert!(decoded.location.is_none());
    }

    /// DrefBox: ローカルファイル
    #[test]
    fn dref_box_local_file() {
        let dref = DrefBox::LOCAL_FILE;
        let encoded = dref.encode_to_vec().unwrap();
        let (decoded, _) = DrefBox::decode(&encoded).unwrap();
        assert!(decoded.url_box.is_some());
        assert!(decoded.url_box.unwrap().location.is_none());
    }

    /// DinfBox: ローカルファイル
    #[test]
    fn dinf_box_local_file() {
        let dinf = DinfBox::LOCAL_FILE;
        let encoded = dinf.encode_to_vec().unwrap();
        let (decoded, _) = DinfBox::decode(&encoded).unwrap();
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
        let encoded = edts.encode_to_vec().unwrap();
        let (decoded, _) = EdtsBox::decode(&encoded).unwrap();
        assert!(decoded.elst_box.is_none());
        assert!(decoded.unknown_boxes.is_empty());
    }
}
