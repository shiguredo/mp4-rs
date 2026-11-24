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
