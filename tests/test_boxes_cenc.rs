//! `src/boxes_cenc.rs` の既知バイト列と意図的なエラーパスに関する回帰テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_boxes_cenc.rs` が担う。
//! 本ファイルは PBT で安定して狙いにくい既知バイト列との一致、FullBox バージョン不正、
//! constant IV の有無と IV サイズの不整合、`sinf` の `schm` / `schi` / `tenc` の
//! 組み合わせ不正といったエラーパスを固定する。

/// 4 バイト size + 4 バイトボックス種別 + ペイロードからボックスの生バイト列を組み立てる
///
/// decode のエラーパスを検証するときに、正常な子ボックスと壊れたボックスを
/// バイト列の段階で組み立てるために使う。
fn build_box(box_type: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = u32::try_from(payload.len() + 8).expect("テスト用のボックスサイズは u32 に収まる");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&size.to_be_bytes());
    bytes.extend_from_slice(&box_type);
    bytes.extend_from_slice(payload);
    bytes
}

// ===== 既知バイト列でのエンコード検証 =====

mod known_bytes_tests {
    use shiguredo_mp4::{
        Encode, Uint,
        boxes::{PsshBox, TencBox},
    };

    /// Widevine の system ID（既知バイト列検証用の固定値）
    const SYSTEM_ID: [u8; 16] = [
        0xED, 0xEF, 0x8B, 0xA9, 0x79, 0xD6, 0x4A, 0xCE, 0xA3, 0xC8, 0x27, 0xDC, 0xD5, 0x1D, 0x21,
        0xED,
    ];

    /// PsshBox (version 0) のエンコード結果が仕様どおりのバイト列になること
    #[test]
    fn pssh_box_version_0_encode_known_bytes() {
        let pssh = PsshBox {
            version: 0,
            system_id: SYSTEM_ID,
            kids: Vec::new(),
            data: vec![0xAA, 0xBB, 0xCC],
        };
        let encoded = pssh.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (FullBox) + 16 (system_id) + 4 (data_size) + 3 (data)
        expected.extend_from_slice(&35u32.to_be_bytes());
        expected.extend_from_slice(b"pssh");
        // version 0, flags 0
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(&SYSTEM_ID);
        expected.extend_from_slice(&3u32.to_be_bytes());
        expected.extend_from_slice(&[0xAA, 0xBB, 0xCC]);

        assert_eq!(
            encoded, expected,
            "PsshBox version 0 のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// PsshBox (version 1) のエンコード結果が仕様どおりのバイト列になること
    ///
    /// version 1 では `system_id` の直後に `KID_count` と各 KID が書かれる。
    #[test]
    fn pssh_box_version_1_encode_known_bytes() {
        let kid1 = [0x11; 16];
        let kid2 = [0x22; 16];
        let pssh = PsshBox {
            version: 1,
            system_id: SYSTEM_ID,
            kids: vec![kid1, kid2],
            data: vec![0xDD, 0xEE],
        };
        let encoded = pssh.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 + 16 + 4 + 16 * 2 + 4 + 2 = 70
        expected.extend_from_slice(&70u32.to_be_bytes());
        expected.extend_from_slice(b"pssh");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(&SYSTEM_ID);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&kid1);
        expected.extend_from_slice(&kid2);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&[0xDD, 0xEE]);

        assert_eq!(
            encoded, expected,
            "PsshBox version 1 のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// TencBox (version 0) のエンコード結果が仕様どおりのバイト列になること
    ///
    /// パターン暗号化パラメーターを持たない場合は version 0 として書き出され、
    /// 予約フィールドが 2 バイト続く。
    #[test]
    fn tenc_box_version_0_encode_known_bytes() {
        let kid = [0x33; 16];
        let tenc = TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 8,
            default_kid: kid,
            default_constant_iv: None,
            default_crypt_byte_block: None,
            default_skip_byte_block: None,
        };
        let encoded = tenc.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (FullBox) + 1 + 1 + 1 + 1 + 16 = 32
        expected.extend_from_slice(&32u32.to_be_bytes());
        expected.extend_from_slice(b"tenc");
        // version 0, flags 0
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        // 予約フィールド (version 0 では 2 バイト)
        expected.extend_from_slice(&[0x00, 0x00]);
        // default_isProtected = 1, default_Per_Sample_IV_Size = 8
        expected.extend_from_slice(&[0x01, 0x08]);
        expected.extend_from_slice(&kid);

        assert_eq!(
            encoded, expected,
            "TencBox version 0 のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// TencBox (version 1, パターン暗号化 + constant IV) のエンコード結果が
    /// 仕様どおりのバイト列になること
    ///
    /// パターン暗号化パラメーターがある場合は version 1 として書き出され、
    /// 予約フィールドに続いて crypt / skip が 4 ビットずつ 1 バイトに詰められる。
    /// constant IV はサイズ 1 バイト + 本体の形で末尾に書かれる。
    #[test]
    fn tenc_box_version_1_encode_known_bytes() {
        let kid = [0x44; 16];
        let constant_iv = [0x55u8; 8];
        let tenc = TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 0,
            default_kid: kid,
            default_constant_iv: Some(constant_iv.to_vec()),
            default_crypt_byte_block: Some(Uint::new(1)),
            default_skip_byte_block: Some(Uint::new(9)),
        };
        let encoded = tenc.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 + 1 (予約) + 1 (パターン) + 1 + 1 + 16 + 1 + 8 = 41
        expected.extend_from_slice(&41u32.to_be_bytes());
        expected.extend_from_slice(b"tenc");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        // 予約フィールド + default_crypt_byte_block = 1 (上位 4 ビット) / default_skip_byte_block = 9 (下位 4 ビット)
        expected.extend_from_slice(&[0x00, 0x19]);
        // default_isProtected = 1, default_Per_Sample_IV_Size = 0
        expected.extend_from_slice(&[0x01, 0x00]);
        expected.extend_from_slice(&kid);
        // constant IV のサイズと本体
        expected.extend_from_slice(&[0x08]);
        expected.extend_from_slice(&constant_iv);

        assert_eq!(
            encoded, expected,
            "TencBox version 1 のエンコード結果が既知のバイト列と一致しない"
        );
    }
}

// ===== PsshBox のエラーパステスト =====

mod pssh_error_tests {
    use shiguredo_mp4::{Decode, Encode, ErrorKind, boxes::PsshBox};

    use super::build_box;

    /// PsshBox: version 0 で KID を持つ場合のエンコードが InvalidInput になること
    #[test]
    fn pssh_box_version_0_with_kids_encode_is_invalid_input() {
        let pssh = PsshBox {
            version: 0,
            system_id: [0x00; 16],
            kids: vec![[0x11; 16]],
            data: Vec::new(),
        };
        let err = pssh
            .encode_to_vec()
            .expect_err("version 0 で KID を持つ PsshBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("version 0 must not have KIDs"),
            "KID の有無の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// PsshBox: version 2 のエンコードが InvalidInput になること
    #[test]
    fn pssh_box_version_2_encode_is_invalid_input() {
        let pssh = PsshBox {
            version: 2,
            system_id: [0x00; 16],
            kids: Vec::new(),
            data: Vec::new(),
        };
        let err = pssh
            .encode_to_vec()
            .expect_err("version 2 の PsshBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("version must be 0 or 1"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// PsshBox: version 2 のデコードが InvalidData になること
    #[test]
    fn pssh_box_version_2_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 2, flags 0
        payload.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&[0x00; 16]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"pssh", &payload);

        let err = PsshBox::decode(&buf).expect_err("version 2 の PsshBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported PsshBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// PsshBox: `DataSize` が残りのペイロードを超える場合のデコードが InvalidData になること
    #[test]
    fn pssh_box_data_size_exceeds_payload_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&[0x00; 16]);
        // データサイズは 5 バイトと宣言するが、実データは 2 バイトしかない
        payload.extend_from_slice(&5u32.to_be_bytes());
        payload.extend_from_slice(&[0xAA, 0xBB]);
        let buf = build_box(*b"pssh", &payload);

        let err =
            PsshBox::decode(&buf).expect_err("data_size が残りを超える PsshBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("data size exceeds remaining payload"),
            "データサイズの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// PsshBox: データの後ろに余分なバイトがある場合のデコードが InvalidData になること
    #[test]
    fn pssh_box_trailing_bytes_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&[0x00; 16]);
        // データサイズ 0 の後ろに余分な 1 バイトを置く
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.push(0xAA);
        let buf = build_box(*b"pssh", &payload);

        let err = PsshBox::decode(&buf)
            .expect_err("データの後ろに余分なバイトを持つ PsshBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("trailing bytes"),
            "末尾バイトの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== SchmBox のテスト =====

mod schm_tests {
    use shiguredo_mp4::{Decode, Encode, ErrorKind, Utf8String, boxes::SchmBox};

    use super::build_box;

    /// SchmBox: `scheme_uri` を持つ場合に flags の `0x000001` が立ち、URI が末尾に書かれること
    #[test]
    fn schm_box_with_uri_sets_flag() {
        let schm = SchmBox {
            scheme_type: *b"cenc",
            scheme_version: 0x0001_0000,
            scheme_uri: Some(Utf8String::new("urn:test:scheme").expect("null 文字を含まない")),
        };
        let encoded = schm.encode_to_vec().expect("Vec への書き込みは失敗しない");

        // FullBox ヘッダーは encoded[8..12] にあり、scheme_uri がある場合だけ flags に 0x000001 が立つ
        assert_eq!(
            &encoded[8..12],
            &[0x00, 0x00, 0x00, 0x01],
            "scheme_uri がある場合は flags の 0x000001 が立つ"
        );
        // scheme_type (4 バイト) と scheme_version (4 バイト) の直後に null 終端で書かれる
        assert_eq!(
            &encoded[20..],
            b"urn:test:scheme\0",
            "scheme_uri は scheme_version の直後に null 終端で書かれる"
        );
    }

    /// SchmBox: `scheme_uri` を持たない場合に flags が 0 になり、URI が書かれないこと
    #[test]
    fn schm_box_without_uri_clears_flag() {
        let schm = SchmBox {
            scheme_type: *b"cbc1",
            scheme_version: 0,
            scheme_uri: None,
        };
        let encoded = schm.encode_to_vec().expect("Vec への書き込みは失敗しない");

        assert_eq!(
            &encoded[8..12],
            &[0x00, 0x00, 0x00, 0x00],
            "scheme_uri がない場合は flags が 0 になる"
        );
        assert_eq!(
            encoded.len(),
            20,
            "scheme_uri がない場合は scheme_version の直後で終わる"
        );
    }

    /// SchmBox: version 1 のデコードが InvalidData になること
    #[test]
    fn schm_box_version_1_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 1, flags 0
        payload.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(b"cenc");
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"schm", &payload);

        let err = SchmBox::decode(&buf).expect_err("version 1 の SchmBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SchmBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SchmBox: 末尾に余分なバイトがある場合のデコードが InvalidData になること
    #[test]
    fn schm_box_trailing_bytes_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(b"cenc");
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.push(0xFF);
        let buf = build_box(*b"schm", &payload);

        let err =
            SchmBox::decode(&buf).expect_err("末尾に余分なバイトがある SchmBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("trailing bytes"),
            "末尾バイトの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== TencBox のエラーパステスト =====

mod tenc_error_tests {
    use shiguredo_mp4::{Encode, ErrorKind, Uint, boxes::TencBox};

    /// 検証の基点となる、すべてのフィールドが有効な TencBox を返す
    fn base_tenc_box() -> TencBox {
        TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 8,
            default_kid: [0x00; 16],
            default_constant_iv: None,
            default_crypt_byte_block: None,
            default_skip_byte_block: None,
        }
    }

    /// エンコードが InvalidInput になり、reason が指定された文言を含むことを確認する
    #[track_caller]
    fn assert_encode_invalid_input(tenc: &TencBox, expected_reason: &str, ctx: &str) {
        let err = tenc
            .encode_to_vec()
            .expect_err(&format!("不正な TencBox なのにエンコードが成功した: {ctx}"));
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない: {ctx} (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains(expected_reason),
            "意図した検査とは別のエラーである: {ctx} (reason={})",
            err.reason,
        );
    }

    /// TencBox: `default_is_protected` が 2 の場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_invalid_default_is_protected_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_is_protected: 2,
                ..base_tenc_box()
            },
            "default_is_protected must be 0 or 1",
            "default_is_protected=2",
        );
    }

    /// TencBox: `default_per_sample_iv_size` が 7 の場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_invalid_iv_size_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_per_sample_iv_size: 7,
                ..base_tenc_box()
            },
            "default_per_sample_iv_size must be 0, 8, or 16",
            "default_per_sample_iv_size=7",
        );
    }

    /// TencBox: パターン暗号化パラメーターが片方だけの場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_one_sided_pattern_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_crypt_byte_block: Some(Uint::new(1)),
                ..base_tenc_box()
            },
            "must both be present or both be absent",
            "crypt_byte_block のみ指定",
        );
        assert_encode_invalid_input(
            &TencBox {
                default_skip_byte_block: Some(Uint::new(1)),
                ..base_tenc_box()
            },
            "must both be present or both be absent",
            "skip_byte_block のみ指定",
        );
    }

    /// TencBox: constant IV の長さが 8 / 16 以外の場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_invalid_constant_iv_size_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_per_sample_iv_size: 0,
                default_constant_iv: Some(vec![0x00; 7]),
                ..base_tenc_box()
            },
            "default_constant_iv must be 8 or 16 bytes",
            "constant IV が 7 バイト",
        );
    }

    /// TencBox: constant IV が必要なのに付いていない場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_missing_constant_iv_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_per_sample_iv_size: 0,
                ..base_tenc_box()
            },
            "default_constant_iv is required when constant IV is used",
            "default_isProtected=1, IVSize=0, constant IV なし",
        );
    }

    /// TencBox: constant IV が不要なのに付いている場合のエンコードが InvalidInput になること
    #[test]
    fn tenc_box_unexpected_constant_iv_encode_is_invalid_input() {
        assert_encode_invalid_input(
            &TencBox {
                default_constant_iv: Some(vec![0x00; 16]),
                ..base_tenc_box()
            },
            "default_constant_iv must be absent when per-sample IV is used",
            "per-sample IV なのに constant IV あり",
        );
    }
}

// ===== TencBox のデコードエラーテスト =====

mod tenc_decode_error_tests {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::TencBox};

    use super::build_box;

    /// `tenc` の生バイト列を組み立てる
    fn build_tenc_bytes(
        version: u8,
        pattern: Option<u8>,
        is_protected: u8,
        iv_size: u8,
        constant_iv: Option<&[u8]>,
        trailing: &[u8],
    ) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[version, 0, 0, 0]);
        payload.push(0); // reserved
        if version == 1 {
            payload.push(pattern.unwrap_or(0));
        } else {
            payload.push(0);
        }
        payload.push(is_protected);
        payload.push(iv_size);
        payload.extend_from_slice(&[0x11; 16]);
        if let Some(iv) = constant_iv {
            payload.push(u8::try_from(iv.len()).expect("テスト用 IV 長は u8 に収まる"));
            payload.extend_from_slice(iv);
        }
        payload.extend_from_slice(trailing);
        build_box(*b"tenc", &payload)
    }

    /// デコードが InvalidData になり、reason が指定された文言を含むことを確認する
    #[track_caller]
    fn assert_decode_invalid_data(buf: &[u8], expected_reason: &str, ctx: &str) {
        let err = TencBox::decode(buf)
            .expect_err(&format!("不正な tenc なのにデコードが成功した: {ctx}"));
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない: {ctx} (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains(expected_reason),
            "意図した検査とは別のエラーである: {ctx} (reason={})",
            err.reason,
        );
    }

    /// TencBox: version 2 のデコードが InvalidData になること
    #[test]
    fn tenc_box_version_2_decode_is_invalid_data() {
        let buf = build_tenc_bytes(2, None, 1, 8, None, &[]);
        assert_decode_invalid_data(&buf, "Unsupported TencBox version", "version 2");
    }

    /// TencBox: `default_is_protected` が 2 の場合のデコードが InvalidData になること
    #[test]
    fn tenc_box_invalid_is_protected_decode_is_invalid_data() {
        let buf = build_tenc_bytes(0, None, 2, 8, None, &[]);
        assert_decode_invalid_data(
            &buf,
            "default_is_protected must be 0 or 1",
            "default_is_protected=2",
        );
    }

    /// TencBox: `default_per_sample_iv_size` が 7 の場合のデコードが InvalidData になること
    #[test]
    fn tenc_box_invalid_iv_size_decode_is_invalid_data() {
        let buf = build_tenc_bytes(0, None, 1, 7, None, &[]);
        assert_decode_invalid_data(
            &buf,
            "default_per_sample_iv_size must be 0, 8, or 16",
            "default_per_sample_iv_size=7",
        );
    }

    /// TencBox: constant IV のサイズが 7 の場合のデコードが InvalidData になること
    #[test]
    fn tenc_box_invalid_constant_iv_size_decode_is_invalid_data() {
        let buf = build_tenc_bytes(0, None, 1, 0, Some(&[0x00; 7]), &[]);
        assert_decode_invalid_data(
            &buf,
            "default_constant_iv must be 8 or 16 bytes",
            "constant IV 7 バイト",
        );
    }

    /// TencBox: constant IV の宣言サイズより実データが短い場合のデコードが InvalidData になること
    #[test]
    fn tenc_box_short_constant_iv_decode_is_invalid_data() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&[0, 0, 0, 0]);
        payload.push(0); // reserved
        payload.push(0); // reserved
        payload.push(1); // isProtected
        payload.push(0); // Per_Sample_IV_Size
        payload.extend_from_slice(&[0x11; 16]);
        payload.push(16); // constant IV サイズは 16 と宣言
        payload.extend_from_slice(&[0x22; 8]); // 実データは 8 バイトだけ
        let buf = build_box(*b"tenc", &payload);
        assert_decode_invalid_data(
            &buf,
            "TencBox constant IV exceeds remaining payload",
            "constant IV の実データ不足",
        );
    }

    /// TencBox: 有効なボックスの後ろに余分なバイトがある場合のデコードが InvalidData になること
    #[test]
    fn tenc_box_trailing_bytes_decode_is_invalid_data() {
        let buf = build_tenc_bytes(0, None, 1, 8, None, &[0xAA]);
        assert_decode_invalid_data(&buf, "trailing bytes", "末尾バイトあり");
    }
}

// ===== FrmaBox のエラーパステスト =====

mod frma_error_tests {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::FrmaBox};

    use super::build_box;

    /// FrmaBox: ペイロード長が 4 以外の場合のデコードが InvalidData になること
    #[test]
    fn frma_box_payload_length_is_not_4_decode_is_invalid_data() {
        for payload_len in [3usize, 5] {
            let payload = vec![0xAA; payload_len];
            let buf = build_box(*b"frma", &payload);
            let err = FrmaBox::decode(&buf).expect_err(&format!(
                "ペイロード {payload_len} バイトの frma はデコードできない"
            ));
            assert_eq!(
                err.kind,
                ErrorKind::InvalidData,
                "エラー種別が InvalidData ではない: payload_len={payload_len} (reason={})",
                err.reason,
            );
            assert!(
                err.reason.contains("FrmaBox payload must be 4 bytes"),
                "ペイロード長の検査とは別のエラーである: payload_len={payload_len} (reason={})",
                err.reason,
            );
        }
    }
}

// ===== SchiBox のテスト =====

mod schi_tests {
    use shiguredo_mp4::{
        BoxSize, BoxType, Decode, Encode, Uint,
        boxes::{SchiBox, TencBox, UnknownBox},
    };

    /// SchiBox が `tenc` と未知の子ボックスを保持したまま往復できること
    #[test]
    fn schi_box_preserves_tenc_and_unknown_boxes() {
        let tenc = TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 0,
            default_kid: [0x77; 16],
            default_constant_iv: Some(vec![0x88; 16]),
            default_crypt_byte_block: Some(Uint::new(2)),
            default_skip_byte_block: Some(Uint::new(3)),
        };
        let unknown = UnknownBox {
            box_type: BoxType::Normal(*b"abcd"),
            box_size: BoxSize::with_payload_size(BoxType::Normal(*b"abcd"), 2)
                .expect("テスト用のペイロード長は総サイズが u64 に収まる"),
            payload: vec![0x11, 0x22],
        };
        let schi = SchiBox {
            tenc_box: Some(tenc),
            unknown_boxes: vec![unknown],
        };

        let encoded = schi.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SchiBox::decode(&encoded)
            .expect("直前にエンコードした有効な SchiBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.tenc_box, schi.tenc_box, "tenc が保持されていない");
        assert_eq!(
            decoded.unknown_boxes, schi.unknown_boxes,
            "unknown_boxes が保持されていない"
        );
    }
}

// ===== SchmBox のエラーパステスト =====

mod schm_error_tests {
    use shiguredo_mp4::{Decode, ErrorKind, boxes::SchmBox};

    use super::build_box;

    /// SchmBox: `scheme_uri` 以外の flags ビットが立っている場合のデコードが InvalidData になること
    #[test]
    fn schm_box_unknown_flags_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000002
        payload.extend_from_slice(&[0, 0, 0, 2]);
        payload.extend_from_slice(b"cenc");
        payload.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        let buf = build_box(*b"schm", &payload);

        let err =
            SchmBox::decode(&buf).expect_err("未知の flags を持つ SchmBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SchmBox flags"),
            "flags の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== SinfBox のエラーパステスト =====

mod sinf_tests {
    use shiguredo_mp4::{
        Decode, Encode, ErrorKind, Uint,
        boxes::{FrmaBox, SchiBox, SchmBox, SinfBox, TencBox},
    };

    use super::build_box;

    /// `frma` ボックスのエンコード済みバイト列を作る
    fn encode_frma_box() -> Vec<u8> {
        FrmaBox {
            data_format: *b"avc1",
        }
        .encode_to_vec()
        .expect("テスト用の FrmaBox はエンコードできる")
    }

    /// `schm` ボックスのエンコード済みバイト列を作る（scheme_uri なし）
    fn encode_schm_box(scheme_type: [u8; 4]) -> Vec<u8> {
        SchmBox {
            scheme_type,
            scheme_version: 0x0001_0000,
            scheme_uri: None,
        }
        .encode_to_vec()
        .expect("テスト用の SchmBox はエンコードできる")
    }

    /// スキームとの整合性を検証するための TencBox を組み立てる
    ///
    /// `pattern` を指定すると version 1 のパターン暗号化パラメーター付きになり、
    /// `None` なら version 0 になる。
    fn make_tenc_box(pattern: Option<(u8, u8)>) -> TencBox {
        let (default_crypt_byte_block, default_skip_byte_block) = match pattern {
            Some((crypt, skip)) => (Some(Uint::new(crypt)), Some(Uint::new(skip))),
            None => (None, None),
        };
        TencBox {
            default_is_protected: 1,
            default_per_sample_iv_size: 8,
            default_kid: [0x00; 16],
            default_constant_iv: None,
            default_crypt_byte_block,
            default_skip_byte_block,
        }
    }

    /// `schi` ボックスのエンコード済みバイト列を作る
    fn encode_schi_box(pattern: Option<(u8, u8)>) -> Vec<u8> {
        SchiBox {
            tenc_box: Some(make_tenc_box(pattern)),
            unknown_boxes: Vec::new(),
        }
        .encode_to_vec()
        .expect("テスト用の SchiBox はエンコードできる")
    }

    /// 子ボックスのエンコード済みバイト列を連結して `sinf` ボックスの生バイト列を作る
    fn build_sinf_box(child_boxes: &[Vec<u8>]) -> Vec<u8> {
        let mut payload = Vec::new();
        for child in child_boxes {
            payload.extend_from_slice(child);
        }
        build_box(*b"sinf", &payload)
    }

    /// スキームを保持する `SchmBox` を組み立てる
    fn make_schm_box(scheme_type: [u8; 4]) -> SchmBox {
        SchmBox {
            scheme_type,
            scheme_version: 0x0001_0000,
            scheme_uri: None,
        }
    }

    /// `schi` ボックスを組み立てる
    fn make_schi_box(tenc_box: Option<TencBox>) -> SchiBox {
        SchiBox {
            tenc_box,
            unknown_boxes: Vec::new(),
        }
    }

    /// デコードが InvalidData になり、reason が指定された文言を含むことを確認する
    #[track_caller]
    fn assert_decode_invalid_data(buf: &[u8], expected_reason: &str, ctx: &str) {
        let err = SinfBox::decode(buf)
            .expect_err(&format!("不正な sinf なのにデコードが成功した: {ctx}"));
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない: {ctx} (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains(expected_reason),
            "意図した検査とは別のエラーである: {ctx} (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `schm` だけを持ち `schi` を持たない場合のデコードが InvalidData になること
    #[test]
    fn sinf_box_with_only_schm_decode_is_invalid_data() {
        let buf = build_sinf_box(&[encode_frma_box(), encode_schm_box(*b"cenc")]);
        assert_decode_invalid_data(
            &buf,
            "schm and schi must both be present or both be absent",
            "schm のみ",
        );
    }

    /// SinfBox: `schi` だけを持ち `schm` を持たない場合のデコードが InvalidData になること
    #[test]
    fn sinf_box_with_only_schi_decode_is_invalid_data() {
        let buf = build_sinf_box(&[encode_frma_box(), encode_schi_box(None)]);
        assert_decode_invalid_data(
            &buf,
            "schm and schi must both be present or both be absent",
            "schi のみ",
        );
    }

    /// SinfBox: CENC 系スキームなのに `tenc` を持たない場合のデコードが InvalidData になること
    #[test]
    fn sinf_box_cenc_scheme_without_tenc_decode_is_invalid_data() {
        let schi = make_schi_box(None)
            .encode_to_vec()
            .expect("テスト用の SchiBox はエンコードできる");
        let buf = build_sinf_box(&[encode_frma_box(), encode_schm_box(*b"cenc"), schi]);
        assert_decode_invalid_data(
            &buf,
            "SinfBox CENC scheme requires a tenc box",
            "cenc + tenc なし",
        );
    }

    /// SinfBox: CENC 系スキームなのに `tenc` を持たない場合のエンコードが InvalidInput になること
    #[test]
    fn sinf_box_cenc_scheme_without_tenc_encode_is_invalid_input() {
        let sinf = SinfBox {
            frma_box: FrmaBox {
                data_format: *b"avc1",
            },
            schm_box: Some(make_schm_box(*b"cenc")),
            schi_box: Some(make_schi_box(None)),
            unknown_boxes: Vec::new(),
        };
        let err = sinf
            .encode_to_vec()
            .expect_err("CENC 系スキームで tenc を持たない SinfBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("SinfBox CENC scheme requires a tenc box"),
            "tenc の有無の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `cens` にパターン暗号化パラメーターを持たない `tenc` を組み合わせた
    /// デコードが InvalidData になること
    #[test]
    fn sinf_box_cens_scheme_without_pattern_tenc_decode_is_error() {
        let buf = build_sinf_box(&[
            encode_frma_box(),
            encode_schm_box(*b"cens"),
            encode_schi_box(None),
        ]);
        let err = SinfBox::decode(&buf)
            .expect_err("cens にパターン暗号化パラメーターを持たない tenc は不正");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("tenc pattern encryption parameters do not match scheme 'cens'"),
            "パターン暗号化パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `cbcs` にパターン暗号化パラメーターを持たない `tenc` を組み合わせた
    /// エンコードが InvalidInput になること
    #[test]
    fn sinf_box_cbcs_scheme_without_pattern_tenc_encode_is_invalid_input() {
        let sinf = SinfBox {
            frma_box: FrmaBox {
                data_format: *b"avc1",
            },
            schm_box: Some(make_schm_box(*b"cbcs")),
            schi_box: Some(make_schi_box(Some(make_tenc_box(None)))),
            unknown_boxes: Vec::new(),
        };
        let err = sinf
            .encode_to_vec()
            .expect_err("cbcs にパターン暗号化パラメーターを持たない tenc はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("requires tenc version 1 pattern encryption parameters"),
            "パターン暗号化パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `cenc` にパターン暗号化パラメーターを持つ `tenc` を組み合わせた
    /// デコードが InvalidData になること
    #[test]
    fn sinf_box_cenc_scheme_with_pattern_tenc_decode_is_error() {
        let buf = build_sinf_box(&[
            encode_frma_box(),
            encode_schm_box(*b"cenc"),
            encode_schi_box(Some((1, 9))),
        ]);
        let err =
            SinfBox::decode(&buf).expect_err("cenc にパターン暗号化パラメーターを持つ tenc は不正");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("tenc pattern encryption parameters do not match scheme 'cenc'"),
            "パターン暗号化パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `cenc` にパターン暗号化パラメーターを持つ `tenc` を組み合わせた
    /// エンコードが InvalidInput になること
    #[test]
    fn sinf_box_cenc_scheme_with_pattern_tenc_encode_is_invalid_input() {
        let sinf = SinfBox {
            frma_box: FrmaBox {
                data_format: *b"avc1",
            },
            schm_box: Some(make_schm_box(*b"cenc")),
            schi_box: Some(make_schi_box(Some(make_tenc_box(Some((1, 9)))))),
            unknown_boxes: Vec::new(),
        };
        let err = sinf
            .encode_to_vec()
            .expect_err("cenc にパターン暗号化パラメーターを持つ tenc はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("must not have tenc version 1 pattern encryption parameters"),
            "パターン暗号化パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SinfBox: `schm` / `schi` を持たない `sinf` の往復
    #[test]
    fn sinf_box_without_scheme_signalling_roundtrip() {
        let sinf = SinfBox {
            frma_box: FrmaBox {
                data_format: *b"avc1",
            },
            schm_box: None,
            schi_box: None,
            unknown_boxes: Vec::new(),
        };
        let encoded = sinf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SinfBox::decode(&encoded).expect("有効な sinf はデコードできる");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sinf);
    }

    /// SinfBox: CENC 以外のスキーム (`piff`) を持つ `sinf` の往復
    #[test]
    fn sinf_box_non_cenc_scheme_roundtrip() {
        let sinf = SinfBox {
            frma_box: FrmaBox {
                data_format: *b"avc1",
            },
            schm_box: Some(make_schm_box(*b"piff")),
            schi_box: Some(make_schi_box(Some(make_tenc_box(None)))),
            unknown_boxes: Vec::new(),
        };
        let encoded = sinf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SinfBox::decode(&encoded).expect("有効な sinf はデコードできる");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sinf);
    }
}

// ===== SencBox / SaizBox / SaioBox の既知バイト列でのエンコード検証 =====

mod sample_aux_info_known_bytes_tests {
    use shiguredo_mp4::{
        Encode,
        boxes::{SaioBox, SaizBox, SencBox, SencSample, Subsample},
    };

    /// SencBox: サブサンプル暗号化フラグとサンプルごとの SAI が仕様どおりに並ぶこと
    ///
    /// サブサンプルを 1 つでも持つサンプルがあれば flags の `0x000002` が立ち、
    /// サブサンプルを持たないサンプルにも `subsample_count = 0` が書かれる。
    #[test]
    fn senc_box_encode_known_bytes() {
        let senc = SencBox {
            samples: vec![
                SencSample {
                    iv: vec![0x01; 8],
                    subsamples: vec![Subsample {
                        bytes_of_clear_data: 5,
                        bytes_of_protected_data: 10,
                    }],
                },
                SencSample {
                    iv: vec![0x02; 8],
                    subsamples: Vec::new(),
                },
            ],
        };
        let encoded = senc.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (FullBox) + 4 (sample_count)
        //      + (8 (IV) + 2 (subsample_count) + 6 (subsample))
        //      + (8 (IV) + 2 (subsample_count))
        expected.extend_from_slice(&42u32.to_be_bytes());
        expected.extend_from_slice(b"senc");
        // version 0, flags 0x000002 (subsample encryption)
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&[0x01; 8]);
        expected.extend_from_slice(&1u16.to_be_bytes());
        expected.extend_from_slice(&5u16.to_be_bytes());
        expected.extend_from_slice(&10u32.to_be_bytes());
        expected.extend_from_slice(&[0x02; 8]);
        expected.extend_from_slice(&0u16.to_be_bytes());

        assert_eq!(
            encoded, expected,
            "SencBox のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// SaizBox: サンプルごとの SAI 長が `default_sample_info_size` の後ろに並ぶこと
    #[test]
    fn saiz_box_encode_known_bytes() {
        let saiz = SaizBox {
            aux_info_type: None,
            aux_info_type_parameter: 0,
            default_sample_info_size: 0,
            sample_count: 2,
            sample_info_sizes: vec![0x10, 0x00],
        };
        let encoded = saiz.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (FullBox) + 1 (default_sample_info_size)
        //      + 4 (sample_count) + 2 (sample_info_size)
        expected.extend_from_slice(&19u32.to_be_bytes());
        expected.extend_from_slice(b"saiz");
        // version 0, flags 0 (aux_info_type なし)
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        expected.push(0);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&[0x10, 0x00]);

        assert_eq!(
            encoded, expected,
            "SaizBox のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// SaioBox: version 0 の 32 bit offset が仕様どおりに並ぶこと
    #[test]
    fn saio_box_version_0_encode_known_bytes() {
        let saio = SaioBox {
            version: 0,
            aux_info_type: None,
            aux_info_type_parameter: 0,
            offsets: vec![0x10, 0x20],
        };
        let encoded = saio.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 + 4 (entry_count) + 4 * 2 (offset)
        expected.extend_from_slice(&24u32.to_be_bytes());
        expected.extend_from_slice(b"saio");
        // version 0, flags 0
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&0x10u32.to_be_bytes());
        expected.extend_from_slice(&0x20u32.to_be_bytes());

        assert_eq!(
            encoded, expected,
            "SaioBox version 0 のエンコード結果が既知のバイト列と一致しない"
        );
    }

    /// SaioBox: version 1 の 64 bit offset が仕様どおりに並ぶこと
    #[test]
    fn saio_box_version_1_encode_known_bytes() {
        let offset = u64::from(u32::MAX) + 1;
        let saio = SaioBox {
            version: 1,
            aux_info_type: None,
            aux_info_type_parameter: 0,
            offsets: vec![offset],
        };
        let encoded = saio.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 + 4 (entry_count) + 8 (offset)
        expected.extend_from_slice(&24u32.to_be_bytes());
        expected.extend_from_slice(b"saio");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&offset.to_be_bytes());

        assert_eq!(
            encoded, expected,
            "SaioBox version 1 のエンコード結果が既知のバイト列と一致しない"
        );
    }
}

// ===== SencBox のエラーパステスト =====

mod senc_error_tests {
    use shiguredo_mp4::{
        Decode, Encode, ErrorKind,
        boxes::{SencBox, SencSample, Subsample},
    };

    use super::build_box;

    /// SencBox: `Decode` は IV サイズを決められないため常に Unsupported になること
    #[test]
    fn senc_box_decode_is_unsupported() {
        let senc = SencBox {
            samples: vec![SencSample {
                iv: vec![0x11; 8],
                subsamples: Vec::new(),
            }],
        };
        let encoded = senc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let err = SencBox::decode(&encoded)
            .expect_err("IV サイズなしで SencBox をデコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "エラー種別が Unsupported ではない (reason={})",
            err.reason,
        );
    }

    /// SencBox: version 1 のデコードが InvalidData になること
    #[test]
    fn senc_box_version_1_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 1, flags 0
        payload.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 8)
            .expect_err("version 1 の SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SencBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: 未知の flags を持つ入力のデコードが InvalidData になること
    #[test]
    fn senc_box_unknown_flags_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000001 (サブサンプル暗号化以外のビット)
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 8)
            .expect_err("未知の flags を持つ SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SencBox flags"),
            "flags の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: IV サイズが 0 / 8 / 16 以外の場合のデコードが InvalidData になること
    #[test]
    fn senc_box_invalid_iv_size_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 7)
            .expect_err("IV サイズ 7 の SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("per-sample IV size must be 0, 8, or 16"),
            "IV サイズの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: サンプルの IV がペイロード末尾を超える入力のデコードが InvalidData になること
    #[test]
    fn senc_box_truncated_iv_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&1u32.to_be_bytes());
        // IV サイズは 8 だが、実データは 4 バイトだけ
        payload.extend_from_slice(&[0x11; 4]);
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 8)
            .expect_err("IV が途中で切れた SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("sample count exceeds available payload"),
            "サンプル数の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: サブサンプル表が切り詰められた入力のデコードが InvalidData になること
    ///
    /// `subsample_count` が残りのペイロードより大きい場合、個々のサブサンプルを
    /// 読みながら判定すると `InsufficientBuffer` が返ってしまう。doc の契約どおり
    /// `InvalidData` を返すことを固定する。
    #[test]
    fn senc_box_truncated_subsample_table_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000002 (サブサンプル暗号化)
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        payload.extend_from_slice(&1u32.to_be_bytes());
        // IV 8 バイト
        payload.extend_from_slice(&[0x11; 8]);
        // subsample_count は 2 だが、サブサンプルは 1 個分 (6 バイト) しかない
        payload.extend_from_slice(&2u16.to_be_bytes());
        payload.extend_from_slice(&[0x00, 0x05, 0x00, 0x00, 0x00, 0x0A]);
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 8)
            .expect_err("サブサンプル表が切り詰められた SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("subsample table exceeds available payload"),
            "サブサンプル表の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: 末尾に余分なバイトがある入力のデコードが InvalidData になること
    #[test]
    fn senc_box_trailing_bytes_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.extend_from_slice(&[0x11; 8]);
        payload.push(0xFF);
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 8)
            .expect_err("末尾に余分なバイトがある SencBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("trailing bytes"),
            "末尾バイトの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: サンプルごとに IV サイズが異なる場合のエンコードが InvalidInput になること
    #[test]
    fn senc_box_mixed_iv_sizes_encode_is_invalid_input() {
        let senc = SencBox {
            samples: vec![
                SencSample {
                    iv: vec![0x11; 8],
                    subsamples: Vec::new(),
                },
                SencSample {
                    iv: vec![0x22; 16],
                    subsamples: Vec::new(),
                },
            ],
        };
        let err = senc
            .encode_to_vec()
            .expect_err("IV サイズの異なるサンプルを持つ SencBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("samples must all have the same IV size"),
            "IV サイズの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: IV サイズが 0 / 8 / 16 以外の場合のエンコードが InvalidInput になること
    #[test]
    fn senc_box_invalid_iv_size_encode_is_invalid_input() {
        let senc = SencBox {
            samples: vec![SencSample {
                iv: vec![0x11; 4],
                subsamples: Vec::new(),
            }],
        };
        let err = senc
            .encode_to_vec()
            .expect_err("IV サイズ 4 の SencBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("IV size must be 0, 8, or 16"),
            "IV サイズの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SencBox: サブサンプル数が u16 を超える場合のエンコードが InvalidInput になること
    #[test]
    fn senc_box_too_many_subsamples_encode_is_invalid_input() {
        let subsamples = vec![
            Subsample {
                bytes_of_clear_data: 1,
                bytes_of_protected_data: 1,
            };
            usize::from(u16::MAX) + 1
        ];
        let senc = SencBox {
            samples: vec![SencSample {
                iv: vec![0x11; 8],
                subsamples,
            }],
        };
        let err = senc
            .encode_to_vec()
            .expect_err("サブサンプル数が u16 を超える SencBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("subsample count exceeds u16::MAX"),
            "サブサンプル数の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== SaizBox のエラーパステスト =====

mod saiz_error_tests {
    use shiguredo_mp4::{Decode, Encode, ErrorKind, boxes::SaizBox};

    use super::build_box;

    /// 有効な SaizBox を返す
    fn valid_saiz_box() -> SaizBox {
        SaizBox {
            aux_info_type: Some(*b"cenc"),
            aux_info_type_parameter: 0,
            default_sample_info_size: 0,
            sample_count: 1,
            sample_info_sizes: vec![16],
        }
    }

    /// SaizBox: version 1 のデコードが InvalidData になること
    #[test]
    fn saiz_box_version_1_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 1, flags 0
        payload.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        payload.push(0);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saiz", &payload);

        let err = SaizBox::decode(&buf).expect_err("version 1 の SaizBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SaizBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: 未知の flags を持つ入力のデコードが InvalidData になること
    #[test]
    fn saiz_box_unknown_flags_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000002
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        payload.push(0);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saiz", &payload);

        let err =
            SaizBox::decode(&buf).expect_err("未知の flags を持つ SaizBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SaizBox flags"),
            "flags の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: `aux_info_type_parameter` が 0 以外の入力のデコードが InvalidData になること
    #[test]
    fn saiz_box_non_zero_parameter_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000001 (aux_info_type あり)
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        payload.extend_from_slice(b"cenc");
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.push(0);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saiz", &payload);

        let err = SaizBox::decode(&buf)
            .expect_err("aux_info_type_parameter が 0 以外の SaizBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("aux_info_type_parameter must be 0"),
            "パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: `aux_info_type_parameter` が 0 以外の場合のエンコードが InvalidInput になること
    #[test]
    fn saiz_box_non_zero_parameter_encode_is_invalid_input() {
        let saiz = SaizBox {
            aux_info_type_parameter: 1,
            ..valid_saiz_box()
        };
        let err = saiz
            .encode_to_vec()
            .expect_err("aux_info_type_parameter が 0 以外の SaizBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("aux_info_type_parameter must be 0"),
            "パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: `sample_info_sizes` の長さが `sample_count` と異なる場合のエンコードが
    /// InvalidInput になること
    #[test]
    fn saiz_box_size_count_mismatch_encode_is_invalid_input() {
        let saiz = SaizBox {
            sample_count: 2,
            sample_info_sizes: vec![16],
            ..valid_saiz_box()
        };
        let err = saiz.encode_to_vec().expect_err(
            "sample_info_sizes の長さが sample_count と異なる SaizBox はエンコードできない",
        );
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("sample_info_sizes length must equal sample_count"),
            "サンプル数の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: `default_sample_info_size` が 0 以外なのに `sample_info_sizes` を持つ場合の
    /// エンコードが InvalidInput になること
    #[test]
    fn saiz_box_unexpected_sizes_encode_is_invalid_input() {
        let saiz = SaizBox {
            default_sample_info_size: 16,
            sample_info_sizes: vec![16],
            ..valid_saiz_box()
        };
        let err = saiz.encode_to_vec().expect_err(
            "default_sample_info_size と sample_info_sizes を併用した SaizBox はエンコードできない",
        );
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains(
                "sample_info_sizes must be empty when default_sample_info_size is non-zero"
            ),
            "サイズ一覧の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaizBox: 末尾に余分なバイトがある入力のデコードが InvalidData になること
    #[test]
    fn saiz_box_trailing_bytes_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.push(0);
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.push(0xFF);
        let buf = build_box(*b"saiz", &payload);

        let err =
            SaizBox::decode(&buf).expect_err("末尾に余分なバイトがある SaizBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("trailing bytes"),
            "末尾バイトの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== SaioBox のエラーパステスト =====

mod saio_error_tests {
    use shiguredo_mp4::{Decode, Encode, ErrorKind, boxes::SaioBox};

    use super::build_box;

    /// 有効な SaioBox を返す
    fn valid_saio_box() -> SaioBox {
        SaioBox {
            version: 0,
            aux_info_type: Some(*b"cenc"),
            aux_info_type_parameter: 0,
            offsets: vec![16],
        }
    }

    /// SaioBox: version 2 のデコードが InvalidData になること
    #[test]
    fn saio_box_version_2_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 2, flags 0
        payload.extend_from_slice(&[0x02, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saio", &payload);

        let err = SaioBox::decode(&buf).expect_err("version 2 の SaioBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SaioBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: version 2 のエンコードが InvalidInput になること
    #[test]
    fn saio_box_version_2_encode_is_invalid_input() {
        let saio = SaioBox {
            version: 2,
            ..valid_saio_box()
        };
        let err = saio
            .encode_to_vec()
            .expect_err("version 2 の SaioBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("version must be 0 or 1"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: `aux_info_type_parameter` が 0 以外の入力のデコードが InvalidData になること
    #[test]
    fn saio_box_non_zero_parameter_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000001 (aux_info_type あり)
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]);
        payload.extend_from_slice(b"cenc");
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saio", &payload);

        let err = SaioBox::decode(&buf)
            .expect_err("aux_info_type_parameter が 0 以外の SaioBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("aux_info_type_parameter must be 0"),
            "パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: `aux_info_type_parameter` が 0 以外の場合のエンコードが InvalidInput になること
    #[test]
    fn saio_box_non_zero_parameter_encode_is_invalid_input() {
        let saio = SaioBox {
            aux_info_type_parameter: 1,
            ..valid_saio_box()
        };
        let err = saio
            .encode_to_vec()
            .expect_err("aux_info_type_parameter が 0 以外の SaioBox はエンコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("aux_info_type_parameter must be 0"),
            "パラメーターの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: version 0 で `u32::MAX` を超える offset を持つ場合のエンコードが
    /// InvalidInput になること
    #[test]
    fn saio_box_version_0_offset_overflow_encode_is_invalid_input() {
        let saio = SaioBox {
            version: 0,
            offsets: vec![u64::from(u32::MAX) + 1],
            ..valid_saio_box()
        };
        let err = saio.encode_to_vec().expect_err(
            "version 0 で u32::MAX を超える offset を持つ SaioBox はエンコードできない",
        );
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("version 0 offset exceeds u32::MAX"),
            "offset の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: 未知の flags を持つ入力のデコードが InvalidData になること
    #[test]
    fn saio_box_unknown_flags_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0x000002
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x02]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"saio", &payload);

        let err =
            SaioBox::decode(&buf).expect_err("未知の flags を持つ SaioBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("Unsupported SaioBox flags"),
            "flags の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SaioBox: 末尾に余分なバイトがある入力のデコードが InvalidData になること
    #[test]
    fn saio_box_trailing_bytes_decode_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.push(0xFF);
        let buf = build_box(*b"saio", &payload);

        let err =
            SaioBox::decode(&buf).expect_err("末尾に余分なバイトがある SaioBox はデコードできない");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("trailing bytes"),
            "末尾バイトの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== SencBox のサンプル数上限テスト =====

mod senc_sample_count_limit_tests {
    use shiguredo_mp4::{ErrorKind, boxes::SencBox};

    use super::build_box;

    /// SencBox: 各サンプルが SAI を持たない場合に、`sample_count` が上限を超えると
    /// InvalidData になること
    ///
    /// IV サイズ 0 かつサブサンプル暗号化でない場合、各サンプルはペイロードを持たないため、
    /// `sample_count` は入力サイズから制約されない。上限がないと、小さな入力で巨大な
    /// `Vec` を確保できてしまう。
    #[test]
    fn senc_box_excessive_sample_count_without_aux_info_is_invalid_data() {
        let mut payload = Vec::new();
        // version 0, flags 0
        payload.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        // 上限 (1,048,576) を超えるサンプル数を宣言する
        payload.extend_from_slice(&(1_048_577u32).to_be_bytes());
        let buf = build_box(*b"senc", &payload);

        let err = SencBox::decode_with_iv_size(&buf, 0)
            .expect_err("各サンプルが SAI を持たないのに巨大な sample_count を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains(
                "sample count exceeds maximum allowed when samples have no auxiliary information"
            ),
            "サンプル数の上限の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}

// ===== `seig` / `sbgp` / `sgpd` の既知バイト列でのエンコード検証 =====

mod sample_group_known_bytes_tests {
    use shiguredo_mp4::{
        Decode, Encode, Uint,
        boxes::{SbgpBox, SbgpEntry, SeigEntry, SgpdBox, SgpdEntry},
    };

    /// テスト用の KID
    const KID: [u8; 16] = [0x11; 16];

    /// `seig` エントリーの固定前置を組み立てる
    ///
    /// 20 バイトの固定前置は、`multi_key_flag` 1 bit + reserved 7 bit、
    /// `crypt_byte_block` 4 bit + `skip_byte_block` 4 bit、`is_protected` 8 bit、
    /// `Per_Sample_IV_Size` 8 bit、KID 16 バイトの順に並ぶ。
    fn seig_fixed_prefix(
        crypt_byte_block: u8,
        skip_byte_block: u8,
        is_protected: u8,
        per_sample_iv_size: u8,
        kid: [u8; 16],
    ) -> Vec<u8> {
        let mut bytes = vec![
            0x00,
            (crypt_byte_block << 4) | skip_byte_block,
            is_protected,
            per_sample_iv_size,
        ];
        bytes.extend_from_slice(&kid);
        bytes
    }

    /// SeigEntry: 20 バイトの固定前置が仕様どおりにエンコードされ、デコードできること
    ///
    /// `crypt_byte_block` / `skip_byte_block` は 4 bit ずつ 2 バイト目にパックされる。
    #[test]
    fn seig_entry_fixed_prefix_known_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(1),
            skip_byte_block: Uint::new(9),
            is_protected: 1,
            per_sample_iv_size: 16,
            kid: KID,
            constant_iv: None,
        };
        let encoded = entry.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let expected = seig_fixed_prefix(1, 9, 1, 16, KID);
        assert_eq!(
            encoded, expected,
            "SeigEntry の固定前置のエンコード結果が既知のバイト列と一致しない"
        );
        assert_eq!(encoded.len(), SeigEntry::FIXED_SIZE);

        let (decoded, consumed) =
            SeigEntry::decode(&encoded).expect("直前にエンコードした SeigEntry をデコードできない");
        assert_eq!(consumed, SeigEntry::FIXED_SIZE);
        assert_eq!(decoded, entry);
    }

    /// SeigEntry: 定義済みフィールドの後ろに追加バイトがあっても無視すること
    ///
    /// 仕様では将来の拡張を読み飛ばす必要があるため、消費バイト数は固定前置の
    /// 20 バイトのままになる。
    #[test]
    fn seig_entry_ignores_trailing_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 8,
            kid: KID,
            constant_iv: None,
        };
        let mut payload = entry.encode_to_vec().expect("Vec への書き込みは失敗しない");
        payload.extend_from_slice(&[0xFF, 0xEE, 0xDD]);

        let (decoded, consumed) = SeigEntry::decode(&payload)
            .expect("追加バイトを持つ SeigEntry の定義済みフィールドをデコードできない");
        assert_eq!(
            consumed,
            SeigEntry::FIXED_SIZE,
            "追加バイトを消費してしまった"
        );
        assert_eq!(decoded, entry);
    }

    /// SeigEntry: constant IV が固定前置の後ろに続くこと
    #[test]
    fn seig_entry_constant_iv_known_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 0,
            kid: KID,
            constant_iv: Some(vec![0xAA; 8]),
        };
        let encoded = entry.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = seig_fixed_prefix(0, 0, 1, 0, KID);
        expected.push(8);
        expected.extend_from_slice(&[0xAA; 8]);
        // 固定前置 20 バイト + constant_IV_size 1 バイト + constant IV 8 バイト
        assert_eq!(encoded.len(), 29);
        assert_eq!(
            encoded, expected,
            "SeigEntry の constant IV のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, consumed) =
            SeigEntry::decode(&encoded).expect("constant IV を持つ SeigEntry をデコードできない");
        assert_eq!(consumed, 29);
        assert_eq!(decoded, entry);
    }

    /// SgpdBox (version 0): `grouping_type` と `entry_count` の後ろに `seig` エントリーが並ぶこと
    #[test]
    fn sgpd_box_version_0_known_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 16,
            kid: KID,
            constant_iv: None,
        };
        let sgpd = SgpdBox {
            version: 0,
            grouping_type: *b"seig",
            default_length: None,
            entries: vec![SgpdEntry::Seig(entry.clone())],
        };
        let encoded = sgpd.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 (ヘッダー) + 4 (FullBox) + 4 (grouping_type) + 4 (entry_count) + 20 (entry)
        expected.extend_from_slice(&40u32.to_be_bytes());
        expected.extend_from_slice(b"sgpd");
        // version 0, flags 0
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(b"seig");
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&seig_fixed_prefix(0, 0, 1, 16, KID));
        assert_eq!(
            encoded, expected,
            "SgpdBox version 0 のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) =
            SgpdBox::decode(&encoded).expect("SgpdBox version 0 をデコードできない");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sgpd);
    }

    /// SgpdBox (version 1, `default_length` あり): エントリーの前に長さが入らないこと
    #[test]
    fn sgpd_box_version_1_fixed_length_known_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 8,
            kid: KID,
            constant_iv: None,
        };
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(20),
            entries: vec![SgpdEntry::Seig(entry.clone())],
        };
        let encoded = sgpd.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 (FullBox) + 4 (grouping_type) + 4 (default_length) + 4 (entry_count) + 20
        expected.extend_from_slice(&44u32.to_be_bytes());
        expected.extend_from_slice(b"sgpd");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(b"seig");
        expected.extend_from_slice(&20u32.to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&seig_fixed_prefix(0, 0, 1, 8, KID));
        assert_eq!(
            encoded, expected,
            "SgpdBox version 1 (default_length あり) のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) = SgpdBox::decode(&encoded)
            .expect("SgpdBox version 1 (default_length あり) をデコードできない");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sgpd);
    }

    /// SgpdBox (version 1, `default_length == 0`):
    /// 各エントリーの前に `description_length` が入ること
    #[test]
    fn sgpd_box_version_1_variable_length_known_bytes() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 0,
            kid: KID,
            constant_iv: Some(vec![0xAA; 8]),
        };
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(entry.clone())],
        };
        let encoded = sgpd.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 + 4 + 4 + 4 (entry_count) + 4 (description_length) + 29 (entry)
        expected.extend_from_slice(&57u32.to_be_bytes());
        expected.extend_from_slice(b"sgpd");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(b"seig");
        expected.extend_from_slice(&0u32.to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&29u32.to_be_bytes());
        expected.extend_from_slice(&seig_fixed_prefix(0, 0, 1, 0, KID));
        expected.push(8);
        expected.extend_from_slice(&[0xAA; 8]);
        assert_eq!(
            encoded, expected,
            "SgpdBox version 1 (default_length 0) のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) = SgpdBox::decode(&encoded)
            .expect("SgpdBox version 1 (default_length 0) をデコードできない");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sgpd);
    }

    /// SbgpBox (version 0): サンプル数と group_description_index が並ぶこと
    #[test]
    fn sbgp_box_version_0_known_bytes() {
        let sbgp = SbgpBox {
            version: 0,
            grouping_type: *b"seig",
            grouping_type_parameter: None,
            entries: vec![
                SbgpEntry {
                    sample_count: 3,
                    group_description_index: 0,
                },
                SbgpEntry {
                    sample_count: 2,
                    group_description_index: 0x1_0001,
                },
            ],
        };
        let encoded = sbgp.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 (FullBox) + 4 (grouping_type) + 4 (entry_count) + 8 * 2
        expected.extend_from_slice(&36u32.to_be_bytes());
        expected.extend_from_slice(b"sbgp");
        // version 0, flags 0
        expected.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(b"seig");
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&3u32.to_be_bytes());
        expected.extend_from_slice(&0u32.to_be_bytes());
        expected.extend_from_slice(&2u32.to_be_bytes());
        expected.extend_from_slice(&0x1_0001u32.to_be_bytes());
        assert_eq!(
            encoded, expected,
            "SbgpBox version 0 のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) =
            SbgpBox::decode(&encoded).expect("SbgpBox version 0 をデコードできない");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sbgp);
    }

    /// SbgpBox (version 1): `grouping_type_parameter` が grouping_type の後ろに入ること
    #[test]
    fn sbgp_box_version_1_known_bytes() {
        let sbgp = SbgpBox {
            version: 1,
            grouping_type: *b"seig",
            grouping_type_parameter: Some(0x0000_0005),
            entries: vec![SbgpEntry {
                sample_count: 1,
                group_description_index: 0x1_0002,
            }],
        };
        let encoded = sbgp.encode_to_vec().expect("Vec への書き込みは失敗しない");

        let mut expected = Vec::new();
        // size = 8 + 4 (FullBox) + 4 (grouping_type) + 4 (parameter)
        //      + 4 (entry_count) + 8 (entry)
        expected.extend_from_slice(&32u32.to_be_bytes());
        expected.extend_from_slice(b"sbgp");
        // version 1, flags 0
        expected.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        expected.extend_from_slice(b"seig");
        expected.extend_from_slice(&5u32.to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&0x1_0002u32.to_be_bytes());
        assert_eq!(
            encoded, expected,
            "SbgpBox version 1 のエンコード結果が既知のバイト列と一致しない"
        );

        let (decoded, size) =
            SbgpBox::decode(&encoded).expect("SbgpBox version 1 をデコードできない");
        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sbgp);
    }
}

// ===== `seig` / `sbgp` / `sgpd` のエラーパステスト =====

mod sample_group_error_tests {
    use shiguredo_mp4::{
        Decode, Encode, ErrorKind, Uint,
        boxes::{SbgpBox, SbgpEntry, SeigEntry, SgpdBox, SgpdEntry},
    };

    use super::build_box;

    /// テスト用の KID
    const KID: [u8; 16] = [0x11; 16];

    /// `seig` エントリーの 20 バイトの固定前置を組み立てる
    fn seig_fixed_prefix(
        first: u8,
        second: u8,
        is_protected: u8,
        per_sample_iv_size: u8,
    ) -> Vec<u8> {
        let mut bytes = vec![first, second, is_protected, per_sample_iv_size];
        bytes.extend_from_slice(&KID);
        bytes
    }

    /// SeigEntry: `multi_key_flag == 1` のデコードが Unsupported になること
    #[test]
    fn seig_entry_multi_key_flag_is_unsupported() {
        let payload = seig_fixed_prefix(0x80, 0x00, 1, 16);
        let err = SeigEntry::decode(&payload).expect_err("multi_key_flag == 1 を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "エラー種別が Unsupported ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("multi_key_flag"),
            "multi_key_flag の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: 4 bit を超えるパターン値のエンコードが InvalidInput になること
    ///
    /// `Uint::new()` は値を検証しないため 16 以上を設定できる。切り詰めて書くと
    /// 読み戻したときに別の値になるので、エンコードで拒否する。
    #[test]
    fn seig_entry_encode_with_pattern_exceeding_four_bits_is_invalid_input() {
        for (crypt_byte_block, skip_byte_block) in [(16u8, 0u8), (0, 16)] {
            let entry = SeigEntry {
                crypt_byte_block: Uint::new(crypt_byte_block),
                skip_byte_block: Uint::new(skip_byte_block),
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: KID,
                constant_iv: None,
            };
            let err = entry
                .encode_to_vec()
                .expect_err("4 bit を超えるパターン値をエンコードしてしまった");
            assert_eq!(
                err.kind,
                ErrorKind::InvalidInput,
                "エラー種別が InvalidInput ではない (reason={})",
                err.reason,
            );
            assert!(
                err.reason.contains("4 bits"),
                "パターン値の検査とは別のエラーである (reason={})",
                err.reason,
            );
        }
    }

    /// SeigEntry: constant IV の長さが `u8::MAX` を超える場合に InvalidInput になること
    #[test]
    fn seig_entry_encode_with_too_large_constant_iv_is_invalid_input() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(1),
            skip_byte_block: Uint::new(9),
            is_protected: 1,
            per_sample_iv_size: 0,
            kid: KID,
            constant_iv: Some(vec![0x77; 256]),
        };
        let err = entry
            .encode_to_vec()
            .expect_err("u8 に収まらない constant IV をエンコードしてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: `crypt_byte_block` / `skip_byte_block` が 4 bit の値として
    /// デコードされること
    #[test]
    fn seig_entry_pattern_values_are_decoded() {
        for (second, crypt_byte_block, skip_byte_block) in
            [(0x19, 1, 9), (0xF0, 0x0F, 0), (0x0F, 0, 0x0F)]
        {
            let payload = seig_fixed_prefix(0x00, second, 1, 16);
            let (entry, _) =
                SeigEntry::decode(&payload).expect("パターン値を持つ seig をデコードできない");
            assert_eq!(
                entry.crypt_byte_block.get(),
                crypt_byte_block,
                "crypt_byte_block が 2 バイト目の上位 4 bit から復元されていない"
            );
            assert_eq!(
                entry.skip_byte_block.get(),
                skip_byte_block,
                "skip_byte_block が 2 バイト目の下位 4 bit から復元されていない"
            );
        }
    }

    /// SeigEntry: constant IV の長さが 8 / 16 以外のデコードが InvalidData になること
    #[test]
    fn seig_entry_invalid_constant_iv_size_is_invalid_data() {
        // constant_IV_size = 12 は仕様上あり得ない長さ
        let mut payload = seig_fixed_prefix(0x00, 0x00, 1, 0);
        payload.push(12);
        payload.extend_from_slice(&[0xAA; 12]);
        let err = SeigEntry::decode(&payload)
            .expect_err("constant IV の長さが 12 バイトの SeigEntry を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("constant_IV_size must be 8 or 16"),
            "constant IV の長さの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: `is_protected == 1 && per_sample_iv_size == 0` なのに
    /// constant IV が無いデコードが InvalidData になること
    #[test]
    fn seig_entry_missing_constant_iv_is_invalid_data() {
        let payload = seig_fixed_prefix(0x00, 0x00, 1, 0);
        let err = SeigEntry::decode(&payload)
            .expect_err("constant IV が無い SeigEntry を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("constant_IV_size is missing"),
            "constant IV の必須検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: `is_protected` が 0 / 1 以外のデコードが InvalidData になること
    #[test]
    fn seig_entry_invalid_is_protected_is_invalid_data() {
        let payload = seig_fixed_prefix(0x00, 0x00, 2, 16);
        let err = SeigEntry::decode(&payload).expect_err("is_protected == 2 を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("is_protected"),
            "is_protected の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: `Per_Sample_IV_Size` が 0 / 8 / 16 以外のデコードが InvalidData になること
    #[test]
    fn seig_entry_invalid_per_sample_iv_size_is_invalid_data() {
        let payload = seig_fixed_prefix(0x00, 0x00, 1, 12);
        let err =
            SeigEntry::decode(&payload).expect_err("Per_Sample_IV_Size == 12 を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("per_sample_iv_size"),
            "IV サイズの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: 固定前置が切り詰められたデコードが InvalidData になること
    #[test]
    fn seig_entry_truncated_prefix_is_invalid_data() {
        let payload = vec![0x00; SeigEntry::FIXED_SIZE - 1];
        let err =
            SeigEntry::decode(&payload).expect_err("20 バイト未満の SeigEntry を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
    }

    /// SeigEntry: constant IV が切り詰められたデコードが InvalidData になること
    #[test]
    fn seig_entry_truncated_constant_iv_is_invalid_data() {
        let mut payload = seig_fixed_prefix(0x00, 0x00, 1, 0);
        // constant_IV_size = 8 だが IV のバイト列がない
        payload.push(8);
        let err = SeigEntry::decode(&payload)
            .expect_err("constant IV が切り詰められた SeigEntry を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `seig` エントリーの `crypt_byte_block` が 4 bit を超える場合の
    /// エンコードが InvalidInput になること
    ///
    /// `Uint::new()` は範囲を検証しないため、エンコード側で `InvalidInput` にする。
    #[test]
    fn sgpd_box_seig_crypt_byte_block_exceeding_four_bits_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(SeigEntry {
                crypt_byte_block: Uint::new(0x10),
                skip_byte_block: Uint::new(0),
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: KID,
                constant_iv: None,
            })],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("crypt_byte_block が 4 bit を超える seig をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("crypt_byte_block exceeds 4 bits"),
            "crypt_byte_block の範囲の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `seig` エントリーの `skip_byte_block` が 4 bit を超える場合の
    /// エンコードが InvalidInput になること
    #[test]
    fn sgpd_box_seig_skip_byte_block_exceeding_four_bits_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(SeigEntry {
                crypt_byte_block: Uint::new(0),
                skip_byte_block: Uint::new(0x10),
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: KID,
                constant_iv: None,
            })],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("skip_byte_block が 4 bit を超える seig をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("skip_byte_block exceeds 4 bits"),
            "skip_byte_block の範囲の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `seig` エントリーの constant IV の長さが 8 / 16 以外の場合の
    /// エンコードが InvalidInput になること
    #[test]
    fn sgpd_box_seig_invalid_constant_iv_size_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(SeigEntry {
                crypt_byte_block: Uint::new(0),
                skip_byte_block: Uint::new(0),
                is_protected: 1,
                per_sample_iv_size: 0,
                kid: KID,
                constant_iv: Some(vec![0xAA; 12]),
            })],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("constant IV の長さが 12 バイトの seig をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("constant_iv must be 8 or 16 bytes"),
            "constant IV の長さの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `is_protected == 1 && per_sample_iv_size == 0` なのに constant IV が
    /// 無い `seig` エントリーのエンコードが InvalidInput になること
    #[test]
    fn sgpd_box_seig_missing_constant_iv_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(SeigEntry {
                crypt_byte_block: Uint::new(0),
                skip_byte_block: Uint::new(0),
                is_protected: 1,
                per_sample_iv_size: 0,
                kid: KID,
                constant_iv: None,
            })],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("constant IV が無い seig をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("constant_iv is required"),
            "constant IV の必須検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: version 2 のデコードが Unsupported になること
    #[test]
    fn sgpd_box_version_2_decode_is_unsupported() {
        // version 2, flags 0 だけで、デコードはバージョン検査で拒否する
        let buf = build_box(*b"sgpd", &[0x02, 0x00, 0x00, 0x00]);
        let err = SgpdBox::decode(&buf).expect_err("SgpdBox version 2 を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "エラー種別が Unsupported ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("SgpdBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: version 0 で `grouping_type` が `seig` 以外のデコードが Unsupported に
    /// なること
    ///
    /// version 0 は `default_length` を持たないため、エントリー長を決められない。
    #[test]
    fn sgpd_box_version_0_non_seig_decode_is_unsupported() {
        let mut payload = vec![0x00, 0x00, 0x00, 0x00];
        payload.extend_from_slice(b"roll");
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"sgpd", &payload);
        let err =
            SgpdBox::decode(&buf).expect_err("version 0 で 'roll' の SgpdBox を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "エラー種別が Unsupported ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("grouping type other than 'seig'"),
            "grouping_type の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: version 2 のエンコードが Unsupported になること
    #[test]
    fn sgpd_box_version_2_encode_is_unsupported() {
        let sgpd = SgpdBox {
            version: 2,
            grouping_type: *b"seig",
            default_length: None,
            entries: Vec::new(),
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("SgpdBox version 2 をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::Unsupported,
            "エラー種別が Unsupported ではない (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `default_length` がエントリー長と一致しないエンコードが InvalidInput に
    /// なること
    #[test]
    fn sgpd_box_default_length_mismatch_encode_is_invalid_input() {
        let entry = SeigEntry {
            crypt_byte_block: Uint::new(0),
            skip_byte_block: Uint::new(0),
            is_protected: 1,
            per_sample_iv_size: 16,
            kid: KID,
            constant_iv: None,
        };
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(24),
            entries: vec![SgpdEntry::Seig(entry)],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("default_length と一致しない長さのエントリーをエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("default_length"),
            "default_length の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: エントリーが切り詰められたデコードが InvalidData になること
    #[test]
    fn sgpd_box_truncated_entries_is_invalid_data() {
        // version 1, flags 0, grouping_type 'seig', default_length 20,
        // entry_count 2 だが 1 エントリー分 (20 バイト) しかない
        let mut payload = vec![0x01, 0x00, 0x00, 0x00];
        payload.extend_from_slice(b"seig");
        payload.extend_from_slice(&20u32.to_be_bytes());
        payload.extend_from_slice(&2u32.to_be_bytes());
        payload.extend_from_slice(&seig_fixed_prefix(0x00, 0x00, 1, 16));
        let buf = build_box(*b"sgpd", &payload);

        let err = SgpdBox::decode(&buf).expect_err("切り詰められた sgpd を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("truncated"),
            "切り詰めの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: version 0 で `seig` 以外のエントリーを指定したエンコードが InvalidInput に
    /// なること
    ///
    /// `grouping_type` が `seig` なので `SgpdEntry::Seig` だけを許可する。
    #[test]
    fn sgpd_box_version_0_unknown_entry_encode_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 0,
            grouping_type: *b"seig",
            default_length: None,
            entries: vec![SgpdEntry::Unknown(vec![0x00; 20])],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("version 0 で Unknown エントリーをエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("requires 'seig' entries"),
            "grouping_type とエントリーの整合の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `grouping_type` が `seig` 以外なのに `SgpdEntry::Seig` を持つ
    /// version 1 のエンコードが InvalidInput になること
    ///
    /// デコードは `grouping_type` だけでエントリーの variant を決めるため、
    /// 不一致のまま書き出すとエンコードとデコードの往復契約が壊れる。
    #[test]
    fn sgpd_box_non_seig_grouping_type_with_seig_entry_encode_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"roll",
            default_length: Some(0),
            entries: vec![SgpdEntry::Seig(SeigEntry {
                crypt_byte_block: Uint::new(0),
                skip_byte_block: Uint::new(0),
                is_protected: 1,
                per_sample_iv_size: 16,
                kid: KID,
                constant_iv: None,
            })],
        };
        let err = sgpd
            .encode_to_vec()
            .expect_err("grouping_type が 'roll' なのに Seig エントリーをエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason
                .contains("'seig' entries require grouping_type 'seig'"),
            "grouping_type とエントリーの整合の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SgpdBox: `grouping_type` が `seig` なのに `SgpdEntry::Unknown` を持つ
    /// version 1 のエンコードが InvalidInput になること
    ///
    /// `seig` のエントリーは `SeigEntry` として解釈できる必要があるため、
    /// 生バイトのまま書き出すことはできない。
    #[test]
    fn sgpd_box_seig_grouping_type_with_unknown_entry_encode_is_invalid_input() {
        let sgpd = SgpdBox {
            version: 1,
            grouping_type: *b"seig",
            default_length: Some(0),
            entries: vec![SgpdEntry::Unknown(vec![0x00; 20])],
        };
        let err = sgpd.encode_to_vec().expect_err(
            "grouping_type が 'seig' なのに Unknown エントリーをエンコードできてしまった",
        );
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("requires 'seig' entries"),
            "grouping_type とエントリーの整合の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SbgpBox: version 2 のデコードが InvalidData になること
    #[test]
    fn sbgp_box_version_2_decode_is_invalid_data() {
        let buf = build_box(*b"sbgp", &[0x02, 0x00, 0x00, 0x00]);
        let err = SbgpBox::decode(&buf).expect_err("SbgpBox version 2 を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("SbgpBox version"),
            "バージョンの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SbgpBox: エントリーが切り詰められたデコードが InvalidData になること
    #[test]
    fn sbgp_box_truncated_entries_is_invalid_data() {
        // entry_count 2 だが 1 エントリー分 (8 バイト) しかない
        let mut payload = vec![0x00, 0x00, 0x00, 0x00];
        payload.extend_from_slice(b"seig");
        payload.extend_from_slice(&2u32.to_be_bytes());
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        let buf = build_box(*b"sbgp", &payload);

        let err = SbgpBox::decode(&buf).expect_err("切り詰められた sbgp を受理してしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidData,
            "エラー種別が InvalidData ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("truncated"),
            "切り詰めの検査とは別のエラーである (reason={})",
            err.reason,
        );
    }

    /// SbgpBox: version 0 に `grouping_type_parameter` を指定したエンコードが
    /// InvalidInput になること
    #[test]
    fn sbgp_box_version_0_with_parameter_encode_is_invalid_input() {
        let sbgp = SbgpBox {
            version: 0,
            grouping_type: *b"seig",
            grouping_type_parameter: Some(1),
            entries: vec![SbgpEntry {
                sample_count: 1,
                group_description_index: 1,
            }],
        };
        let err = sbgp
            .encode_to_vec()
            .expect_err("version 0 で grouping_type_parameter をエンコードできてしまった");
        assert_eq!(
            err.kind,
            ErrorKind::InvalidInput,
            "エラー種別が InvalidInput ではない (reason={})",
            err.reason,
        );
        assert!(
            err.reason.contains("grouping_type_parameter"),
            "grouping_type_parameter の検査とは別のエラーである (reason={})",
            err.reason,
        );
    }
}
