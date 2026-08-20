//! `src/boxes.rs` の意図的なエラーパス・境界値に関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_additional_boxes.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい `UnknownBox` の size=0 拒否と、
//! コンテナ box 内部の未知 box ループが size=0 で停止する挙動を固定する。

mod unknown_box_size_zero {
    use shiguredo_mp4::{BoxSize, BoxType, Decode, ErrorKind, boxes::UnknownBox};

    /// `UnknownBox::decode` に size=0（`BoxSize::VARIABLE_SIZE`）のバッファを与えるとエラーになること
    #[test]
    fn size_zero_returns_invalid_data() {
        // size=0 (4) + type "xxxx" (4) + ペイロード 4 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"xxxx");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let err = UnknownBox::decode(&buf).expect_err("size=0 の UnknownBox はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("UnknownBox does not accept size=0"),
            "エラー理由に size=0 拒否が含まれること: 実際は {}",
            err.reason
        );
    }

    /// `UnknownBox::decode` に size=8（ヘッダーのみで空ペイロード）のバッファを与えると成功すること
    ///
    /// size=0 拒否の追加による正常系の回帰確認
    #[test]
    fn size_eight_with_empty_payload_succeeds() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(b"xxxx");

        let (unknown, size) =
            UnknownBox::decode(&buf).expect("size=8 のヘッダーのみ UnknownBox はデコードできる");
        assert_eq!(size, buf.len());
        assert_eq!(unknown.box_size, BoxSize::U32(8));
        assert_eq!(unknown.box_type, BoxType::Normal(*b"xxxx"));
        assert!(
            unknown.payload.is_empty(),
            "size=8 はヘッダーのみなのでペイロードは空であること"
        );
    }

    /// size=1 + largesize=0（`BoxSize::LARGE_VARIABLE_SIZE`）も引き続きエラーになること
    ///
    /// `decode_header_and_payload` のサイズ下限検査に任せた経路の回帰確認
    #[test]
    fn u64_largesize_zero_returns_invalid_data() {
        // size=1 (4) + type "xxxx" (4) + largesize=0 (8)
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(b"xxxx");
        buf.extend_from_slice(&0u64.to_be_bytes());

        let err = UnknownBox::decode(&buf).expect_err("largesize=0 はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
    }
}

mod stpp_box_trailing_zero_padding {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::StppBox};

    /// `StppBox::decode` で、末尾のゼロ埋めが size=0 の未知 box として読める長さ（8 バイト以上）あるとエラーになること
    ///
    /// 3 つの null 終端空文字列（namespace / schema_location / auxiliary_mime_types）を
    /// 正常に消費した後、残ったゼロパディングが `UnknownBox` の size=0 として
    /// 吸収されずにエラーになることを確認する。
    /// ゼロ埋めが 8 バイト以上ないと `UnknownBox` は size フィールドと type フィールドを
    /// 読み切れず `InsufficientBuffer` になる（従来も失敗）ため、
    /// 8 バイト以上にして size=0 の拒否（新挙動）を確実に検証する
    #[test]
    fn trailing_zero_padding_returns_invalid_data() {
        // size (4) + type "stpp" (4) + reserved (6) + data_reference_index (2)
        // + namespace (1) + schema_location (1) + auxiliary_mime_types (1) + ゼロ埋め (8)
        let mut buf = Vec::new();
        buf.extend_from_slice(&27u32.to_be_bytes());
        buf.extend_from_slice(b"stpp");
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&[0x00]);
        buf.extend_from_slice(&[0x00]);
        buf.extend_from_slice(&[0x00]);
        buf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

        let err =
            StppBox::decode(&buf).expect_err("末尾ゼロ埋めが size=0 として読めるとエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("UnknownBox does not accept size=0"),
            "エラー理由に size=0 拒否が含まれること: 実際は {}",
            err.reason
        );
    }
}

mod root_box_unknown_size_zero {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::RootBox};

    /// `RootBox::decode` の未知型分岐は、size=0 の未知型 top-level box でエラーになること
    #[test]
    fn unknown_type_size_zero_returns_invalid_data() {
        // size=0 (4) + type "test" (4)
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"test");

        let err = RootBox::decode(&buf).expect_err("未知型 top-level の size=0 はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
    }

    /// `RootBox::decode` の既知型分岐（`mdat`）は、size=0 を従来どおり受理すること
    #[test]
    fn known_type_mdat_size_zero_succeeds() {
        // size=0 (4) + type "mdat" (4) + ペイロード 3 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"mdat");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        let (decoded, size) =
            RootBox::decode(&buf).expect("既知型 mdat の size=0 は従来どおり成功する");
        assert!(matches!(decoded, RootBox::Mdat(_)));
        assert_eq!(size, buf.len());
    }
}
