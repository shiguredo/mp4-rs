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
    #[test]
    fn new_rejects_out_of_range_bytes() {
        assert!(
            LanguageCode::new([0x5F, b'n', b'd']).is_none(),
            "0x5F は範囲外なので拒否される"
        );
        assert!(
            LanguageCode::new([b'u', 0x80, b'd']).is_none(),
            "0x80 は範囲外なので拒否される"
        );
        assert!(
            LanguageCode::new([0x00, 0x00, 0x00]).is_none(),
            "0x00 は範囲外なので拒否される"
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
