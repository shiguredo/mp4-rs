//! `src/boxes.rs` の意図的なエラーパス・境界値に関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_additional_boxes.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい `UnknownBox` の可変長サイズ
//! （`BoxSize::VARIABLE_SIZE`）の扱いを以下の 3 経路で固定する:
//! - `UnknownBox::decode` への直接入力（可変長サイズはエラーになる）
//! - コンテナボックス（`stpp`）内部の未知ボックスループ（可変長サイズでエラーになる）
//! - `RootBox` の未知型 top-level ボックス（可変長サイズを従来どおり受理する）

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

    /// `UnknownBox::decode_top_level` は size=0（`BoxSize::VARIABLE_SIZE`）を受理すること
    ///
    /// `RootBox::decode` の未知型分岐はこの関数を使うため、`UnknownBox::decode` の
    /// 拒否との対比でトップレベルの許容を固定する
    #[test]
    fn decode_top_level_size_zero_succeeds() {
        // size=0 (4) + type "xxxx" (4) + ペイロード 4 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"xxxx");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let (unknown, size) =
            UnknownBox::decode_top_level(&buf).expect("decode_top_level は size=0 を受理する");
        assert_eq!(size, buf.len());
        assert_eq!(unknown.box_size, BoxSize::VARIABLE_SIZE);
        assert_eq!(unknown.box_type, BoxType::Normal(*b"xxxx"));
        assert_eq!(
            unknown.payload,
            &buf[8..],
            "ペイロードはヘッダー直後からバッファ末尾までであること"
        );
    }
}

mod stpp_box_trailing_zero_padding {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::StppBox};

    /// `StppBox::decode` で、末尾のゼロ埋めが size=0 の未知ボックスとして読める長さ（8 バイト以上）あるとエラーになること
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
    use shiguredo_mp4::{BoxSize, Decode, boxes::RootBox};

    /// `RootBox::decode` の未知型分岐は、size=0 の未知型 top-level ボックスを従来どおり受理すること
    #[test]
    fn unknown_type_size_zero_succeeds() {
        // size=0 (4) + type "test" (4) + ペイロード 4 バイト
        let mut buf = Vec::new();
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(b"test");
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

        let (decoded, size) =
            RootBox::decode(&buf).expect("未知型 top-level の size=0 は従来どおり成功する");
        assert_eq!(size, buf.len());
        let unknown = match decoded {
            RootBox::Unknown(unknown) => unknown,
            _ => panic!("未知型 top-level は RootBox::Unknown にデコードされる"),
        };
        assert_eq!(unknown.box_size, BoxSize::VARIABLE_SIZE);
        assert_eq!(
            unknown.payload,
            &buf[8..],
            "ペイロードはヘッダー直後からバッファ末尾までであること"
        );
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

// ===== pbt/tests/prop_additional_boxes.rs の root_box_tests から移動 =====

// ===== RootBox のテスト =====

mod root_box_tests {
    use shiguredo_mp4::{
        BaseBox, BoxSize, BoxType, Decode, Encode,
        boxes::{Brand, FreeBox, MdatBox, RootBox, UnknownBox},
    };

    /// RootBox::Free の encode/decode roundtrip
    #[test]
    fn root_box_free_roundtrip() {
        let free = FreeBox {
            payload: vec![0u8; 100],
        };
        let root = RootBox::Free(free);

        let encoded = root.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = RootBox::decode(&encoded)
            .expect("直前にエンコードした有効な RootBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Free(_)));
        assert_eq!(decoded.box_type(), FreeBox::TYPE);
        assert!(!decoded.is_unknown_box());

        // children() のテスト
        assert_eq!(decoded.children().count(), 0);
    }

    /// RootBox::Mdat の encode/decode roundtrip
    #[test]
    fn root_box_mdat_roundtrip() {
        let mdat = MdatBox {
            payload: vec![1, 2, 3, 4, 5],
        };
        let root = RootBox::Mdat(mdat);

        let encoded = root.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = RootBox::decode(&encoded)
            .expect("直前にエンコードした有効な RootBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Mdat(_)));
        assert_eq!(decoded.box_type(), MdatBox::TYPE);
        assert!(!decoded.is_unknown_box());
    }

    /// RootBox::Unknown の encode/decode roundtrip
    #[test]
    fn root_box_unknown_roundtrip() {
        let unknown = UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::with_payload_size(BoxType::Normal(*b"test"), 10),
            payload: vec![0u8; 10],
        };
        let root = RootBox::Unknown(unknown);

        let encoded = root.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = RootBox::decode(&encoded)
            .expect("直前にエンコードした有効な RootBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert!(matches!(decoded, RootBox::Unknown(_)));
        assert_eq!(decoded.box_type(), BoxType::Normal(*b"test"));
        assert!(decoded.is_unknown_box());
    }

    /// Brand の Debug 実装テスト: 有効な UTF-8
    #[test]
    fn brand_debug_valid_utf8() {
        let brand = Brand::new(*b"isom");
        let debug_str = format!("{brand:?}");
        assert!(debug_str.contains("isom"));
    }

    /// Brand の Debug 実装テスト: 無効な UTF-8
    #[test]
    fn brand_debug_invalid_utf8() {
        let brand = Brand::new([0xFF, 0xFE, 0x00, 0x01]);
        let debug_str = format!("{brand:?}");
        // 無効な UTF-8 の場合はバイト配列として表示される
        assert!(debug_str.contains("Brand"));
    }

    /// Brand の各定数のテスト
    #[test]
    fn brand_constants() {
        assert_eq!(Brand::ISOM.get(), *b"isom");
        assert_eq!(Brand::AVC1.get(), *b"avc1");
        assert_eq!(Brand::ISO2.get(), *b"iso2");
        assert_eq!(Brand::MP71.get(), *b"mp71");
        assert_eq!(Brand::ISO3.get(), *b"iso3");
        assert_eq!(Brand::ISO4.get(), *b"iso4");
        assert_eq!(Brand::ISO5.get(), *b"iso5");
        assert_eq!(Brand::ISO6.get(), *b"iso6");
        assert_eq!(Brand::ISO7.get(), *b"iso7");
        assert_eq!(Brand::ISO8.get(), *b"iso8");
        assert_eq!(Brand::ISO9.get(), *b"iso9");
        assert_eq!(Brand::ISOA.get(), *b"isoa");
        assert_eq!(Brand::ISOB.get(), *b"isob");
        assert_eq!(Brand::RELO.get(), *b"relo");
        assert_eq!(Brand::MP41.get(), *b"mp41");
        assert_eq!(Brand::AV01.get(), *b"av01");
    }

    /// Brand の encode/decode roundtrip
    #[test]
    fn brand_roundtrip() {
        let brand = Brand::new(*b"test");
        let encoded = brand.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) =
            Brand::decode(&encoded).expect("直前にエンコードした有効な Brand は必ずデコードできる");

        assert_eq!(size, 4);
        assert_eq!(decoded.get(), *b"test");
    }
}

// ===== pbt/tests/prop_additional_boxes.rs の boundary_tests (Free/Mdat/Unknown) から移動 =====

mod additional_boxes_boundary_tests {
    use shiguredo_mp4::{
        BoxSize, BoxType, Decode, Encode,
        boxes::{FreeBox, MdatBox, UnknownBox},
    };

    /// UnknownBox: 空のペイロード
    #[test]
    fn unknown_box_empty_payload() {
        let unknown = UnknownBox {
            box_type: BoxType::Normal(*b"test"),
            box_size: BoxSize::with_payload_size(BoxType::Normal(*b"test"), 0),
            payload: vec![],
        };
        let encoded = unknown
            .encode_to_vec()
            .expect("Vec への書き込みは失敗しない");
        let (decoded, _) = UnknownBox::decode(&encoded)
            .expect("直前にエンコードした有効な UnknownBox は必ずデコードできる");
        assert!(decoded.payload.is_empty());
    }

    /// FreeBox: 空のペイロード
    #[test]
    fn free_box_empty_payload() {
        let free = FreeBox { payload: vec![] };
        let encoded = free.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = FreeBox::decode(&encoded)
            .expect("直前にエンコードした有効な FreeBox は必ずデコードできる");
        assert!(decoded.payload.is_empty());
    }

    /// MdatBox: 空のペイロード
    #[test]
    fn mdat_box_empty_payload() {
        let mdat = MdatBox { payload: vec![] };
        let encoded = mdat.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = MdatBox::decode(&encoded)
            .expect("直前にエンコードした有効な MdatBox は必ずデコードできる");
        assert!(decoded.payload.is_empty());
    }
}
