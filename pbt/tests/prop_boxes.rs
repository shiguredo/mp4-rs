//! ボックス構造体の Property-Based Testing

use std::num::NonZeroU32;

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, LanguageCode, Mp4FileTime, Utf8String,
    boxes::{
        Brand, Co64Box, CslgBox, CttsBox, CttsEntry, DinfBox, DrefBox, EdtsBox, ElstBox, ElstEntry,
        FtypBox, HdlrBox, MdhdBox, MvhdBox, SdtpBox, SdtpSampleFlags, SmhdBox, StcoBox, StscBox,
        StscEntry, StssBox, SttsBox, SttsEntry, TkhdBox, UrlBox, VmhdBox,
    },
};

/// このファイルの主要 PBT ケース数（旧 `with_cases(500)` を維持）
const CASES: usize = 500;

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

/// CttsEntry (version 0 互換) を生成する
fn arb_ctts_entry_v0(ctx: &mut TestCaseContext) -> CttsEntry {
    CttsEntry {
        sample_count: noprop::sample_u32(ctx),
        sample_offset: noprop::sample_u32(ctx) as i64,
    }
}

/// CttsEntry (version 1) を生成する
fn arb_ctts_entry_v1(ctx: &mut TestCaseContext) -> CttsEntry {
    CttsEntry {
        sample_count: noprop::sample_u32(ctx),
        sample_offset: noprop::sample_i32(ctx) as i64,
    }
}

/// SdtpSampleFlags を生成する
fn arb_sdtp_sample_flags(ctx: &mut TestCaseContext) -> SdtpSampleFlags {
    let is_leading = noprop::sample_u64_in(ctx, 0..4) as u8;
    let sample_depends_on = noprop::sample_u64_in(ctx, 0..4) as u8;
    let sample_is_depended_on = noprop::sample_u64_in(ctx, 0..4) as u8;
    let sample_has_redundancy = noprop::sample_u64_in(ctx, 0..4) as u8;
    SdtpSampleFlags::from_fields(
        is_leading,
        sample_depends_on,
        sample_is_depended_on,
        sample_has_redundancy,
    )
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

/// ElstEntry (version 0 互換) を生成する
fn arb_elst_entry_v0(ctx: &mut TestCaseContext) -> ElstEntry {
    let edit_duration = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
    // (i32::MIN as i64)..=(i32::MAX as i64) を u64 で表現してから i64 に変換
    let media_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64)) as i64 + i32::MIN as i64;
    let rate_int = noprop::sample_i16(ctx);
    let rate_frac = noprop::sample_i16(ctx);
    ElstEntry {
        edit_duration,
        media_time,
        media_rate: FixedPointNumber::new(rate_int, rate_frac),
    }
}

/// ElstEntry (version 1) を生成する
fn arb_elst_entry_v1(ctx: &mut TestCaseContext) -> ElstEntry {
    ElstEntry {
        edit_duration: noprop::sample_u64(ctx),
        media_time: noprop::sample_i64(ctx),
        media_rate: FixedPointNumber::new(noprop::sample_i16(ctx), noprop::sample_i16(ctx)),
    }
}

/// 4 文字のブランド名を生成する
fn arb_brand(ctx: &mut TestCaseContext) -> Brand {
    let bytes = [
        noprop::sample_u64_in(ctx, 0x20..=0x7E) as u8,
        noprop::sample_u64_in(ctx, 0x20..=0x7E) as u8,
        noprop::sample_u64_in(ctx, 0x20..=0x7E) as u8,
        noprop::sample_u64_in(ctx, 0x20..=0x7E) as u8,
    ];
    Brand::new(bytes)
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

// ===== SttsBox のテスト =====

/// SttsBox の encode/decode roundtrip
#[test]
fn stts_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..50, arb_stts_entry);
        let stts = SttsBox {
            entries: entries.clone(),
        };
        let encoded = stts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SttsBox::decode(&encoded)
            .expect("直前にエンコードした有効な SttsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            assert_eq!(orig.sample_count, dec.sample_count);
            assert_eq!(orig.sample_delta, dec.sample_delta);
        }
        Ok(())
    })?;
    Ok(())
}

/// SttsBox::from_sample_deltas の不変条件: 連続する同じ delta は集約される
#[test]
fn stts_from_sample_deltas_invariant() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let deltas = sample_vec(ctx, 0..100, noprop::sample_u32);
        let stts = SttsBox::from_sample_deltas(deltas.iter().cloned())
            .expect("100 件以下の入力で sample_count が溢れることはない");

        // 隣接エントリは異なる sample_delta を持つ
        for window in stts.entries.windows(2) {
            assert_ne!(
                window[0].sample_delta, window[1].sample_delta,
                "隣接エントリが同じ sample_delta を持っている"
            );
        }

        // sample_count の合計が元の deltas 数と一致
        let total_count: u32 = stts.entries.iter().map(|e| e.sample_count).sum();
        assert_eq!(total_count as usize, deltas.len());
        Ok(())
    })?;
    Ok(())
}

// ===== CttsBox のテスト =====

/// CttsBox (version 0) の encode/decode roundtrip
#[test]
fn ctts_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..50, arb_ctts_entry_v0);
        let ctts = CttsBox {
            version: 0,
            entries: entries.clone(),
        };
        let encoded = ctts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = CttsBox::decode(&encoded)
            .expect("直前にエンコードした有効な CttsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.version, 0);
        assert_eq!(decoded.entries, entries);
        Ok(())
    })?;
    Ok(())
}

/// CttsBox (version 1) の encode/decode roundtrip
#[test]
fn ctts_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..50, arb_ctts_entry_v1);
        let ctts = CttsBox {
            version: 1,
            entries: entries.clone(),
        };
        let encoded = ctts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = CttsBox::decode(&encoded)
            .expect("直前にエンコードした有効な CttsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.entries, entries);
        Ok(())
    })?;
    Ok(())
}

/// CttsBox: version が 2 以上の場合はデコードエラー
#[test]
fn ctts_box_invalid_version_decode_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let version = noprop::sample_u64_in(ctx, 2..=u8::MAX as u64) as u8;
        let ctts = CttsBox {
            version: 1,
            entries: vec![CttsEntry {
                sample_count: 1,
                sample_offset: 0,
            }],
        };
        let mut encoded = ctts
            .encode_to_vec()
            .expect("ctts テスト fixture はエンコードできる");
        encoded[8] = version; // full box version
        assert!(CttsBox::decode(&encoded).is_err());
        Ok(())
    })?;
    Ok(())
}

/// CttsBox: version 0 で負の sample_offset をエンコードするとエラー
#[test]
fn ctts_box_v0_negative_offset_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let sample_count = noprop::sample_u32(ctx);
        // i64::MIN..0 の負値
        let sample_offset = noprop::sample_i64(ctx).min(-1);
        let ctts = CttsBox {
            version: 0,
            entries: vec![CttsEntry {
                sample_count,
                sample_offset,
            }],
        };
        assert!(ctts.encode_to_vec().is_err());
        Ok(())
    })?;
    Ok(())
}

// ===== CslgBox のテスト =====

/// CslgBox (version 0) の encode/decode roundtrip
#[test]
fn cslg_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let composition_to_dts_shift = noprop::sample_i32(ctx);
        let least_decode_to_display_delta = noprop::sample_i32(ctx);
        let greatest_decode_to_display_delta = noprop::sample_i32(ctx);
        let composition_start_time = noprop::sample_i32(ctx);
        let composition_end_time = noprop::sample_i32(ctx);
        let cslg = CslgBox {
            version: 0,
            composition_to_dts_shift: composition_to_dts_shift as i64,
            least_decode_to_display_delta: least_decode_to_display_delta as i64,
            greatest_decode_to_display_delta: greatest_decode_to_display_delta as i64,
            composition_start_time: composition_start_time as i64,
            composition_end_time: composition_end_time as i64,
        };
        let encoded = cslg.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = CslgBox::decode(&encoded)
            .expect("直前にエンコードした有効な CslgBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, cslg);
        Ok(())
    })?;
    Ok(())
}

/// CslgBox (version 1) の encode/decode roundtrip
#[test]
fn cslg_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let cslg = CslgBox {
            version: 1,
            composition_to_dts_shift: noprop::sample_i64(ctx),
            least_decode_to_display_delta: noprop::sample_i64(ctx),
            greatest_decode_to_display_delta: noprop::sample_i64(ctx),
            composition_start_time: noprop::sample_i64(ctx),
            composition_end_time: noprop::sample_i64(ctx),
        };
        let encoded = cslg.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = CslgBox::decode(&encoded)
            .expect("直前にエンコードした有効な CslgBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, cslg);
        Ok(())
    })?;
    Ok(())
}

/// CslgBox: version が 2 以上の場合はデコードエラー
#[test]
fn cslg_box_invalid_version_decode_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let version = noprop::sample_u64_in(ctx, 2..=u8::MAX as u64) as u8;
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
            .expect("cslg テスト fixture はエンコードできる");
        encoded[8] = version; // full box version
        assert!(CslgBox::decode(&encoded).is_err());
        Ok(())
    })?;
    Ok(())
}

// ===== SdtpBox のテスト =====

/// SdtpBox の encode/decode roundtrip
#[test]
fn sdtp_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..100, arb_sdtp_sample_flags);
        let sdtp = SdtpBox {
            entries: entries.clone(),
        };
        let encoded = sdtp.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SdtpBox::decode(&encoded)
            .expect("直前にエンコードした有効な SdtpBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.entries, entries);
        Ok(())
    })?;
    Ok(())
}

/// SdtpBox: version が 0 以外の場合はデコードエラー
#[test]
fn sdtp_box_invalid_version_decode_error() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..100, arb_sdtp_sample_flags);
        let version = noprop::sample_u64_in(ctx, 1..=u8::MAX as u64) as u8;
        let sdtp = SdtpBox { entries };
        let mut encoded = sdtp
            .encode_to_vec()
            .expect("sdtp テスト fixture はエンコードできる");
        encoded[8] = version; // full box version
        assert!(SdtpBox::decode(&encoded).is_err());
        Ok(())
    })?;
    Ok(())
}

// ===== StscBox のテスト =====

/// StscBox の encode/decode roundtrip
#[test]
fn stsc_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..50, arb_stsc_entry);
        let stsc = StscBox {
            entries: entries.clone(),
        };
        let encoded = stsc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = StscBox::decode(&encoded)
            .expect("直前にエンコードした有効な StscBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            assert_eq!(orig.first_chunk, dec.first_chunk);
            assert_eq!(orig.sample_per_chunk, dec.sample_per_chunk);
            assert_eq!(orig.sample_description_index, dec.sample_description_index);
        }
        Ok(())
    })?;
    Ok(())
}

// ===== StcoBox のテスト =====

/// StcoBox の encode/decode roundtrip
#[test]
fn stco_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let offsets = sample_vec(ctx, 0..100, noprop::sample_u32);
        let stco = StcoBox {
            chunk_offsets: offsets.clone(),
        };
        let encoded = stco.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = StcoBox::decode(&encoded)
            .expect("直前にエンコードした有効な StcoBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.chunk_offsets, offsets);
        Ok(())
    })?;
    Ok(())
}

// ===== Co64Box のテスト =====

/// Co64Box の encode/decode roundtrip
#[test]
fn co64_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let offsets = sample_vec(ctx, 0..100, noprop::sample_u64);
        let co64 = Co64Box {
            chunk_offsets: offsets.clone(),
        };
        let encoded = co64.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = Co64Box::decode(&encoded)
            .expect("直前にエンコードした有効な Co64Box は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.chunk_offsets, offsets);
        Ok(())
    })?;
    Ok(())
}

// ===== ElstBox のテスト =====

/// ElstBox (version 0) の encode/decode roundtrip
#[test]
fn elst_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..20, arb_elst_entry_v0);
        let elst = ElstBox {
            entries: entries.clone(),
        };
        let encoded = elst.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = ElstBox::decode(&encoded)
            .expect("直前にエンコードした有効な ElstBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            assert_eq!(orig.edit_duration, dec.edit_duration);
            assert_eq!(orig.media_time, dec.media_time);
            assert_eq!(orig.media_rate.integer, dec.media_rate.integer);
            assert_eq!(orig.media_rate.fraction, dec.media_rate.fraction);
        }
        Ok(())
    })?;
    Ok(())
}

/// ElstBox (version 1) の encode/decode roundtrip
#[test]
fn elst_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..20, arb_elst_entry_v1);
        let elst = ElstBox {
            entries: entries.clone(),
        };
        let encoded = elst.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = ElstBox::decode(&encoded)
            .expect("直前にエンコードした有効な ElstBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.entries.len(), entries.len());
        for (orig, dec) in entries.iter().zip(decoded.entries.iter()) {
            assert_eq!(orig.edit_duration, dec.edit_duration);
            assert_eq!(orig.media_time, dec.media_time);
            assert_eq!(orig.media_rate.integer, dec.media_rate.integer);
            assert_eq!(orig.media_rate.fraction, dec.media_rate.fraction);
        }
        Ok(())
    })?;
    Ok(())
}

// ===== FtypBox のテスト =====

/// FtypBox の encode/decode roundtrip
#[test]
fn ftyp_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let major_brand = arb_brand(ctx);
        let minor_version = noprop::sample_u32(ctx);
        let compatible_brands = sample_vec(ctx, 0..10, arb_brand);
        let ftyp = FtypBox {
            major_brand,
            minor_version,
            compatible_brands: compatible_brands.clone(),
        };
        let encoded = ftyp.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = FtypBox::decode(&encoded)
            .expect("直前にエンコードした有効な FtypBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.major_brand.get(), major_brand.get());
        assert_eq!(decoded.minor_version, minor_version);
        assert_eq!(decoded.compatible_brands.len(), compatible_brands.len());
        for (orig, dec) in compatible_brands
            .iter()
            .zip(decoded.compatible_brands.iter())
        {
            assert_eq!(orig.get(), dec.get());
        }
        Ok(())
    })?;
    Ok(())
}

// ===== MvhdBox のテスト =====

/// MvhdBox (version 0) の encode/decode roundtrip
#[test]
fn mvhd_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let creation_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let modification_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let rate_int = noprop::sample_i16(ctx);
        let rate_frac = noprop::sample_u16(ctx);
        let volume_int = noprop::sample_i8(ctx);
        let volume_frac = noprop::sample_u8(ctx);
        let mut matrix = [0i32; 9];
        for m in &mut matrix {
            *m = noprop::sample_i32(ctx);
        }
        let next_track_id = noprop::sample_u32(ctx);

        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
            duration,
            rate: FixedPointNumber::new(rate_int, rate_frac),
            volume: FixedPointNumber::new(volume_int, volume_frac),
            matrix,
            next_track_id,
        };
        let encoded = mvhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MvhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.creation_time.as_secs(), creation_time);
        assert_eq!(decoded.modification_time.as_secs(), modification_time);
        assert_eq!(decoded.timescale.get(), timescale);
        assert_eq!(decoded.duration, duration);
        assert_eq!(decoded.rate.integer, rate_int);
        assert_eq!(decoded.rate.fraction, rate_frac);
        assert_eq!(decoded.volume.integer, volume_int);
        assert_eq!(decoded.volume.fraction, volume_frac);
        assert_eq!(decoded.matrix, matrix);
        assert_eq!(decoded.next_track_id, next_track_id);
        Ok(())
    })?;
    Ok(())
}

/// MvhdBox (version 1) の encode/decode roundtrip
#[test]
fn mvhd_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let creation_time = noprop::sample_u64(ctx);
        let modification_time = noprop::sample_u64(ctx);
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64(ctx);
        let rate_int = noprop::sample_i16(ctx);
        let rate_frac = noprop::sample_u16(ctx);
        let volume_int = noprop::sample_i8(ctx);
        let volume_frac = noprop::sample_u8(ctx);
        let mut matrix = [0i32; 9];
        for m in &mut matrix {
            *m = noprop::sample_i32(ctx);
        }
        let next_track_id = noprop::sample_u32(ctx);

        let mvhd = MvhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
            duration,
            rate: FixedPointNumber::new(rate_int, rate_frac),
            volume: FixedPointNumber::new(volume_int, volume_frac),
            matrix,
            next_track_id,
        };
        let encoded = mvhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MvhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MvhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.creation_time.as_secs(), creation_time);
        assert_eq!(decoded.modification_time.as_secs(), modification_time);
        assert_eq!(decoded.timescale.get(), timescale);
        assert_eq!(decoded.duration, duration);
        assert_eq!(decoded.rate.integer, rate_int);
        assert_eq!(decoded.rate.fraction, rate_frac);
        assert_eq!(decoded.volume.integer, volume_int);
        assert_eq!(decoded.volume.fraction, volume_frac);
        assert_eq!(decoded.matrix, matrix);
        assert_eq!(decoded.next_track_id, next_track_id);
        Ok(())
    })?;
    Ok(())
}

// ===== TkhdBox のテスト =====

/// TkhdBox (version 0) の encode/decode roundtrip
#[test]
fn tkhd_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let flag_track_enabled = noprop::sample_bool(ctx);
        let flag_track_in_movie = noprop::sample_bool(ctx);
        let flag_track_in_preview = noprop::sample_bool(ctx);
        let flag_track_size_is_aspect_ratio = noprop::sample_bool(ctx);
        let creation_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let modification_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let track_id = noprop::sample_u32(ctx);
        let duration = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let layer = noprop::sample_i16(ctx);
        let alternate_group = noprop::sample_i16(ctx);
        let volume_int = noprop::sample_i8(ctx);
        let volume_frac = noprop::sample_u8(ctx);
        let mut matrix = [0i32; 9];
        for m in &mut matrix {
            *m = noprop::sample_i32(ctx);
        }
        let width_int = noprop::sample_i16(ctx);
        let width_frac = noprop::sample_u16(ctx);
        let height_int = noprop::sample_i16(ctx);
        let height_frac = noprop::sample_u16(ctx);

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
        let encoded = tkhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TkhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な TkhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.flag_track_enabled, flag_track_enabled);
        assert_eq!(decoded.flag_track_in_movie, flag_track_in_movie);
        assert_eq!(decoded.flag_track_in_preview, flag_track_in_preview);
        assert_eq!(
            decoded.flag_track_size_is_aspect_ratio,
            flag_track_size_is_aspect_ratio
        );
        assert_eq!(decoded.creation_time.as_secs(), creation_time);
        assert_eq!(decoded.modification_time.as_secs(), modification_time);
        assert_eq!(decoded.track_id, track_id);
        assert_eq!(decoded.duration, duration);
        assert_eq!(decoded.layer, layer);
        assert_eq!(decoded.alternate_group, alternate_group);
        assert_eq!(decoded.volume.integer, volume_int);
        assert_eq!(decoded.volume.fraction, volume_frac);
        assert_eq!(decoded.matrix, matrix);
        assert_eq!(decoded.width.integer, width_int);
        assert_eq!(decoded.width.fraction, width_frac);
        assert_eq!(decoded.height.integer, height_int);
        assert_eq!(decoded.height.fraction, height_frac);
        Ok(())
    })?;
    Ok(())
}

// ===== MdhdBox のテスト =====

/// MdhdBox (version 0) の encode/decode roundtrip
#[test]
fn mdhd_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let creation_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let modification_time = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64));
        let language = arb_language_code_lower(ctx);

        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
            duration,
            language,
        };
        let encoded = mdhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MdhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MdhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.creation_time.as_secs(), creation_time);
        assert_eq!(decoded.modification_time.as_secs(), modification_time);
        assert_eq!(decoded.timescale.get(), timescale);
        assert_eq!(decoded.duration, duration);
        assert_eq!(decoded.language, language);
        Ok(())
    })?;
    Ok(())
}

/// MdhdBox (version 1) の encode/decode roundtrip
#[test]
fn mdhd_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let creation_time = noprop::sample_u64(ctx);
        let modification_time = noprop::sample_u64(ctx);
        let timescale = noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32;
        let duration = noprop::sample_u64(ctx);
        let language = arb_language_code_lower(ctx);

        let mdhd = MdhdBox {
            creation_time: Mp4FileTime::from_secs(creation_time),
            modification_time: Mp4FileTime::from_secs(modification_time),
            timescale: NonZeroU32::new(timescale).expect("サンプル値域が 1 以上なので非ゼロ"),
            duration,
            language,
        };
        let encoded = mdhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = MdhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な MdhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.creation_time.as_secs(), creation_time);
        assert_eq!(decoded.modification_time.as_secs(), modification_time);
        assert_eq!(decoded.timescale.get(), timescale);
        assert_eq!(decoded.duration, duration);
        assert_eq!(decoded.language, language);
        Ok(())
    })?;
    Ok(())
}

// ===== HdlrBox のテスト =====

/// HdlrBox の encode/decode roundtrip
#[test]
fn hdlr_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let handler_type = noprop::sample_bytes::<4>(ctx);
        let name_len = noprop::sample_usize_in(ctx, 0..100);
        let name = noprop::sample_bytes_vec(ctx, name_len);
        let hdlr = HdlrBox {
            handler_type,
            name: name.clone(),
        };
        let encoded = hdlr.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = HdlrBox::decode(&encoded)
            .expect("直前にエンコードした有効な HdlrBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.handler_type, handler_type);
        assert_eq!(decoded.name, name);
        Ok(())
    })?;
    Ok(())
}

// ===== SmhdBox のテスト =====

/// SmhdBox の encode/decode roundtrip
#[test]
fn smhd_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let balance_int = noprop::sample_u8(ctx);
        let balance_frac = noprop::sample_u8(ctx);
        let smhd = SmhdBox {
            balance: FixedPointNumber::new(balance_int, balance_frac),
        };
        let encoded = smhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SmhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な SmhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.balance.integer, balance_int);
        assert_eq!(decoded.balance.fraction, balance_frac);
        Ok(())
    })?;
    Ok(())
}

// ===== VmhdBox のテスト =====

/// VmhdBox の encode/decode roundtrip
#[test]
fn vmhd_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let graphicsmode = noprop::sample_u16(ctx);
        let opcolor = [
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
            noprop::sample_u16(ctx),
        ];
        let vmhd = VmhdBox {
            graphicsmode,
            opcolor,
        };
        let encoded = vmhd.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = VmhdBox::decode(&encoded)
            .expect("直前にエンコードした有効な VmhdBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.graphicsmode, graphicsmode);
        assert_eq!(decoded.opcolor, opcolor);
        Ok(())
    })?;
    Ok(())
}

// ===== StssBox のテスト =====

/// StssBox の encode/decode roundtrip
#[test]
fn stss_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let sample_numbers = sample_vec(ctx, 0..100, |ctx| {
            noprop::sample_u64_in(ctx, 1..=u32::MAX as u64) as u32
        });
        let stss = StssBox {
            sample_numbers: sample_numbers
                .iter()
                .map(|&n| NonZeroU32::new(n).expect("サンプル値域が 1 以上なので非ゼロ"))
                .collect(),
        };
        let encoded = stss.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = StssBox::decode(&encoded)
            .expect("直前にエンコードした有効な StssBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.sample_numbers.len(), sample_numbers.len());
        for (orig, dec) in sample_numbers.iter().zip(decoded.sample_numbers.iter()) {
            assert_eq!(*orig, dec.get());
        }
        Ok(())
    })?;
    Ok(())
}

// ===== UrlBox のテスト =====

/// UrlBox (ローカルファイル) の encode/decode roundtrip
///
/// 生成側の分岐は無いが、shape を PBT に合わせるため CASES 回 assert する
#[test]
fn url_box_local_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |_ctx| {
        let url = UrlBox::LOCAL_FILE;
        let encoded = url.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = UrlBox::decode(&encoded)
            .expect("直前にエンコードした有効な UrlBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(decoded.location.is_none());
        Ok(())
    })?;
    Ok(())
}

/// UrlBox (リモート URL) の encode/decode roundtrip
#[test]
fn url_box_remote_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        // 旧 regex `[a-zA-Z0-9:/._-]{1,100}` に相当する ASCII 部分集合
        let len = noprop::sample_usize_in(ctx, 1..=100);
        let alphabet: &[u8] =
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:/._-";
        let mut location = String::with_capacity(len);
        for _ in 0..len {
            let idx = noprop::sample_usize_in(ctx, 0..alphabet.len());
            location.push(alphabet[idx] as char);
        }

        let url = UrlBox {
            location: Some(
                Utf8String::new(&location)
                    .expect("サンプラーで null 文字を含まない ASCII のみ生成"),
            ),
        };
        let encoded = url.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = UrlBox::decode(&encoded)
            .expect("直前にエンコードした有効な UrlBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(
            decoded.location.as_ref().map(|s| s.get()),
            Some(location.as_str())
        );
        Ok(())
    })?;
    Ok(())
}

// ===== DrefBox のテスト =====

/// DrefBox の encode/decode roundtrip
#[test]
fn dref_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |_ctx| {
        // DrefBox::LOCAL_FILE は UrlBox::LOCAL_FILE を持つ
        let dref = DrefBox::LOCAL_FILE;
        let encoded = dref.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = DrefBox::decode(&encoded)
            .expect("直前にエンコードした有効な DrefBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(decoded.url_box.is_some());
        Ok(())
    })?;
    Ok(())
}

// ===== DinfBox のテスト =====

/// DinfBox の encode/decode roundtrip
#[test]
fn dinf_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |_ctx| {
        let dinf = DinfBox::LOCAL_FILE;
        let encoded = dinf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = DinfBox::decode(&encoded)
            .expect("直前にエンコードした有効な DinfBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(decoded.dref_box.url_box.is_some());
        Ok(())
    })?;
    Ok(())
}

// ===== EdtsBox のテスト =====

/// EdtsBox (空) の encode/decode roundtrip
#[test]
fn edts_box_empty_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |_ctx| {
        let edts = EdtsBox {
            elst_box: None,
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = EdtsBox::decode(&encoded)
            .expect("直前にエンコードした有効な EdtsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(decoded.elst_box.is_none());
        Ok(())
    })?;
    Ok(())
}

/// EdtsBox (ElstBox 付き) の encode/decode roundtrip
#[test]
fn edts_box_with_elst_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let entries = sample_vec(ctx, 0..10, arb_elst_entry_v0);
        let edts = EdtsBox {
            elst_box: Some(ElstBox {
                entries: entries.clone(),
            }),
            unknown_boxes: vec![],
        };
        let encoded = edts.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = EdtsBox::decode(&encoded)
            .expect("直前にエンコードした有効な EdtsBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(decoded.elst_box.is_some());
        assert_eq!(
            decoded
                .elst_box
                .expect("直前の assert! で Some であることを確認済み")
                .entries
                .len(),
            entries.len()
        );
        Ok(())
    })?;
    Ok(())
}
