//! 基本型の Property-Based Testing

use proptest::prelude::*;
use shiguredo_mp4::{
    BoxHeader, BoxSize, BoxType, Decode, Encode, FixedPointNumber, FullBoxFlags, FullBoxHeader,
    Mp4FileTime, Uint, Utf8String,
};

/// FullBoxFlags の値を生成する Strategy (24 ビット)
fn arb_full_box_flags() -> impl Strategy<Value = u32> {
    0u32..=0x00FF_FFFF
}

/// BoxType::Normal 用の 4 バイト値を生成する Strategy
fn arb_box_type_normal() -> impl Strategy<Value = [u8; 4]> {
    any::<[u8; 4]>()
}

/// BoxType::Uuid 用の 16 バイト値を生成する Strategy
fn arb_box_type_uuid() -> impl Strategy<Value = [u8; 16]> {
    any::<[u8; 16]>()
}

/// BoxSize::U32 用の値を生成する Strategy (ヘッダーサイズ 8 以上)
fn arb_box_size_u32() -> impl Strategy<Value = u32> {
    prop_oneof![
        Just(0u32), // VARIABLE_SIZE
        8u32..=u32::MAX,
    ]
}

/// BoxSize::U64 用の値を生成する Strategy (4GB 超、またはゼロ)
fn arb_box_size_u64() -> impl Strategy<Value = u64> {
    prop_oneof![
        Just(0u64), // LARGE_VARIABLE_SIZE
        ((u32::MAX as u64) + 1)..=u64::MAX,
    ]
}

/// null を含まない UTF-8 文字列を生成する Strategy
fn arb_utf8_string() -> impl Strategy<Value = String> {
    "[^\x00]{0,100}"
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // FullBoxFlags の Roundtrip
    #[test]
    fn full_box_flags_roundtrip(value in arb_full_box_flags()) {
        let flags = FullBoxFlags::new(value);
        let encoded = flags.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), 3);

        let (decoded, size) = FullBoxFlags::decode(&encoded).unwrap();
        prop_assert_eq!(size, 3);
        prop_assert_eq!(decoded.get(), flags.get());
    }

    // FullBoxFlags のビット操作
    #[test]
    fn full_box_flags_bit_operations(value in arb_full_box_flags()) {
        let flags = FullBoxFlags::new(value);

        for i in 0..24 {
            let expected = (value & (1 << i)) != 0;
            prop_assert_eq!(flags.is_set(i), expected, "bit {} mismatch", i);
        }
    }

    // FullBoxFlags::from_flags の検証 (各ビット位置は一度だけ)
    #[test]
    fn full_box_flags_from_flags(bit_mask in any::<u32>()) {
        // 24 ビットのマスクから (bit_position, is_set) のリストを生成
        let bits: Vec<(usize, bool)> = (0..24).map(|i| (i, (bit_mask & (1 << i)) != 0)).collect();
        let flags = FullBoxFlags::from_flags(bits);

        for i in 0..24 {
            let expected = (bit_mask & (1 << i)) != 0;
            prop_assert_eq!(flags.is_set(i), expected, "bit {} mismatch", i);
        }
    }

    // FullBoxHeader の Roundtrip
    #[test]
    fn full_box_header_roundtrip(version in any::<u8>(), flags_value in arb_full_box_flags()) {
        let header = FullBoxHeader {
            version,
            flags: FullBoxFlags::new(flags_value),
        };
        let encoded = header.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), 4);

        let (decoded, size) = FullBoxHeader::decode(&encoded).unwrap();
        prop_assert_eq!(size, 4);
        prop_assert_eq!(decoded.version, header.version);
        prop_assert_eq!(decoded.flags.get(), header.flags.get());
    }

    // FixedPointNumber<u8, u8> の Roundtrip
    #[test]
    fn fixed_point_u8_u8_roundtrip(integer in any::<u8>(), fraction in any::<u8>()) {
        let fpn: FixedPointNumber<u8, u8> = FixedPointNumber::new(integer, fraction);
        let encoded = fpn.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), 2);

        let (decoded, size) = FixedPointNumber::<u8, u8>::decode(&encoded).unwrap();
        prop_assert_eq!(size, 2);
        prop_assert_eq!(decoded.integer, fpn.integer);
        prop_assert_eq!(decoded.fraction, fpn.fraction);
    }

    // FixedPointNumber<i16, u16> の Roundtrip
    #[test]
    fn fixed_point_i16_u16_roundtrip(integer in any::<i16>(), fraction in any::<u16>()) {
        let fpn: FixedPointNumber<i16, u16> = FixedPointNumber::new(integer, fraction);
        let encoded = fpn.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), 4);

        let (decoded, size) = FixedPointNumber::<i16, u16>::decode(&encoded).unwrap();
        prop_assert_eq!(size, 4);
        prop_assert_eq!(decoded.integer, fpn.integer);
        prop_assert_eq!(decoded.fraction, fpn.fraction);
    }

    // BoxType::Normal の external_size
    #[test]
    fn box_type_normal_external_size(ty in arb_box_type_normal()) {
        let box_type = BoxType::Normal(ty);
        prop_assert_eq!(box_type.external_size(), 4);
    }

    // BoxType::Uuid の external_size
    #[test]
    fn box_type_uuid_external_size(ty in arb_box_type_uuid()) {
        let box_type = BoxType::Uuid(ty);
        prop_assert_eq!(box_type.external_size(), 20); // 4 + 16
    }

    // BoxType::as_bytes
    #[test]
    fn box_type_as_bytes_normal(ty in arb_box_type_normal()) {
        let box_type = BoxType::Normal(ty);
        prop_assert_eq!(box_type.as_bytes(), &ty[..]);
    }

    // BoxType::as_bytes for Uuid
    #[test]
    fn box_type_as_bytes_uuid(ty in arb_box_type_uuid()) {
        let box_type = BoxType::Uuid(ty);
        prop_assert_eq!(box_type.as_bytes(), &ty[..]);
    }

    // BoxSize::U32 の get と external_size
    #[test]
    fn box_size_u32_properties(size in arb_box_size_u32()) {
        let box_size = BoxSize::U32(size);
        prop_assert_eq!(box_size.get(), size as u64);
        prop_assert_eq!(box_size.external_size(), 4);
    }

    // BoxSize::U64 の get と external_size
    #[test]
    fn box_size_u64_properties(size in arb_box_size_u64()) {
        let box_size = BoxSize::U64(size);
        prop_assert_eq!(box_size.get(), size);
        prop_assert_eq!(box_size.external_size(), 12); // 4 + 8
    }

    // BoxSize::with_payload_size が正しいサイズを返す
    #[test]
    fn box_size_with_payload_size(payload in 0u64..=(u32::MAX as u64 - 8)) {
        let box_type = BoxType::Normal(*b"test");
        let box_size = BoxSize::with_payload_size(box_type, payload);

        // サイズフィールド (4) + ボックス種別 (4) + ペイロード
        let expected = 4 + 4 + payload;
        prop_assert_eq!(box_size.get(), expected);
        prop_assert!(matches!(box_size, BoxSize::U32(_)));
    }

    // BoxHeader の Roundtrip (Normal タイプ、U32 サイズ)
    #[test]
    fn box_header_normal_u32_roundtrip(ty in arb_box_type_normal(), size in 8u32..=1000000u32) {
        let header = BoxHeader {
            box_type: BoxType::Normal(ty),
            box_size: BoxSize::U32(size),
        };
        let encoded = header.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded).unwrap();
        prop_assert_eq!(decode_size, header.external_size());
        prop_assert_eq!(decoded.box_type, header.box_type);
        prop_assert_eq!(decoded.box_size, header.box_size);
    }

    // BoxHeader の Roundtrip (Uuid タイプ、U32 サイズ)
    #[test]
    fn box_header_uuid_u32_roundtrip(ty in arb_box_type_uuid(), size in 24u32..=1000000u32) {
        let header = BoxHeader {
            box_type: BoxType::Uuid(ty),
            box_size: BoxSize::U32(size),
        };
        let encoded = header.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded).unwrap();
        prop_assert_eq!(decode_size, header.external_size());
        prop_assert_eq!(decoded.box_type, header.box_type);
        prop_assert_eq!(decoded.box_size, header.box_size);
    }

    // BoxHeader の Roundtrip (Normal タイプ、U64 サイズ)
    #[test]
    fn box_header_normal_u64_roundtrip(ty in arb_box_type_normal(), size in ((u32::MAX as u64) + 1)..=((u32::MAX as u64) + 1000000)) {
        let header = BoxHeader {
            box_type: BoxType::Normal(ty),
            box_size: BoxSize::U64(size),
        };
        let encoded = header.encode_to_vec().unwrap();

        prop_assert_eq!(encoded.len(), header.external_size());

        let (decoded, decode_size) = BoxHeader::decode(&encoded).unwrap();
        prop_assert_eq!(decode_size, header.external_size());
        prop_assert_eq!(decoded.box_type, header.box_type);
        prop_assert_eq!(decoded.box_size, header.box_size);
    }

    // Utf8String の Roundtrip
    #[test]
    fn utf8_string_roundtrip(s in arb_utf8_string()) {
        let utf8_str = Utf8String::new(&s).unwrap();
        let encoded = utf8_str.encode_to_vec().unwrap();

        // null 終端を含む
        prop_assert_eq!(encoded.len(), s.len() + 1);
        prop_assert_eq!(encoded.last(), Some(&0u8));

        let (decoded, size) = Utf8String::decode(&encoded).unwrap();
        prop_assert_eq!(size, s.len() + 1);
        prop_assert_eq!(decoded.get(), utf8_str.get());
    }

    // Utf8String::new は null を含む文字列を拒否
    #[test]
    fn utf8_string_rejects_null(prefix in "[^\x00]{0,10}", suffix in "[^\x00]{0,10}") {
        let s = format!("{}\x00{}", prefix, suffix);
        prop_assert!(Utf8String::new(&s).is_none());
    }

    // Mp4FileTime の from_secs と as_secs
    #[test]
    fn mp4_file_time_roundtrip(secs in any::<u64>()) {
        let time = Mp4FileTime::from_secs(secs);
        prop_assert_eq!(time.as_secs(), secs);
    }

    // Uint<u8, 4, 0> のビット操作
    #[test]
    fn uint_u8_4_0_from_bits(value in 0u8..=255) {
        let uint: Uint<u8, 4, 0> = Uint::from_bits(value);
        // 下位 4 ビットを抽出
        prop_assert_eq!(uint.get(), value & 0x0F);
    }

    // Uint<u8, 4, 4> のビット操作
    #[test]
    fn uint_u8_4_4_from_bits(value in 0u8..=255) {
        let uint: Uint<u8, 4, 4> = Uint::from_bits(value);
        // 上位 4 ビットを抽出
        prop_assert_eq!(uint.get(), (value >> 4) & 0x0F);
    }

    // Uint<u16, 12, 0> のビット操作
    #[test]
    fn uint_u16_12_0_from_bits(value in any::<u16>()) {
        let uint: Uint<u16, 12, 0> = Uint::from_bits(value);
        prop_assert_eq!(uint.get(), value & 0x0FFF);
    }

    // Uint の to_bits と from_bits の対称性
    #[test]
    fn uint_to_bits_from_bits_symmetry(value in 0u8..=0x0F) {
        let uint: Uint<u8, 4, 4> = Uint::new(value);
        let bits = uint.to_bits();
        let recovered: Uint<u8, 4, 4> = Uint::from_bits(bits);
        prop_assert_eq!(recovered.get(), value);
    }

    // Uint<T, 1, OFFSET> の as_bool
    #[test]
    fn uint_1_as_bool(value in any::<bool>()) {
        let uint: Uint<u8, 1, 0> = Uint::from(value);
        prop_assert_eq!(uint.as_bool(), value);
    }
}

/// エラーケースのテスト用モジュール
mod error_cases {
    use super::*;
    use shiguredo_mp4::ErrorKind;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        // 不十分なバッファでのエンコード: FullBoxFlags
        #[test]
        fn full_box_flags_encode_insufficient_buffer(
            value in arb_full_box_flags(),
            buf_size in 0usize..3
        ) {
            let flags = FullBoxFlags::new(value);
            let mut buf = vec![0u8; buf_size];
            let result = flags.encode(&mut buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 不十分なバッファでのエンコード: FullBoxHeader
        #[test]
        fn full_box_header_encode_insufficient_buffer(
            version in any::<u8>(),
            flags_value in arb_full_box_flags(),
            buf_size in 0usize..4
        ) {
            let header = FullBoxHeader {
                version,
                flags: FullBoxFlags::new(flags_value),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 不十分なバッファでのエンコード: BoxHeader (Normal, U32)
        #[test]
        fn box_header_encode_insufficient_buffer(
            ty in arb_box_type_normal(),
            size in 8u32..=1000000u32,
            buf_size in 0usize..8
        ) {
            let header = BoxHeader {
                box_type: BoxType::Normal(ty),
                box_size: BoxSize::U32(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 不十分なバッファでのエンコード: BoxHeader (Uuid, U32)
        #[test]
        fn box_header_uuid_encode_insufficient_buffer(
            ty in arb_box_type_uuid(),
            size in 24u32..=1000000u32,
            buf_size in 0usize..24
        ) {
            let header = BoxHeader {
                box_type: BoxType::Uuid(ty),
                box_size: BoxSize::U32(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 不十分なバッファでのエンコード: BoxHeader (Normal, U64)
        #[test]
        fn box_header_u64_encode_insufficient_buffer(
            ty in arb_box_type_normal(),
            size in ((u32::MAX as u64) + 1)..=((u32::MAX as u64) + 1000000),
            buf_size in 0usize..16
        ) {
            let header = BoxHeader {
                box_type: BoxType::Normal(ty),
                box_size: BoxSize::U64(size),
            };
            let mut buf = vec![0u8; buf_size];
            let result = header.encode(&mut buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 切り詰められた入力でのデコード: FullBoxFlags
        #[test]
        fn full_box_flags_decode_truncated(buf_size in 0usize..3) {
            let buf = vec![0xFFu8; buf_size];
            let result = FullBoxFlags::decode(&buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 切り詰められた入力でのデコード: FullBoxHeader
        #[test]
        fn full_box_header_decode_truncated(buf_size in 0usize..4) {
            let buf = vec![0xFFu8; buf_size];
            let result = FullBoxHeader::decode(&buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 切り詰められた入力でのデコード: BoxHeader
        #[test]
        fn box_header_decode_truncated(buf_size in 0usize..8) {
            let buf = vec![0xFFu8; buf_size];
            let result = BoxHeader::decode(&buf);
            prop_assert!(result.is_err());
        }

        // 切り詰められた入力でのデコード: FixedPointNumber<u8, u8>
        #[test]
        fn fixed_point_u8_u8_decode_truncated(buf_size in 0usize..2) {
            let buf = vec![0xFFu8; buf_size];
            let result = FixedPointNumber::<u8, u8>::decode(&buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // 切り詰められた入力でのデコード: FixedPointNumber<i16, u16>
        #[test]
        fn fixed_point_i16_u16_decode_truncated(buf_size in 0usize..4) {
            let buf = vec![0xFFu8; buf_size];
            let result = FixedPointNumber::<i16, u16>::decode(&buf);
            prop_assert!(result.is_err());
            prop_assert_eq!(result.unwrap_err().kind, ErrorKind::InsufficientBuffer);
        }

        // null 終端がない Utf8String のデコード
        #[test]
        fn utf8_string_decode_no_null_terminator(data in proptest::collection::vec(1u8..=255, 1..100)) {
            // null を含まないバイト列
            let result = Utf8String::decode(&data);
            prop_assert!(result.is_err());
        }

        // 不正なボックスサイズ (ヘッダーサイズより小さい)
        #[test]
        fn box_header_decode_invalid_size(size in 1u32..8) {
            // サイズフィールドが 1-7 の場合、ヘッダーサイズ (8) より小さいのでエラー
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&size.to_be_bytes());
            buf[4..8].copy_from_slice(b"test");

            let result = BoxHeader::decode(&buf);
            prop_assert!(result.is_err());
        }

        // 任意のバイト列でのデコード (クラッシュしないことを確認)
        #[test]
        fn box_header_decode_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // クラッシュしなければ OK (エラーは許容)
            let _ = BoxHeader::decode(&data);
        }

        // 任意のバイト列でのデコード: FullBoxFlags
        #[test]
        fn full_box_flags_decode_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..100)) {
            let _ = FullBoxFlags::decode(&data);
        }

        // 任意のバイト列でのデコード: FullBoxHeader
        #[test]
        fn full_box_header_decode_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..100)) {
            let _ = FullBoxHeader::decode(&data);
        }

        // 任意のバイト列でのデコード: Utf8String
        #[test]
        fn utf8_string_decode_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..256)) {
            let _ = Utf8String::decode(&data);
        }

        // 任意のバイト列でのデコード: FixedPointNumber
        #[test]
        fn fixed_point_decode_arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..100)) {
            let _ = FixedPointNumber::<u8, u8>::decode(&data);
            let _ = FixedPointNumber::<i16, u16>::decode(&data);
            let _ = FixedPointNumber::<i32, u32>::decode(&data);
        }
    }
}

/// 境界値テスト
mod boundary_tests {
    use super::*;
    use shiguredo_mp4::ErrorKind;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // BoxSize::with_payload_size が U64 になる境界
        #[test]
        fn box_size_u64_boundary(payload in (u32::MAX as u64 - 7)..=(u32::MAX as u64 + 100)) {
            let box_type = BoxType::Normal(*b"test");
            let box_size = BoxSize::with_payload_size(box_type, payload);

            // 4 + 4 + payload > u32::MAX の場合は U64
            let total = 8u64.saturating_add(payload);
            if total > u32::MAX as u64 {
                prop_assert!(matches!(box_size, BoxSize::U64(_)));
            } else {
                prop_assert!(matches!(box_size, BoxSize::U32(_)));
            }
        }

        // 大きなペイロードサイズ
        #[test]
        fn box_size_large_payload(payload in (u32::MAX as u64)..=u64::MAX) {
            let box_type = BoxType::Normal(*b"test");
            let box_size = BoxSize::with_payload_size(box_type, payload);

            // 常に U64 になるはず
            prop_assert!(matches!(box_size, BoxSize::U64(_)));
        }
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
        let encoded = flags.encode_to_vec().unwrap();
        assert_eq!(encoded.len(), 3);

        let (decoded, _) = FullBoxFlags::decode(&encoded).unwrap();
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
        let s = Utf8String::new("").unwrap();
        let encoded = s.encode_to_vec().unwrap();
        assert_eq!(encoded, vec![0]);

        let (decoded, size) = Utf8String::decode(&encoded).unwrap();
        assert_eq!(size, 1);
        assert_eq!(decoded.get(), "");
    }

    #[test]
    fn utf8_string_only_null() {
        // null のみのバイト列
        let buf = [0u8];
        let (decoded, size) = Utf8String::decode(&buf).unwrap();
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

        let (header, size) = BoxHeader::decode(&buf).unwrap();
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
            "HvccBox should return error for NAL unit length exceeding payload: got {:?}",
            result
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
            "HvccBox should return error for NAL unit length exceeding payload: got {:?}",
            result
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
            "VpccBox should return error for codec_init_size exceeding payload"
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
            "VpccBox should return error for codec_init_size exceeding payload: got {:?}",
            result
        );
    }
}

/// さらに変な値を使ったテスト
mod weird_values {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// 極端に大きいサイズフィールドを持つボックスヘッダー
        #[test]
        fn box_header_with_extreme_sizes(
            size_type in prop_oneof![
                Just(0xFFFFFFFFu32), // 最大 u32
                Just(0x80000000u32), // 符号付きで負になる値
                Just(0x7FFFFFFFu32), // 符号付き最大
                Just(1u32),          // 拡張サイズマーカー
                Just(2u32),          // 無効 (ヘッダーより小さい)
                Just(7u32),          // 境界 (ヘッダーより小さい)
            ],
            box_type in any::<[u8; 4]>()
        ) {
            let mut buf = [0u8; 8];
            buf[0..4].copy_from_slice(&size_type.to_be_bytes());
            buf[4..8].copy_from_slice(&box_type);

            // クラッシュしなければ OK
            let _ = BoxHeader::decode(&buf);
        }

        /// 極端なバイトパターン
        #[test]
        fn decode_extreme_byte_patterns(
            pattern in prop_oneof![
                Just(vec![0xFFu8; 64]),           // オール 0xFF
                Just(vec![0x00u8; 64]),           // オール 0x00
                Just(vec![0x80u8; 64]),           // オール 0x80 (符号ビット)
                Just(vec![0x7Fu8; 64]),           // オール 0x7F
                Just((0..64).map(|i| i as u8).collect::<Vec<_>>()),  // 連番
                Just((0..64).map(|i| (i * 2) as u8).collect::<Vec<_>>()), // 偶数
                Just((0..64).map(|i| if i % 2 == 0 { 0xFF } else { 0x00 }).collect::<Vec<_>>()), // 交互
                Just(vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]), // マジックバイト
            ]
        ) {
            // クラッシュしなければ OK
            let _ = BoxHeader::decode(&pattern);
            let _ = FullBoxFlags::decode(&pattern);
            let _ = FullBoxHeader::decode(&pattern);
            let _ = Utf8String::decode(&pattern);
            let _ = FixedPointNumber::<u8, u8>::decode(&pattern);
        }

        /// 拡張サイズ境界テスト (size=1 で不正な拡張サイズ)
        #[test]
        fn box_header_extended_size_edge_cases(
            extended_size in prop_oneof![
                Just(0u64),                     // 0 (無効)
                Just(1u64),                     // 1 (無効)
                Just(15u64),                    // ヘッダーサイズ未満
                Just(16u64),                    // ちょうどヘッダーサイズ
                Just(0xFFFFFFFFu64),            // u32 最大値
                Just(0x100000000u64),           // u32 + 1
                Just(u64::MAX),                 // 最大値
                Just(u64::MAX - 1),             // 最大値 - 1
            ]
        ) {
            let mut buf = vec![0u8; 16];
            buf[0..4].copy_from_slice(&1u32.to_be_bytes()); // size = 1 (extended)
            buf[4..8].copy_from_slice(b"test");
            buf[8..16].copy_from_slice(&extended_size.to_be_bytes());

            // クラッシュしなければ OK
            let _ = BoxHeader::decode(&buf);
        }

        /// UUID ボックスの変なパターン
        #[test]
        fn box_header_uuid_weird_patterns(
            uuid in prop_oneof![
                Just([0xFFu8; 16]),      // オール 0xFF
                Just([0x00u8; 16]),      // オール 0x00
                Just([0x80u8; 16]),      // オール 0x80
                any::<[u8; 16]>(),       // ランダム
            ],
            size in 24u32..=0xFFFFu32
        ) {
            let mut buf = vec![0u8; 24];
            buf[0..4].copy_from_slice(&size.to_be_bytes());
            buf[4..8].copy_from_slice(b"uuid");
            buf[8..24].copy_from_slice(&uuid);

            let result = BoxHeader::decode(&buf);
            if let Ok((header, _)) = result {
                // UUID として正しくデコードされたか確認
                assert!(matches!(header.box_type, BoxType::Uuid(_)));
            }
        }

        /// FullBoxFlags の境界ビット操作
        #[test]
        fn full_box_flags_boundary_bits(
            value in prop_oneof![
                Just(0u32),
                Just(1u32),
                Just(0x800000u32),       // ビット 23
                Just(0x400000u32),       // ビット 22
                Just(0x000001u32),       // ビット 0
                Just(0xAAAAAAu32),       // 交互パターン
                Just(0x555555u32),       // 逆交互パターン
                Just(0xFFFFFFu32),       // 24 ビット全部
                Just(0xFFFFFFFFu32),     // 32 ビット全部 (上位 8 ビットは切り捨て)
            ]
        ) {
            let flags = FullBoxFlags::new(value);
            let encoded = flags.encode_to_vec().unwrap();
            let (decoded, _) = FullBoxFlags::decode(&encoded).unwrap();

            // 上位 8 ビットは切り捨てられる
            prop_assert_eq!(decoded.get(), value & 0x00FFFFFF);
        }

        /// Mp4FileTime の極端な値
        #[test]
        fn mp4_file_time_extreme_values(
            secs in prop_oneof![
                Just(0u64),
                Just(1u64),
                Just(u64::MAX),
                Just(u64::MAX - 1),
                Just(2082844800u64),     // Unix エポック
                Just(0x80000000u64),     // 符号付き境界
                Just(0xFFFFFFFFu64),     // u32 最大
                Just(0x100000000u64),    // u32 + 1
            ]
        ) {
            let time = Mp4FileTime::from_secs(secs);
            prop_assert_eq!(time.as_secs(), secs);
        }

        /// Utf8String の変な文字
        #[test]
        fn utf8_string_weird_chars(
            s in prop_oneof![
                Just(String::new()),                              // 空
                Just("a".repeat(1000)),                           // 長い
                Just("\u{FEFF}BOM".to_string()),                  // BOM 付き
                Just("\u{200B}".to_string()),                     // ゼロ幅スペース
                Just("\u{FFFD}".to_string()),                     // 置換文字
                Just("日本語テスト".to_string()),                 // 日本語
                Just("🎉".to_string()),                           // 絵文字 (4バイト UTF-8)
                Just("\t\r\n".to_string()),                       // 制御文字
                Just("a\tb\rc\nd".to_string()),                   // 混合
            ]
        ) {
            if let Some(utf8_str) = Utf8String::new(&s) {
                let encoded = utf8_str.encode_to_vec().unwrap();
                let (decoded, _) = Utf8String::decode(&encoded).unwrap();
                prop_assert_eq!(decoded.get(), utf8_str.get());
            }
        }

        /// 不正な UTF-8 シーケンスのデコード
        #[test]
        fn utf8_string_invalid_sequences(
            data in prop_oneof![
                Just(vec![0x80, 0x00]),           // 継続バイトから開始
                Just(vec![0xC0, 0x80, 0x00]),     // オーバーロングエンコード
                Just(vec![0xE0, 0x80, 0x80, 0x00]), // オーバーロングエンコード
                Just(vec![0xF0, 0x80, 0x80, 0x80, 0x00]), // オーバーロングエンコード
                Just(vec![0xFE, 0x00]),           // 無効な先頭バイト
                Just(vec![0xFF, 0x00]),           // 無効な先頭バイト
                Just(vec![0xC2, 0x00]),           // 不完全なシーケンス (継続バイトがない)
                Just(vec![0xE0, 0xA0, 0x00]),     // 不完全なシーケンス
                Just(vec![0xED, 0xA0, 0x80, 0x00]), // サロゲートペア (UTF-8 では無効)
            ]
        ) {
            let result = Utf8String::decode(&data);
            // 不正な UTF-8 はエラーになるはず
            prop_assert!(result.is_err());
        }
    }
}
