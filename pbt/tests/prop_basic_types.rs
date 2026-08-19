//! 基本型の Property-Based Testing

use noprop::TestCaseContext;
use shiguredo_mp4::{
    BoxHeader, BoxSize, BoxType, Decode, Encode, FixedPointNumber, FullBoxFlags, FullBoxHeader,
    Mp4FileTime, Uint, Utf8String,
};

/// このファイルの主要 PBT ケース数（旧 `with_cases(1000)` を維持）
const CASES: usize = 1000;

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

/// FullBoxFlags の値を生成する (24 ビット)
fn arb_full_box_flags(ctx: &mut TestCaseContext) -> u32 {
    noprop::sample_u64_in(ctx, 0..=0x00FF_FFFF) as u32
}

/// FullBoxFlags のビット位置を生成する
///
/// `u32` の型幅 (32) 前後の境界値を確実にサンプリングしつつ、
/// 任意の `usize` 値も混ぜて広く探索する。
/// 32 境界のガード条件（`is_set` / `from_flags` の 32 以上を無視する挙動）を
/// 確率的に踏むための構成。
fn arb_bit_position(ctx: &mut TestCaseContext) -> usize {
    // 旧 `prop_oneof!` の 6 分岐（境界 5 + 任意 1）を等確率選択
    noprop::sample_with_boundaries(
        ctx,
        &[0usize, 31, 32, 33, usize::MAX],
        noprop::Ratio::new(5, 6),
        noprop::sample_usize,
    )
}

/// BoxType::Normal 用の 4 バイト値を生成する
fn arb_box_type_normal(ctx: &mut TestCaseContext) -> [u8; 4] {
    noprop::sample_bytes::<4>(ctx)
}

/// BoxType::Uuid 用の 16 バイト値を生成する
fn arb_box_type_uuid(ctx: &mut TestCaseContext) -> [u8; 16] {
    noprop::sample_bytes::<16>(ctx)
}

/// BoxSize::U32 用の値を生成する (ヘッダーサイズ 8 以上、または 0)
fn arb_box_size_u32(ctx: &mut TestCaseContext) -> u32 {
    // 旧 `prop_oneof![Just(0), 8..=u32::MAX]` の 2 分岐を等確率選択
    noprop::sample_with_boundaries(ctx, &[0u32], noprop::Ratio::new(1, 2), |ctx| {
        noprop::sample_u64_in(ctx, 8..=u32::MAX as u64) as u32
    })
}

/// BoxSize::U64 用の値を生成する (4GB 超、またはゼロ)
fn arb_box_size_u64(ctx: &mut TestCaseContext) -> u64 {
    noprop::sample_with_boundaries(ctx, &[0u64], noprop::Ratio::new(1, 2), |ctx| {
        noprop::sample_u64_in(ctx, ((u32::MAX as u64) + 1)..=u64::MAX)
    })
}

/// null を含まない UTF-8 文字列を生成する
fn arb_utf8_string(ctx: &mut TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=100);
    let mut s = String::new();
    while s.chars().count() < len {
        let c = noprop::sample_char(ctx);
        if c != '\0' {
            s.push(c);
        }
    }
    s
}

// FullBoxFlags の Roundtrip
#[test]
fn full_box_flags_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = arb_full_box_flags(ctx);
        let flags = FullBoxFlags::new(value);
        let encoded = flags.encode_to_vec().expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), 3);

        let (decoded, size) = FullBoxFlags::decode(&encoded)
            .expect("直前にエンコードした 3 バイト表現は必ずデコードできる");
        assert_eq!(size, 3);
        assert_eq!(decoded.get(), flags.get());
        Ok(())
    })?;
    Ok(())
}

// FullBoxFlags::from_flags の検証 (各ビット位置は一度だけ)
#[test]
fn full_box_flags_from_flags() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let bit_mask = noprop::sample_u32(ctx);
        // 24 ビットのマスクから (bit_position, is_set) のリストを生成
        let bits: Vec<(usize, bool)> = (0..24).map(|i| (i, (bit_mask & (1 << i)) != 0)).collect();
        let flags = FullBoxFlags::from_flags(bits);

        for i in 0..24 {
            let expected = (bit_mask & (1 << i)) != 0;
            assert_eq!(flags.is_set(i), expected, "bit {i} が一致しない");
        }
        Ok(())
    })?;
    Ok(())
}

// FullBoxFlags::is_set の任意ビット位置に対する挙動を検証する
#[test]
fn full_box_flags_is_set_any_bit_position() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let flags = noprop::sample_u32(ctx);
        let i = arb_bit_position(ctx);
        let fbf = FullBoxFlags::new(flags);
        let expected = if i < 32 { (flags >> i) & 1 == 1 } else { false };
        assert_eq!(
            fbf.is_set(i),
            expected,
            "フラグ不一致 flags={flags:#x} i={i}"
        );
        Ok(())
    })?;
    Ok(())
}

// FullBoxFlags::from_flags の任意ビット位置に対する挙動を検証する
#[test]
fn full_box_flags_from_flags_any_bit_position() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let i = arb_bit_position(ctx);
        let fbf = FullBoxFlags::from_flags([(i, true)]);
        let expected = if i < 32 { 1u32 << i } else { 0 };
        assert_eq!(fbf.get(), expected, "不一致 i={i}");
        assert_eq!(fbf.is_set(i), i < 32, "is_set が一致しない i={i}");
        Ok(())
    })?;
    Ok(())
}

// FullBoxFlags::from_flags の重複ビット位置に対する冪等性を検証する
#[test]
fn full_box_flags_from_flags_duplicate_positions_or_folded() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let items = sample_vec(ctx, 0..64, |ctx| {
            (noprop::sample_usize(ctx), noprop::sample_bool(ctx))
        });
        let actual = FullBoxFlags::from_flags(items.clone()).get();

        let expected: u32 = items
            .iter()
            .filter(|(_, b)| *b)
            .filter(|(i, _)| *i < 32)
            .fold(0u32, |acc, (i, _)| acc | (1u32 << *i));

        assert_eq!(actual, expected, "items が一致しない: {items:?}");
        Ok(())
    })?;
    Ok(())
}

// FullBoxHeader の Roundtrip
#[test]
fn full_box_header_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let version = noprop::sample_u8(ctx);
        let flags_value = arb_full_box_flags(ctx);
        let header = FullBoxHeader {
            version,
            flags: FullBoxFlags::new(flags_value),
        };
        let encoded = header
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), 4);

        let (decoded, size) = FullBoxHeader::decode(&encoded)
            .expect("直前にエンコードした 4 バイト表現は必ずデコードできる");
        assert_eq!(size, 4);
        assert_eq!(decoded.version, header.version);
        assert_eq!(decoded.flags.get(), header.flags.get());
        Ok(())
    })?;
    Ok(())
}

// FixedPointNumber<u8, u8> の Roundtrip
#[test]
fn fixed_point_u8_u8_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let integer = noprop::sample_u8(ctx);
        let fraction = noprop::sample_u8(ctx);
        let fpn: FixedPointNumber<u8, u8> = FixedPointNumber::new(integer, fraction);
        let encoded = fpn.encode_to_vec().expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), 2);

        let (decoded, size) = FixedPointNumber::<u8, u8>::decode(&encoded)
            .expect("直前にエンコードした 2 バイト表現は必ずデコードできる");
        assert_eq!(size, 2);
        assert_eq!(decoded.integer, fpn.integer);
        assert_eq!(decoded.fraction, fpn.fraction);
        Ok(())
    })?;
    Ok(())
}

// FixedPointNumber<i16, u16> の Roundtrip
#[test]
fn fixed_point_i16_u16_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let integer = noprop::sample_i16(ctx);
        let fraction = noprop::sample_u16(ctx);
        let fpn: FixedPointNumber<i16, u16> = FixedPointNumber::new(integer, fraction);
        let encoded = fpn.encode_to_vec().expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), 4);

        let (decoded, size) = FixedPointNumber::<i16, u16>::decode(&encoded)
            .expect("直前にエンコードした 4 バイト表現は必ずデコードできる");
        assert_eq!(size, 4);
        assert_eq!(decoded.integer, fpn.integer);
        assert_eq!(decoded.fraction, fpn.fraction);
        Ok(())
    })?;
    Ok(())
}

// BoxType::Normal の external_size
#[test]
fn box_type_normal_external_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_normal(ctx);
        let box_type = BoxType::Normal(ty);
        assert_eq!(box_type.external_size(), 4);
        Ok(())
    })?;
    Ok(())
}

// BoxType::Uuid の external_size
#[test]
fn box_type_uuid_external_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_uuid(ctx);
        let box_type = BoxType::Uuid(ty);
        assert_eq!(box_type.external_size(), 20); // 4 + 16
        Ok(())
    })?;
    Ok(())
}

// BoxType::as_bytes
#[test]
fn box_type_as_bytes_normal() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_normal(ctx);
        let box_type = BoxType::Normal(ty);
        assert_eq!(box_type.as_bytes(), &ty[..]);
        Ok(())
    })?;
    Ok(())
}

// BoxType::as_bytes for Uuid
#[test]
fn box_type_as_bytes_uuid() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_uuid(ctx);
        let box_type = BoxType::Uuid(ty);
        assert_eq!(box_type.as_bytes(), &ty[..]);
        Ok(())
    })?;
    Ok(())
}

// BoxSize::U32 の get と external_size
#[test]
fn box_size_u32_properties() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let size = arb_box_size_u32(ctx);
        let box_size = BoxSize::U32(size);
        assert_eq!(box_size.get(), size as u64);
        assert_eq!(box_size.external_size(), 4);
        Ok(())
    })?;
    Ok(())
}

// BoxSize::U64 の get と external_size
#[test]
fn box_size_u64_properties() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let size = arb_box_size_u64(ctx);
        let box_size = BoxSize::U64(size);
        assert_eq!(box_size.get(), size);
        assert_eq!(box_size.external_size(), 12); // 4 + 8
        Ok(())
    })?;
    Ok(())
}

// BoxSize::with_payload_size が正しいサイズを返す
#[test]
fn box_size_with_payload_size() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let payload = noprop::sample_u64_in(ctx, 0..=(u32::MAX as u64 - 8));
        let box_type = BoxType::Normal(*b"test");
        let box_size = BoxSize::with_payload_size(box_type, payload);

        // サイズフィールド (4) + ボックス種別 (4) + ペイロード
        let expected = 4 + 4 + payload;
        assert_eq!(box_size.get(), expected);
        assert!(matches!(box_size, BoxSize::U32(_)));
        Ok(())
    })?;
    Ok(())
}

// BoxHeader の Roundtrip (Normal タイプ、U32 サイズ)
#[test]
fn box_header_normal_u32_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_normal(ctx);
        let size = noprop::sample_u64_in(ctx, 8..=1_000_000) as u32;
        let header = BoxHeader {
            box_type: BoxType::Normal(ty),
            box_size: BoxSize::U32(size),
        };
        let encoded = header
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded)
            .expect("直前にエンコードした有効なヘッダーは必ずデコードできる");
        assert_eq!(decode_size, header.external_size());
        assert_eq!(decoded.box_type, header.box_type);
        assert_eq!(decoded.box_size, header.box_size);
        Ok(())
    })?;
    Ok(())
}

// BoxHeader の Roundtrip (Uuid タイプ、U32 サイズ)
#[test]
fn box_header_uuid_u32_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_uuid(ctx);
        let size = noprop::sample_u64_in(ctx, 24..=1_000_000) as u32;
        let header = BoxHeader {
            box_type: BoxType::Uuid(ty),
            box_size: BoxSize::U32(size),
        };
        let encoded = header
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded)
            .expect("直前にエンコードした有効なヘッダーは必ずデコードできる");
        assert_eq!(decode_size, header.external_size());
        assert_eq!(decoded.box_type, header.box_type);
        assert_eq!(decoded.box_size, header.box_size);
        Ok(())
    })?;
    Ok(())
}

// BoxHeader の Roundtrip (Normal タイプ、U64 サイズ)
#[test]
fn box_header_normal_u64_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let ty = arb_box_type_normal(ctx);
        let size = noprop::sample_u64_in(
            ctx,
            ((u32::MAX as u64) + 1)..=((u32::MAX as u64) + 1_000_000),
        );
        let header = BoxHeader {
            box_type: BoxType::Normal(ty),
            box_size: BoxSize::U64(size),
        };
        let encoded = header
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");

        assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded)
            .expect("直前にエンコードした有効なヘッダーは必ずデコードできる");
        assert_eq!(decode_size, header.external_size());
        assert_eq!(decoded.box_type, header.box_type);
        assert_eq!(decoded.box_size, header.box_size);
        Ok(())
    })?;
    Ok(())
}

// Utf8String の Roundtrip
#[test]
fn utf8_string_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let s = arb_utf8_string(ctx);
        let utf8_str = Utf8String::new(&s).expect("サンプラーで null 文字を除外している");
        let encoded = utf8_str
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");

        // null 終端を含む
        assert_eq!(encoded.len(), s.len() + 1);
        assert_eq!(encoded.last(), Some(&0u8));

        let (decoded, size) = Utf8String::decode(&encoded)
            .expect("直前にエンコードした null 終端 UTF-8 は必ずデコードできる");
        assert_eq!(size, s.len() + 1);
        assert_eq!(decoded.get(), utf8_str.get());
        Ok(())
    })?;
    Ok(())
}

// Utf8String::new は null を含む文字列を拒否
#[test]
fn utf8_string_rejects_null() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let prefix = arb_utf8_string_short(ctx);
        let suffix = arb_utf8_string_short(ctx);
        let s = format!("{prefix}\x00{suffix}");
        assert!(Utf8String::new(&s).is_none());
        Ok(())
    })?;
    Ok(())
}

/// 短い null なし文字列（0-10 文字）を生成する（`utf8_string_rejects_null` 専用）
fn arb_utf8_string_short(ctx: &mut TestCaseContext) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=10);
    let mut s = String::new();
    while s.chars().count() < len {
        let c = noprop::sample_char(ctx);
        if c != '\0' {
            s.push(c);
        }
    }
    s
}

// Mp4FileTime の from_secs と as_secs
#[test]
fn mp4_file_time_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let secs = noprop::sample_u64(ctx);
        let time = Mp4FileTime::from_secs(secs);
        assert_eq!(time.as_secs(), secs);
        Ok(())
    })?;
    Ok(())
}

// Uint<u8, 4, 0> のビット操作
#[test]
fn uint_u8_4_0_from_bits() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = noprop::sample_u8(ctx);
        let uint: Uint<u8, 4, 0> = Uint::from_bits(value);
        // 下位 4 ビットを抽出
        assert_eq!(uint.get(), value & 0x0F);
        Ok(())
    })?;
    Ok(())
}

// Uint<u8, 4, 4> のビット操作
#[test]
fn uint_u8_4_4_from_bits() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = noprop::sample_u8(ctx);
        let uint: Uint<u8, 4, 4> = Uint::from_bits(value);
        // 上位 4 ビットを抽出
        assert_eq!(uint.get(), (value >> 4) & 0x0F);
        Ok(())
    })?;
    Ok(())
}

// Uint<u16, 12, 0> のビット操作
#[test]
fn uint_u16_12_0_from_bits() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = noprop::sample_u16(ctx);
        let uint: Uint<u16, 12, 0> = Uint::from_bits(value);
        assert_eq!(uint.get(), value & 0x0FFF);
        Ok(())
    })?;
    Ok(())
}

// Uint の to_bits と from_bits の対称性
#[test]
fn uint_to_bits_from_bits_symmetry() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = noprop::sample_u64_in(ctx, 0..=0x0F) as u8;
        let uint: Uint<u8, 4, 4> = Uint::new(value);
        let bits = uint.to_bits();
        let recovered: Uint<u8, 4, 4> = Uint::from_bits(bits);
        assert_eq!(recovered.get(), value);
        Ok(())
    })?;
    Ok(())
}

// Uint<T, 1, OFFSET> の as_bool
#[test]
fn uint_1_as_bool() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let value = noprop::sample_bool(ctx);
        let uint: Uint<u8, 1, 0> = Uint::from(value);
        assert_eq!(uint.as_bool(), value);
        Ok(())
    })?;
    Ok(())
}

/// エラーケースのテスト用モジュール
mod error_cases {
    use super::*;
    use shiguredo_mp4::ErrorKind;

    /// このモジュールの PBT ケース数（旧 `with_cases(1000)` を維持）
    const CASES: usize = 1000;

    // 不十分なバッファでのエンコード: FullBoxFlags
    #[test]
    fn full_box_flags_encode_insufficient_buffer() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let value = arb_full_box_flags(ctx);
            let buf_size = noprop::sample_usize_in(ctx, 0..3);
            let flags = FullBoxFlags::new(value);
            let mut buf = vec![0u8; buf_size];
            let result = flags.encode(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 不十分なバッファでのエンコード: FullBoxHeader
    #[test]
    fn full_box_header_encode_insufficient_buffer() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let version = noprop::sample_u8(ctx);
            let flags_value = arb_full_box_flags(ctx);
            let buf_size = noprop::sample_usize_in(ctx, 0..4);
            let header = FullBoxHeader {
                version,
                flags: FullBoxFlags::new(flags_value),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 不十分なバッファでのエンコード: BoxHeader (Normal, U32)
    #[test]
    fn box_header_encode_insufficient_buffer() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let ty = arb_box_type_normal(ctx);
            let size = noprop::sample_u64_in(ctx, 8..=1_000_000) as u32;
            let buf_size = noprop::sample_usize_in(ctx, 0..8);
            let header = BoxHeader {
                box_type: BoxType::Normal(ty),
                box_size: BoxSize::U32(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 不十分なバッファでのエンコード: BoxHeader (Uuid, U32)
    #[test]
    fn box_header_uuid_encode_insufficient_buffer() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let ty = arb_box_type_uuid(ctx);
            let size = noprop::sample_u64_in(ctx, 24..=1_000_000) as u32;
            let buf_size = noprop::sample_usize_in(ctx, 0..24);
            let header = BoxHeader {
                box_type: BoxType::Uuid(ty),
                box_size: BoxSize::U32(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 不十分なバッファでのエンコード: BoxHeader (Normal, U64)
    #[test]
    fn box_header_u64_encode_insufficient_buffer() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let ty = arb_box_type_normal(ctx);
            let size = noprop::sample_u64_in(
                ctx,
                ((u32::MAX as u64) + 1)..=((u32::MAX as u64) + 1_000_000),
            );
            let buf_size = noprop::sample_usize_in(ctx, 0..16);
            let header = BoxHeader {
                box_type: BoxType::Normal(ty),
                box_size: BoxSize::U64(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 切り詰められた入力でのデコード: FullBoxFlags
    #[test]
    fn full_box_flags_decode_truncated() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let buf_size = noprop::sample_usize_in(ctx, 0..3);
            let buf = vec![0xFFu8; buf_size];
            let result = FullBoxFlags::decode(&buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 切り詰められた入力でのデコード: FullBoxHeader
    #[test]
    fn full_box_header_decode_truncated() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let buf_size = noprop::sample_usize_in(ctx, 0..4);
            let buf = vec![0xFFu8; buf_size];
            let result = FullBoxHeader::decode(&buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 切り詰められた入力でのデコード: BoxHeader
    #[test]
    fn box_header_decode_truncated() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let buf_size = noprop::sample_usize_in(ctx, 0..8);
            let buf = vec![0xFFu8; buf_size];
            let result = BoxHeader::decode(&buf);
            assert!(result.is_err());
            Ok(())
        })?;
        Ok(())
    }

    // 切り詰められた入力でのデコード: FixedPointNumber<u8, u8>
    #[test]
    fn fixed_point_u8_u8_decode_truncated() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let buf_size = noprop::sample_usize_in(ctx, 0..2);
            let buf = vec![0xFFu8; buf_size];
            let result = FixedPointNumber::<u8, u8>::decode(&buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // 切り詰められた入力でのデコード: FixedPointNumber<i16, u16>
    #[test]
    fn fixed_point_i16_u16_decode_truncated() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let buf_size = noprop::sample_usize_in(ctx, 0..4);
            let buf = vec![0xFFu8; buf_size];
            let result = FixedPointNumber::<i16, u16>::decode(&buf);
            assert!(result.is_err());
            assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
            Ok(())
        })?;
        Ok(())
    }

    // null 終端がない Utf8String のデコード
    #[test]
    fn utf8_string_decode_no_null_terminator() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 1..100);
            let data: Vec<u8> = (0..n)
                .map(|_| noprop::sample_u64_in(ctx, 1..=255) as u8)
                .collect();
            // null を含まないバイト列
            let result = Utf8String::decode(&data);
            assert!(result.is_err());
            Ok(())
        })?;
        Ok(())
    }

    // 不正なボックスサイズ (ヘッダーサイズより小さい)
    #[test]
    fn box_header_decode_invalid_size() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let size = noprop::sample_u64_in(ctx, 1..8) as u32;
            // サイズフィールドが 1-7 の場合、ヘッダーサイズ (8) より小さいのでエラー
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&size.to_be_bytes());
            buf[4..8].copy_from_slice(b"test");

            let result = BoxHeader::decode(&buf);
            assert!(result.is_err());
            Ok(())
        })?;
        Ok(())
    }

    // 任意のバイト列でのデコード (クラッシュしないことを確認)
    #[test]
    fn box_header_decode_arbitrary_bytes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 0..1024);
            let data = noprop::sample_bytes_vec(ctx, n);
            // クラッシュしなければ OK (エラーは許容)
            let _ = BoxHeader::decode(&data);
            Ok(())
        })?;
        Ok(())
    }

    // 任意のバイト列でのデコード: FullBoxFlags
    #[test]
    fn full_box_flags_decode_arbitrary_bytes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 0..100);
            let data = noprop::sample_bytes_vec(ctx, n);
            let _ = FullBoxFlags::decode(&data);
            Ok(())
        })?;
        Ok(())
    }

    // 任意のバイト列でのデコード: FullBoxHeader
    #[test]
    fn full_box_header_decode_arbitrary_bytes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 0..100);
            let data = noprop::sample_bytes_vec(ctx, n);
            let _ = FullBoxHeader::decode(&data);
            Ok(())
        })?;
        Ok(())
    }

    // 任意のバイト列でのデコード: Utf8String
    #[test]
    fn utf8_string_decode_arbitrary_bytes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 0..256);
            let data = noprop::sample_bytes_vec(ctx, n);
            let _ = Utf8String::decode(&data);
            Ok(())
        })?;
        Ok(())
    }

    // 任意のバイト列でのデコード: FixedPointNumber
    #[test]
    fn fixed_point_decode_arbitrary_bytes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let n = noprop::sample_usize_in(ctx, 0..100);
            let data = noprop::sample_bytes_vec(ctx, n);
            let _ = FixedPointNumber::<u8, u8>::decode(&data);
            let _ = FixedPointNumber::<i16, u16>::decode(&data);
            let _ = FixedPointNumber::<i32, u32>::decode(&data);
            Ok(())
        })?;
        Ok(())
    }
}

/// 境界値テスト
mod boundary_tests {
    use super::*;
    use shiguredo_mp4::ErrorKind;

    /// このモジュールの PBT ケース数（旧 `with_cases(100)` を維持）
    const CASES: usize = 100;

    // BoxSize::with_payload_size が U64 になる境界
    #[test]
    fn box_size_u64_boundary() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let payload =
                noprop::sample_u64_in(ctx, (u32::MAX as u64 - 7)..=(u32::MAX as u64 + 100));
            let box_type = BoxType::Normal(*b"test");
            let box_size = BoxSize::with_payload_size(box_type, payload);

            // 4 + 4 + payload > u32::MAX の場合は U64
            let total = 8u64.saturating_add(payload);
            if total > u32::MAX as u64 {
                assert!(matches!(box_size, BoxSize::U64(_)));
            } else {
                assert!(matches!(box_size, BoxSize::U32(_)));
            }
            Ok(())
        })?;
        Ok(())
    }

    // 大きなペイロードサイズ
    #[test]
    fn box_size_large_payload() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let payload = noprop::sample_u64_in(ctx, (u32::MAX as u64)..=u64::MAX);
            let box_type = BoxType::Normal(*b"test");
            let box_size = BoxSize::with_payload_size(box_type, payload);

            // 常に U64 になるはず
            assert!(matches!(box_size, BoxSize::U64(_)));
            Ok(())
        })?;
        Ok(())
    }

    #[test]
    fn full_box_flags_zero() {
        let flags = FullBoxFlags::empty();
        assert_eq!(flags.get(), 0);

        for i in 0..24 {
            assert!(!flags.is_set(i));
        }
    }

    #[test]
    fn full_box_flags_max() {
        let flags = FullBoxFlags::new(0x00FF_FFFF);
        assert_eq!(flags.get(), 0x00FF_FFFF);

        for i in 0..24 {
            assert!(flags.is_set(i));
        }
    }

    #[test]
    fn full_box_flags_overflow_ignored() {
        // 24 ビットを超える値は切り捨てられる
        let flags = FullBoxFlags::new(0xFFFF_FFFF);
        // エンコード後は 24 ビットに収まる
        let encoded = flags.encode_to_vec().expect("Vec への書き込みは失敗しない");
        assert_eq!(encoded.len(), 3);

        let (decoded, _) = FullBoxFlags::decode(&encoded)
            .expect("直前にエンコードした 3 バイト表現は必ずデコードできる");
        assert_eq!(decoded.get(), 0x00FF_FFFF);
    }

    #[test]
    fn box_size_variable() {
        assert_eq!(BoxSize::VARIABLE_SIZE.get(), 0);
        assert_eq!(BoxSize::LARGE_VARIABLE_SIZE.get(), 0);
    }

    #[test]
    fn box_size_variable_external_sizes() {
        assert_eq!(BoxSize::VARIABLE_SIZE.external_size(), 4);
        assert_eq!(BoxSize::LARGE_VARIABLE_SIZE.external_size(), 12);
    }

    #[test]
    fn utf8_string_empty() {
        let s = Utf8String::new("").expect("空文字列は null を含まないので有効");
        let encoded = s.encode_to_vec().expect("Vec への書き込みは失敗しない");
        assert_eq!(encoded, vec![0]);

        let (decoded, size) = Utf8String::decode(&encoded)
            .expect("直前にエンコードした null 終端 UTF-8 は必ずデコードできる");
        assert_eq!(size, 1);
        assert_eq!(decoded.get(), "");
    }

    #[test]
    fn utf8_string_only_null() {
        // null のみのバイト列
        let buf = [0u8];
        let (decoded, size) =
            Utf8String::decode(&buf).expect("null のみは空文字列として有効にデコードできる");
        assert_eq!(size, 1);
        assert_eq!(decoded.get(), "");
    }

    #[test]
    fn utf8_string_invalid_utf8() {
        // 不正な UTF-8 シーケンス (null 終端あり)
        let buf = [0xFF, 0xFE, 0x00];
        let result = Utf8String::decode(&buf);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, ErrorKind::InvalidInput);
    }

    #[test]
    fn mp4_file_time_unix_epoch() {
        let time = Mp4FileTime::from_unix_time(core::time::Duration::from_secs(0));
        // 1904/1/1 から 1970/1/1 までの秒数
        assert_eq!(time.as_secs(), 2082844800);
    }

    #[test]
    fn mp4_file_time_max() {
        let time = Mp4FileTime::from_secs(u64::MAX);
        assert_eq!(time.as_secs(), u64::MAX);
    }

    #[test]
    fn box_header_min_size() {
        assert_eq!(BoxHeader::MIN_SIZE, 8);
    }

    #[test]
    fn box_header_max_size() {
        // 4 (size) + 8 (extended size) + 4 (type) + 16 (uuid)
        assert_eq!(BoxHeader::MAX_SIZE, 32);
    }

    #[test]
    fn box_header_size_zero_means_variable() {
        // サイズ 0 は可変長ボックスを意味する
        let header = BoxHeader {
            box_type: BoxType::Normal(*b"mdat"),
            box_size: BoxSize::VARIABLE_SIZE,
        };
        assert_eq!(header.box_size.get(), 0);
    }

    #[test]
    fn box_header_decode_extended_size() {
        // サイズフィールドが 1 の場合、拡張サイズを使用
        let mut buf = vec![0u8; 16];
        buf[0..4].copy_from_slice(&1u32.to_be_bytes()); // size = 1 (extended)
        buf[4..8].copy_from_slice(b"test");
        buf[8..16].copy_from_slice(&0x100000001u64.to_be_bytes()); // 4GB + 1

        let (header, size) =
            BoxHeader::decode(&buf).expect("組み立てた 16 バイトの拡張サイズヘッダーは有効");
        assert_eq!(size, 16);
        assert!(matches!(header.box_size, BoxSize::U64(0x100000001)));
    }
}

/// コーデックボックスの境界値テスト (feature/fix-infinite-loop で修正された問題)
mod codec_box_boundary_tests {
    use shiguredo_mp4::{Decode, boxes::HvccBox, boxes::VpccBox};

    /// HvccBox: NAL unit length がペイロード境界を超える場合のテスト
    ///
    /// 修正前: panic (slice index out of bounds)
    /// 修正後: Error を返す
    #[test]
    fn hvcc_box_nal_unit_length_exceeds_payload() {
        // 最小限の有効な HvccBox ヘッダー + 不正な NAL unit length
        let mut buf = Vec::new();

        // BoxHeader: size=0 (可変長), type="hvcC"
        buf.extend_from_slice(&0u32.to_be_bytes()); // size = 0 (variable)
        buf.extend_from_slice(b"hvcC");

        // configuration_version = 1
        buf.push(1);
        // general_profile_space(2) | general_tier_flag(1) | general_profile_idc(5) = 0
        buf.push(0);
        // general_profile_compatibility_flags (4 bytes)
        buf.extend_from_slice(&[0u8; 4]);
        // general_constraint_indicator_flags (6 bytes)
        buf.extend_from_slice(&[0u8; 6]);
        // general_level_idc
        buf.push(0);
        // reserved(4) | min_spatial_segmentation_idc(12) (2 bytes)
        buf.extend_from_slice(&[0xF0, 0x00]);
        // reserved(6) | parallelism_type(2)
        buf.push(0xFC);
        // reserved(6) | chroma_format_idc(2)
        buf.push(0xFC);
        // reserved(5) | bit_depth_luma_minus8(3)
        buf.push(0xF8);
        // reserved(5) | bit_depth_chroma_minus8(3)
        buf.push(0xF8);
        // avg_frame_rate (2 bytes)
        buf.extend_from_slice(&[0, 0]);
        // constant_frame_rate(2) | num_temporal_layers(3) | temporal_id_nested(1) | length_size_minus_one(2)
        buf.push(0);
        // num_of_arrays = 1 (1つの NALU 配列)
        buf.push(1);

        // NALU array
        // array_completeness(1) | reserved(1) | nal_unit_type(6)
        buf.push(0);
        // num_nalus = 1
        buf.extend_from_slice(&1u16.to_be_bytes());
        // nal_unit_length = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の NAL unit データは 0 バイト (境界を超えている)

        let result = HvccBox::decode(&buf);
        // 修正後はエラーを返すはず (panic しない)
        assert!(
            result.is_err(),
            "HvccBox は NAL unit 長が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }

    /// HvccBox: NAL unit length がペイロード境界を超える場合
    ///
    /// ボックスサイズを固定して、ペイロードが正確に計算されるケース
    /// NAL unit length が不正な場合は Error を返す
    #[test]
    fn hvcc_box_nal_unit_length_exceeds_payload_with_fixed_size() {
        // ボックスサイズを固定して、ペイロードが正確に計算されるようにする
        let mut buf = Vec::new();

        // configuration_version = 1
        buf.push(1);
        // general_profile_space(2) | general_tier_flag(1) | general_profile_idc(5) = 0
        buf.push(0);
        // general_profile_compatibility_flags (4 bytes)
        buf.extend_from_slice(&[0u8; 4]);
        // general_constraint_indicator_flags (6 bytes)
        buf.extend_from_slice(&[0u8; 6]);
        // general_level_idc
        buf.push(0);
        // reserved(4) | min_spatial_segmentation_idc(12) (2 bytes)
        buf.extend_from_slice(&[0xF0, 0x00]);
        // reserved(6) | parallelism_type(2)
        buf.push(0xFC);
        // reserved(6) | chroma_format_idc(2)
        buf.push(0xFC);
        // reserved(5) | bit_depth_luma_minus8(3)
        buf.push(0xF8);
        // reserved(5) | bit_depth_chroma_minus8(3)
        buf.push(0xF8);
        // avg_frame_rate (2 bytes)
        buf.extend_from_slice(&[0, 0]);
        // constant_frame_rate(2) | num_temporal_layers(3) | temporal_id_nested(1) | length_size_minus_one(2)
        buf.push(0);
        // num_of_arrays = 1 (1つの NALU 配列)
        buf.push(1);

        // NALU array
        // array_completeness(1) | reserved(1) | nal_unit_type(6)
        buf.push(0);
        // num_nalus = 1
        buf.extend_from_slice(&1u16.to_be_bytes());
        // nal_unit_length = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の NAL unit データは 0 バイト (境界を超えている)

        // ペイロードサイズを計算
        let payload_size = buf.len();

        // BoxHeader を先頭に付加
        let mut full_buf = Vec::new();
        let box_size = (8 + payload_size) as u32; // 8 = BoxHeader サイズ
        full_buf.extend_from_slice(&box_size.to_be_bytes());
        full_buf.extend_from_slice(b"hvcC");
        full_buf.extend_from_slice(&buf);

        let result = HvccBox::decode(&full_buf);
        // 修正前は panic、修正後はエラーを返すはず
        assert!(
            result.is_err(),
            "HvccBox は NAL unit 長が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }

    /// VpccBox: codec_init_size がペイロード境界を超える場合のテスト
    ///
    /// 修正前: panic (slice index out of bounds)
    /// 修正後: Error を返す
    #[test]
    fn vpcc_box_codec_init_size_exceeds_payload() {
        // 最小限の有効な VpccBox ヘッダー + 不正な codec_init_size
        let mut buf = Vec::new();

        // BoxHeader: size=0 (可変長), type="vpcC"
        buf.extend_from_slice(&0u32.to_be_bytes()); // size = 0 (variable)
        buf.extend_from_slice(b"vpcC");

        // FullBoxHeader: version=1, flags=0
        buf.push(1); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags

        // profile
        buf.push(0);
        // level
        buf.push(0);
        // bit_depth(4) | chroma_subsampling(3) | video_full_range_flag(1)
        buf.push(0);
        // colour_primaries
        buf.push(0);
        // transfer_characteristics
        buf.push(0);
        // matrix_coefficients
        buf.push(0);
        // codec_init_size = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の codec_initialization_data は 0 バイト (境界を超えている)

        let result = VpccBox::decode(&buf);
        // 修正後はエラーを返すはず (panic しない)
        assert!(
            result.is_err(),
            "VpccBox は codec_init_size が payload を超える場合にエラーを返す"
        );
    }

    /// VpccBox: codec_init_size がペイロード境界を超える場合
    ///
    /// ボックスサイズを固定して、ペイロードが正確に計算されるケース
    /// codec_init_size が不正な場合は Error を返す
    #[test]
    fn vpcc_box_codec_init_size_exceeds_payload_with_fixed_size() {
        // ボックスサイズを固定して、ペイロードが正確に計算されるようにする
        let mut buf = Vec::new();

        // FullBoxHeader: version=1, flags=0
        buf.push(1); // version
        buf.extend_from_slice(&[0, 0, 0]); // flags

        // profile
        buf.push(0);
        // level
        buf.push(0);
        // bit_depth(4) | chroma_subsampling(3) | video_full_range_flag(1)
        buf.push(0);
        // colour_primaries
        buf.push(0);
        // transfer_characteristics
        buf.push(0);
        // matrix_coefficients
        buf.push(0);
        // codec_init_size = 0xFFFF (ペイロードを大幅に超える値)
        buf.extend_from_slice(&0xFFFFu16.to_be_bytes());
        // 実際の codec_initialization_data は 0 バイト (境界を超えている)

        // ペイロードサイズを計算
        let payload_size = buf.len();

        // BoxHeader を先頭に付加
        let mut full_buf = Vec::new();
        let box_size = (8 + payload_size) as u32; // 8 = BoxHeader サイズ
        full_buf.extend_from_slice(&box_size.to_be_bytes());
        full_buf.extend_from_slice(b"vpcC");
        full_buf.extend_from_slice(&buf);

        let result = VpccBox::decode(&full_buf);
        // 修正前は panic、修正後はエラーを返すはず
        assert!(
            result.is_err(),
            "VpccBox は codec_init_size が payload を超える場合にエラーを返す: 実際は {result:?}"
        );
    }
}

/// さらに変な値を使ったテスト
mod weird_values {
    use super::*;

    /// このモジュールの PBT ケース数（旧 `with_cases(500)` を維持）
    const CASES: usize = 500;

    /// 極端に大きいサイズフィールドを持つボックスヘッダー
    #[test]
    fn box_header_with_extreme_sizes() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let size_type = noprop::sample_choice(
                ctx,
                &[
                    0xFFFFFFFFu32, // 最大 u32
                    0x80000000u32, // 符号付きで負になる値
                    0x7FFFFFFFu32, // 符号付き最大
                    1u32,          // 拡張サイズマーカー
                    2u32,          // 無効 (ヘッダーより小さい)
                    7u32,          // 境界 (ヘッダーより小さい)
                ],
            );
            let box_type = noprop::sample_bytes::<4>(ctx);
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&size_type.to_be_bytes());
            buf[4..8].copy_from_slice(&box_type);

            // クラッシュしなければ OK
            let _ = BoxHeader::decode(&buf);
            Ok(())
        })?;
        Ok(())
    }

    /// 極端なバイトパターン
    #[test]
    fn decode_extreme_byte_patterns() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        let patterns: Vec<Vec<u8>> = vec![
            vec![0xFFu8; 64],                         // オール 0xFF
            vec![0x00u8; 64],                         // オール 0x00
            vec![0x80u8; 64],                         // オール 0x80 (符号ビット)
            vec![0x7Fu8; 64],                         // オール 0x7F
            (0..64).map(|i| i as u8).collect(),       // 連番
            (0..64).map(|i| (i * 2) as u8).collect(), // 偶数
            (0..64)
                .map(|i| if i % 2 == 0 { 0xFF } else { 0x00 })
                .collect(), // 交互
            vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE], // マジックバイト
        ];
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let idx = noprop::sample_usize_in(ctx, 0..patterns.len());
            let pattern = &patterns[idx];
            // クラッシュしなければ OK
            let _ = BoxHeader::decode(pattern);
            let _ = FullBoxFlags::decode(pattern);
            let _ = FullBoxHeader::decode(pattern);
            let _ = Utf8String::decode(pattern);
            let _ = FixedPointNumber::<u8, u8>::decode(pattern);
            Ok(())
        })?;
        Ok(())
    }

    /// 拡張サイズ境界テスト (size=1 で不正な拡張サイズ)
    #[test]
    fn box_header_extended_size_edge_cases() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let extended_size = noprop::sample_choice(
                ctx,
                &[
                    0u64,           // 0 (無効)
                    1u64,           // 1 (無効)
                    15u64,          // ヘッダーサイズ未満
                    16u64,          // ちょうどヘッダーサイズ
                    0xFFFFFFFFu64,  // u32 最大値
                    0x100000000u64, // u32 + 1
                    u64::MAX,       // 最大値
                    u64::MAX - 1,   // 最大値 - 1
                ],
            );
            let mut buf = vec![0u8; 16];
            buf[0..4].copy_from_slice(&1u32.to_be_bytes()); // size = 1 (extended)
            buf[4..8].copy_from_slice(b"test");
            buf[8..16].copy_from_slice(&extended_size.to_be_bytes());

            // クラッシュしなければ OK
            let _ = BoxHeader::decode(&buf);
            Ok(())
        })?;
        Ok(())
    }

    /// UUID ボックスの変なパターン
    #[test]
    fn box_header_uuid_weird_patterns() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            // 旧 `prop_oneof![Just(...), Just(...), Just(...), any::<[u8; 16]>()]` を等確率選択
            let uuid = noprop::sample_with_boundaries(
                ctx,
                &[[0xFFu8; 16], [0x00u8; 16], [0x80u8; 16]],
                noprop::Ratio::new(3, 4),
                |ctx| noprop::sample_bytes::<16>(ctx),
            );
            let size = noprop::sample_u64_in(ctx, 24..=0xFFFF) as u32;
            let mut buf = vec![0u8; 24];
            buf[0..4].copy_from_slice(&size.to_be_bytes());
            buf[4..8].copy_from_slice(b"uuid");
            buf[8..24].copy_from_slice(&uuid);

            let result = BoxHeader::decode(&buf);
            if let Ok((header, _)) = result {
                // UUID として正しくデコードされたか確認
                assert!(matches!(header.box_type, BoxType::Uuid(_)));
            }
            Ok(())
        })?;
        Ok(())
    }

    /// FullBoxFlags の境界ビット操作
    #[test]
    fn full_box_flags_boundary_bits() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let value = noprop::sample_choice(
                ctx,
                &[
                    0u32,
                    1u32,
                    0x800000u32,   // ビット 23
                    0x400000u32,   // ビット 22
                    0x000001u32,   // ビット 0
                    0xAAAAAAu32,   // 交互パターン
                    0x555555u32,   // 逆交互パターン
                    0xFFFFFFu32,   // 24 ビット全部
                    0xFFFFFFFFu32, // 32 ビット全部 (上位 8 ビットは切り捨て)
                ],
            );
            let flags = FullBoxFlags::new(value);
            let encoded = flags.encode_to_vec().expect("Vec への書き込みは失敗しない");
            let (decoded, _) = FullBoxFlags::decode(&encoded)
                .expect("直前にエンコードした 3 バイト表現は必ずデコードできる");

            // 上位 8 ビットは切り捨てられる
            assert_eq!(decoded.get(), value & 0x00FFFFFF);
            Ok(())
        })?;
        Ok(())
    }

    /// Mp4FileTime の極端な値
    #[test]
    fn mp4_file_time_extreme_values() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let secs = noprop::sample_choice(
                ctx,
                &[
                    0u64,
                    1u64,
                    u64::MAX,
                    u64::MAX - 1,
                    2082844800u64,  // Unix エポック
                    0x80000000u64,  // 符号付き境界
                    0xFFFFFFFFu64,  // u32 最大
                    0x100000000u64, // u32 + 1
                ],
            );
            let time = Mp4FileTime::from_secs(secs);
            assert_eq!(time.as_secs(), secs);
            Ok(())
        })?;
        Ok(())
    }

    /// Utf8String の変な文字
    #[test]
    fn utf8_string_weird_chars() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        let strings: Vec<String> = vec![
            String::new(),              // 空
            "a".repeat(1000),           // 長い
            "\u{FEFF}BOM".to_string(),  // BOM 付き
            "\u{200B}".to_string(),     // ゼロ幅スペース
            "\u{FFFD}".to_string(),     // 置換文字
            "日本語テスト".to_string(), // 日本語
            "🎉".to_string(),           // 絵文字 (4バイト UTF-8)
            "\t\r\n".to_string(),       // 制御文字
            "a\tb\rc\nd".to_string(),   // 混合
        ];
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let idx = noprop::sample_usize_in(ctx, 0..strings.len());
            let s = &strings[idx];
            if let Some(utf8_str) = Utf8String::new(s) {
                let encoded = utf8_str
                    .encode_to_vec()
                    .expect("Vec への書き込みは失敗しない");
                let (decoded, _) = Utf8String::decode(&encoded)
                    .expect("直前にエンコードした null 終端 UTF-8 は必ずデコードできる");
                assert_eq!(decoded.get(), utf8_str.get());
            }
            Ok(())
        })?;
        Ok(())
    }

    /// 不正な UTF-8 シーケンスのデコード
    #[test]
    fn utf8_string_invalid_sequences() -> noprop::TestResult {
        let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
        let bad_sequences: Vec<Vec<u8>> = vec![
            vec![0x80, 0x00],                   // 継続バイトから開始
            vec![0xC0, 0x80, 0x00],             // オーバーロングエンコード
            vec![0xE0, 0x80, 0x80, 0x00],       // オーバーロングエンコード
            vec![0xF0, 0x80, 0x80, 0x80, 0x00], // オーバーロングエンコード
            vec![0xFE, 0x00],                   // 無効な先頭バイト
            vec![0xFF, 0x00],                   // 無効な先頭バイト
            vec![0xC2, 0x00],                   // 不完全なシーケンス (継続バイトがない)
            vec![0xE0, 0xA0, 0x00],             // 不完全なシーケンス
            vec![0xED, 0xA0, 0x80, 0x00],       // サロゲートペア (UTF-8 では無効)
        ];
        noprop::Runner::new(seed).run(CASES, |ctx| {
            let idx = noprop::sample_usize_in(ctx, 0..bad_sequences.len());
            let data = &bad_sequences[idx];
            let result = Utf8String::decode(data);
            // 不正な UTF-8 はエラーになるはず
            assert!(result.is_err());
            Ok(())
        })?;
        Ok(())
    }
}
