//! `src/basic_types.rs` の意図的なエラーパス・境界値に関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_basic_types.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい `decode_header_and_payload` の
//! size=0 / largesize=0 / サイズ下限 / バッファ不足の挙動、および
//! `LanguageCode` / `Utf8String::Default` の境界値を固定する。

mod decode_header_and_payload_size_zero {
    use shiguredo_mp4::{BoxHeader, BoxSize, BoxType, ErrorKind};

    /// size=0（`BoxSize::VARIABLE_SIZE`）のとき、ペイロードはヘッダー直後からバッファ末尾までになること
    #[test]
    fn u32_size_zero_uses_buffer_tail_as_payload() {
        // size=0 (4) + type "mdat" (4) + ペイロード 3 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        let (header, payload) = BoxHeader::decode_header_and_payload(&buf)
            .expect("VARIABLE_SIZE はバッファ末尾までをボックスとしてデコードできる");

        assert_eq!(header.box_type, BoxType::Normal(*b"mdat"));
        assert_eq!(header.box_size, BoxSize::VARIABLE_SIZE);
        assert_eq!(
            payload,
            &buf[8..],
            "ペイロードはヘッダー直後からバッファ末尾までであること"
        );
    }

    /// size=0 でヘッダーのみのバッファなら、ペイロードは空になること
    #[test]
    fn u32_size_zero_with_header_only_returns_empty_payload() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");

        let (header, payload) = BoxHeader::decode_header_and_payload(&buf)
            .expect("VARIABLE_SIZE でヘッダーのみでもデコードできる");

        assert_eq!(header.box_size, BoxSize::VARIABLE_SIZE);
        assert!(
            payload.is_empty(),
            "ヘッダーのみのときペイロードは空であること: 実際の長さ {}",
            payload.len()
        );
    }

    /// largesize=0（`BoxSize::U64(0)`）は仕様未定義のためエラーになること
    #[test]
    fn u64_largesize_zero_returns_invalid_data() {
        // size=1 (4) + type "mdat" (4) + largesize=0 (8)
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&0u64.to_be_bytes());
        // ペイロードを足しても largesize=0 はエラーになる
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        let err = BoxHeader::decode_header_and_payload(&buf)
            .expect_err("U64(0) は可変長として扱わずエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("box size is smaller than header size"),
            "エラー理由はサイズ下限違反であること: 実際は {}",
            err.reason
        );
    }

    /// size > 0 かつヘッダーより小さい場合はエラーになること（回帰防止）
    ///
    /// 現行では `BoxHeader::decode` 側の下限検査で `InvalidInput` になる。
    /// `decode_header_and_payload` 経由でも引き続きエラーになることを固定する。
    #[test]
    fn size_smaller_than_header_returns_error() {
        // size=4（ヘッダー 8 バイト未満）+ type "mdat"
        let mut buf = Vec::new();
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");

        let err = BoxHeader::decode_header_and_payload(&buf)
            .expect_err("ヘッダーより小さい size はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidInput);
    }

    /// size がバッファ長を超える場合は `InsufficientBuffer` になること
    ///
    /// `check_buffer_size` を size=0 / 下限判定の後に置く順序の回帰固定。
    #[test]
    fn size_exceeds_buffer_returns_insufficient_buffer() {
        // size=16（ボックス全体 16 バイトを要求）だがバッファはヘッダー 8 バイトのみ
        let mut buf = Vec::new();
        buf.extend_from_slice(&16u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");

        let err = BoxHeader::decode_header_and_payload(&buf)
            .expect_err("宣言サイズがバッファを超える場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
    }
}

mod language_code {
    use shiguredo_mp4::LanguageCode;

    /// `LanguageCode::UNDEFINED` が `*b"und"` であること
    #[test]
    fn undefined_is_und() {
        assert_eq!(LanguageCode::UNDEFINED.as_bytes(), *b"und");
        assert_eq!(LanguageCode::default(), LanguageCode::UNDEFINED);
    }

    /// 受理範囲の境界（`0x60` / `0x7F`）を含む 3 バイトは成功すること
    #[test]
    fn new_accepts_boundary_bytes() {
        assert_eq!(
            LanguageCode::new([0x60, 0x60, 0x60])
                .expect("下限は受理される")
                .as_bytes(),
            [0x60, 0x60, 0x60]
        );
        assert_eq!(
            LanguageCode::new([0x7F, 0x7F, 0x7F])
                .expect("上限は受理される")
                .as_bytes(),
            [0x7F, 0x7F, 0x7F]
        );
        assert_eq!(
            LanguageCode::new(*b"eng")
                .expect("ISO-639-2/T の小文字は受理される")
                .as_bytes(),
            *b"eng"
        );
    }

    /// 受理範囲の外側（`0x5F` / `0x80`）を 1 バイトでも含むと拒否すること
    ///
    /// 位置ごとの網羅（1 バイト目・2 バイト目・3 バイト目の単独違反）を確認する
    #[test]
    fn new_rejects_out_of_range_bytes() {
        assert!(
            LanguageCode::new([0x5F, b'n', b'd']).is_none(),
            "1 バイト目が 0x5F（範囲外）なので拒否される"
        );
        assert!(
            LanguageCode::new([b'u', 0x80, b'd']).is_none(),
            "2 バイト目が 0x80（範囲外）なので拒否される"
        );
        assert!(
            LanguageCode::new([b'u', b'n', 0x5F]).is_none(),
            "3 バイト目が 0x5F（範囲外）なので拒否される"
        );
        assert!(
            LanguageCode::new([b'u', b'n', 0x80]).is_none(),
            "3 バイト目が 0x80（範囲外）なので拒否される"
        );
        assert!(
            LanguageCode::new([0x00, 0x00, 0x00]).is_none(),
            "全バイトが 0x00（範囲外）なので拒否される"
        );
    }

    /// `from_ascii` は 3 バイトかつ各バイトが `0x60..=0x7F` のときだけ成功すること
    #[test]
    fn from_ascii_accepts_valid_three_byte_codes() {
        assert_eq!(
            LanguageCode::from_ascii("jpn").expect("小文字 3 文字は受理される"),
            LanguageCode::new(*b"jpn").expect("new でも受理される")
        );
        assert_eq!(
            LanguageCode::from_ascii("`{|")
                .expect("0x60..=0x7F 内なら非 a-z も受理される")
                .as_bytes(),
            [0x60, 0x7B, 0x7C]
        );
    }

    /// バイト長が 3 以外、または大文字を含む文字列は拒否すること
    #[test]
    fn from_ascii_rejects_invalid_inputs() {
        assert!(
            LanguageCode::from_ascii("").is_none(),
            "空文字列は拒否される"
        );
        assert!(
            LanguageCode::from_ascii("en").is_none(),
            "2 文字は拒否される"
        );
        assert!(
            LanguageCode::from_ascii("engx").is_none(),
            "4 文字は拒否される"
        );
        assert!(
            LanguageCode::from_ascii("ENG").is_none(),
            "大文字は 0x60..=0x7F の外なので拒否される"
        );
        // 「あ」は 3 バイト UTF-8。バイト長は 3 だが個々のバイトが 0x60..=0x7F の外に落ちる
        assert!(
            LanguageCode::from_ascii("あ").is_none(),
            "非 ASCII マルチバイト文字はバイト範囲外として拒否される"
        );
    }
}

mod utf8_string_default {
    use shiguredo_mp4::Utf8String;

    /// `Utf8String::default()` が `Utf8String::EMPTY`（空文字列）と等しいこと
    #[test]
    fn default_equals_empty() {
        assert_eq!(Utf8String::default(), Utf8String::EMPTY);
        assert_eq!(Utf8String::default().get(), "");
    }
}

// ===== pbt/tests/prop_basic_types.rs の boundary_tests 固定入力から移動 =====

mod boundary_tests_from_prop_basic_types {
    use shiguredo_mp4::{
        BoxHeader, BoxSize, BoxType, Decode, Encode, ErrorKind, FullBoxFlags, Mp4FileTime,
        Utf8String,
    };

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
