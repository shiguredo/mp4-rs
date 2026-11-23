//! `src/descriptors.rs` の境界値・エラーパス単体テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_descriptors.rs` が担う。

// ===== 境界値テスト =====

mod descriptors_boundary_tests {
    use shiguredo_mp4::{
        Decode, Encode, Uint,
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

    /// DecoderSpecificInfo: 空のペイロード
    #[test]
    fn decoder_specific_info_empty() {
        let info = DecoderSpecificInfo { payload: vec![] };
        let encoded = info.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DecoderSpecificInfo::decode(&encoded)
            .expect("直前にエンコードした有効な DecoderSpecificInfo は必ずデコードできる");
        assert!(decoded.payload.is_empty());
    }

    /// DecoderConfigDescriptor: AAC 用のデフォルト設定
    #[test]
    fn decoder_config_descriptor_aac_defaults() {
        let desc = DecoderConfigDescriptor {
            object_type_indication:
                DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
            stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
            up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
            buffer_size_db: Uint::new(0),
            max_bitrate: 128000,
            avg_bitrate: 128000,
            dec_specific_info: None,
        };
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な DecoderConfigDescriptor は必ずデコードできる");
        assert_eq!(decoded.object_type_indication, 0x40);
        assert_eq!(decoded.stream_type.get(), 0x05);
        assert_eq!(decoded.up_stream.get(), 0);
    }

    /// EsDescriptor: 最小構成
    #[test]
    fn es_descriptor_minimal() {
        let desc = EsDescriptor {
            es_id: EsDescriptor::MIN_ES_ID,
            stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
            depends_on_es_id: None,
            url_string: None,
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication: 0x40,
                stream_type: Uint::new(0x05),
                up_stream: Uint::new(0),
                buffer_size_db: Uint::new(0),
                max_bitrate: 0,
                avg_bitrate: 0,
                dec_specific_info: None,
            },
            sl_config_descr: SlConfigDescriptor,
        };
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = EsDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な EsDescriptor は必ずデコードできる");
        assert_eq!(decoded.es_id, 1);
        assert_eq!(decoded.stream_priority.get(), 0);
        assert!(decoded.depends_on_es_id.is_none());
        assert!(decoded.url_string.is_none());
        assert!(decoded.ocr_es_id.is_none());
    }

    /// EsDescriptor: 全オプション付き
    #[test]
    fn es_descriptor_all_options() {
        let desc = EsDescriptor {
            es_id: 1000,
            stream_priority: Uint::new(31), // 最大値
            depends_on_es_id: Some(1),
            url_string: Some("http://example.com".to_string()),
            ocr_es_id: Some(2),
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication: 0x40,
                stream_type: Uint::new(0x05),
                up_stream: Uint::new(0),
                buffer_size_db: Uint::new(0x00FFFFFF), // 24-bit 最大値
                max_bitrate: u32::MAX,
                avg_bitrate: u32::MAX,
                dec_specific_info: Some(DecoderSpecificInfo {
                    payload: vec![0x11, 0x90],
                }),
            },
            sl_config_descr: SlConfigDescriptor,
        };
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = EsDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な EsDescriptor は必ずデコードできる");
        assert_eq!(decoded.es_id, 1000);
        assert_eq!(decoded.stream_priority.get(), 31);
        assert_eq!(decoded.depends_on_es_id, Some(1));
        assert_eq!(decoded.url_string, Some("http://example.com".to_string()));
        assert_eq!(decoded.ocr_es_id, Some(2));
        assert_eq!(decoded.dec_config_descr.buffer_size_db.get(), 0x00FFFFFF);
        assert_eq!(decoded.dec_config_descr.max_bitrate, u32::MAX);
    }

    /// SlConfigDescriptor: 固定値
    #[test]
    fn sl_config_descriptor_fixed() {
        let desc = SlConfigDescriptor;
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = SlConfigDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な SlConfigDescriptor は必ずデコードできる");
        // SlConfigDescriptor はフィールドを持たない
        assert_eq!(decoded, SlConfigDescriptor);
    }

    /// DecoderConfigDescriptor: stream_type 境界値
    #[test]
    fn decoder_config_descriptor_stream_type_boundary() {
        // 最大値 (6 bits = 63)
        let desc = DecoderConfigDescriptor {
            object_type_indication: 0,
            stream_type: Uint::new(63),
            up_stream: Uint::new(1),
            buffer_size_db: Uint::new(0),
            max_bitrate: 0,
            avg_bitrate: 0,
            dec_specific_info: None,
        };
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な DecoderConfigDescriptor は必ずデコードできる");
        assert_eq!(decoded.stream_type.get(), 63);
        assert_eq!(decoded.up_stream.get(), 1);
    }
}

// ===== descriptors.rs のエラーパステスト =====

mod descriptor_error_tests {
    use shiguredo_mp4::{
        Decode, Encode, ErrorKind, Uint,
        descriptors::{
            DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
        },
    };

    // ===== EsDescriptor のエラーパス =====

    /// EsDescriptor: URL 文字列が長すぎる (256バイト以上)
    #[test]
    fn es_descriptor_url_too_long() {
        let desc = EsDescriptor {
            es_id: 1,
            stream_priority: Uint::new(0),
            depends_on_es_id: None,
            url_string: Some("x".repeat(256)), // 256 バイト
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication: 0x40,
                stream_type: Uint::new(0x05),
                up_stream: Uint::new(0),
                buffer_size_db: Uint::new(0),
                max_bitrate: 0,
                avg_bitrate: 0,
                dec_specific_info: None,
            },
            sl_config_descr: SlConfigDescriptor,
        };
        let result = desc.encode_to_vec();
        assert!(result.is_err());
    }

    /// EsDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn es_descriptor_invalid_tag() {
        // tag = 4 (DecoderConfigDescriptor のタグ) だが EsDescriptor を期待
        let data = [
            0x04, // tag = 4 (不正、3 を期待)
            0x05, // size = 5
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x00, 0x00, // padding
        ];
        let result = EsDescriptor::decode(&data);
        assert!(result.is_err());
    }

    // ===== DecoderConfigDescriptor のエラーパス =====

    /// DecoderConfigDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn decoder_config_descriptor_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、4 を期待)
            0x05, // size = 5
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
        ];
        let result = DecoderConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    // ===== DecoderSpecificInfo のエラーパス =====

    /// DecoderSpecificInfo: 不正なタグでのデコードエラー
    #[test]
    fn decoder_specific_info_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、5 を期待)
            0x02, // size = 2
            0x11, 0x90, // payload
        ];
        let result = DecoderSpecificInfo::decode(&data);
        assert!(result.is_err());
    }

    /// DecoderSpecificInfo: ペイロードがバッファ境界を超過
    #[test]
    fn decoder_specific_info_payload_exceeds_boundary() {
        let data = [
            0x05, // tag = 5
            0xFF, 0x01, // size = 16257 (境界超過)
            0x11, 0x90, // 2 バイトしかない
        ];
        let result = DecoderSpecificInfo::decode(&data);
        assert!(result.is_err());
    }

    /// DecoderSpecificInfo: サイズ欄の値が `usize` に収まらない
    ///
    /// 宣言サイズは 2 の 64 乗 + 2 で、64 ビット環境ではサイズ欄の 10 バイト目、32 ビット環境では 6 バイト目で
    /// `usize` に収まらなくなる。
    /// 上位ビットを切り捨てた 2 バイトのペイロードとして受け入れず、サイズ欄の時点で `InvalidData` を返す
    #[test]
    fn decoder_specific_info_size_exceeds_usize_max() {
        let data = [
            0x05, // tag = 5
            0x82, // サイズ欄 1 バイト目: 継続ビットあり、値 2
            // サイズ欄 2〜9 バイト目: 継続ビットあり、値 0
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
            0x02, // サイズ欄 10 バイト目: 継続ビットなし、値 2
            // 上位ビットを切り捨てたサイズ 2 と一致するペイロード
            0xAA, 0xBB,
        ];
        let err = DecoderSpecificInfo::decode(&data)
            .expect_err("usize に収まらない宣言サイズはエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("Descriptor size exceeds usize::MAX"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// DecoderSpecificInfo: サイズ欄の途中で入力が尽きる
    ///
    /// サイズ欄の値が `usize` に収まらなくなった時点で、次のバイトを読まずに `InvalidData` を返す。
    /// サイズ欄の値が `usize` に収まる範囲で入力が尽きた場合は `InsufficientBuffer` を返す
    #[test]
    fn decoder_specific_info_size_field_truncated() {
        // 10 バイト目にも継続ビットがあるが、続きのバイトがない
        let overflowed = [
            0x05, // tag = 5
            0x82, // サイズ欄 1 バイト目: 継続ビットあり、値 2
            // サイズ欄 2〜10 バイト目: 継続ビットあり、値 0
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80,
        ];
        let err = DecoderSpecificInfo::decode(&overflowed)
            .expect_err("値が usize に収まらなくなったサイズ欄はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);

        // 2 バイト目にも継続ビットがあるが、続きのバイトがない
        let in_range = [
            0x05, // tag = 5
            0x80, 0x80, // サイズ欄 1〜2 バイト目: 継続ビットあり、値 0
        ];
        let err = DecoderSpecificInfo::decode(&in_range)
            .expect_err("サイズ欄の途中で入力が尽きるとエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
    }

    // ===== SlConfigDescriptor のエラーパス =====

    /// SlConfigDescriptor: 不正なタグでのデコードエラー
    #[test]
    fn sl_config_descriptor_invalid_tag() {
        let data = [
            0x03, // tag = 3 (不正、6 を期待)
            0x01, // size = 1
            0x02, // predefined = 2
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// SlConfigDescriptor: 未サポートの predefined 値
    #[test]
    fn sl_config_descriptor_unsupported_predefined() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x00, // predefined = 0 (未サポート、2 のみ対応)
        ];
        let err =
            SlConfigDescriptor::decode(&data).expect_err("未サポートの predefined はエラーになる");
        // 宣言されたペイロードの中で読めた値が未サポートの場合は、`InvalidData` に置き換えず `Unsupported` を返す
        assert_eq!(err.kind, ErrorKind::Unsupported);
    }

    /// SlConfigDescriptor: predefined = 1 (未サポート)
    #[test]
    fn sl_config_descriptor_predefined_1() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x01, // predefined = 1 (未サポート)
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }

    /// SlConfigDescriptor: predefined = 3 (未サポート)
    #[test]
    fn sl_config_descriptor_predefined_3() {
        let data = [
            0x06, // tag = 6
            0x01, // size = 1
            0x03, // predefined = 3 (未サポート)
        ];
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
    }
}

// ===== 宣言されたペイロードの範囲に関するテスト =====

mod descriptor_declared_payload_tests {
    use shiguredo_mp4::{
        Decode, ErrorKind,
        descriptors::{DecoderConfigDescriptor, EsDescriptor, SlConfigDescriptor},
    };

    /// SlConfigDescriptor: 入力が宣言されたペイロードの終端に達しない
    ///
    /// ペイロードの中を読む前にエラーにするため、中身が読める場合も読めない場合も `InsufficientBuffer` を返す
    #[test]
    fn sl_config_descriptor_payload_exceeds_input() {
        // 中身だけなら読める入力（足りない入力でも中身が読めれば成功にする実装を検出する）
        let decodable = [
            0x06, // tag = 6
            0x02, // size = 2 (入力にはペイロードが 1 バイトしかない)
            0x02, // predefined = 2
        ];
        let err = SlConfigDescriptor::decode(&decodable)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);

        // 中身を読むと `Unsupported` になる入力（終端を確かめる前に中身を読む実装を検出する）
        let unsupported = [
            0x06, // tag = 6
            0x02, // size = 2 (入力にはペイロードが 1 バイトしかない)
            0x00, // predefined = 0 (未サポート)
        ];
        let err = SlConfigDescriptor::decode(&unsupported)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
    }

    /// SlConfigDescriptor: 宣言されたペイロードに predefined が収まらない
    ///
    /// 入力には続きのバイトがあるが、宣言されたペイロードの外なので predefined として読まない
    #[test]
    fn sl_config_descriptor_predefined_outside_declared_payload() {
        let data = [
            0x06, // tag = 6
            0x00, // size = 0
            // ここから後ろは宣言されたペイロードの外
            0x02, // predefined = 2
        ];
        let err = SlConfigDescriptor::decode(&data)
            .expect_err("宣言されたペイロードに predefined が収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("SlConfigDescriptor"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// DecoderConfigDescriptor: 入力が宣言されたペイロードの終端に達しない
    ///
    /// ペイロードの中を読む前にエラーにするため、中身が読める場合も読めない場合も `InsufficientBuffer` を返す
    #[test]
    fn decoder_config_descriptor_payload_exceeds_input() {
        // 中身だけなら読める入力（足りない入力でも中身が読めれば成功にする実装を検出する）
        let decodable = [
            0x04, // tag = 4
            0x0E, // size = 14 (入力にはペイロードが 13 バイトしかない)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
        ];
        let err = DecoderConfigDescriptor::decode(&decodable)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);

        // 中身を読むと子の DecoderSpecificInfo が `InvalidData` になる入力
        // （終端を確かめる前に中身を読む実装を検出する）
        let broken = [
            0x04, // tag = 4
            0x20, // size = 32 (入力にはペイロードが 16 バイトしかない)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            0x05, 0x10, // DecoderSpecificInfo の tag と size = 16
            0x11, // DecoderSpecificInfo の payload の 1 バイト目
        ];
        let err = DecoderConfigDescriptor::decode(&broken)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
    }

    /// DecoderConfigDescriptor: 宣言されたペイロードに buffer_size_db が収まらない
    ///
    /// 入力には buffer_size_db の 3 バイトがあるが、宣言されたペイロードの外なので読まない
    #[test]
    fn decoder_config_descriptor_buffer_size_db_outside_declared_payload() {
        let data = [
            0x04, // tag = 4
            0x02, // size = 2 (stream_type + up_stream まで)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            // ここから後ろは宣言されたペイロードの外
            0x00, 0x00, 0x00, // buffer_size_db
        ];
        let err = DecoderConfigDescriptor::decode(&data)
            .expect_err("宣言されたペイロードに buffer_size_db が収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("buffer_size_db"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// DecoderConfigDescriptor: 宣言されたペイロードに固定フィールドが収まらない
    ///
    /// 入力には max_bitrate 以降のバイトがあるが、宣言されたペイロードの外なので読まない
    #[test]
    fn decoder_config_descriptor_fields_outside_declared_payload() {
        let data = [
            0x04, // tag = 4
            0x05, // size = 5 (buffer_size_db まで)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            // ここから後ろは宣言されたペイロードの外
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
        ];
        let err = DecoderConfigDescriptor::decode(&data)
            .expect_err("宣言されたペイロードに固定フィールドが収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("DecoderConfigDescriptor"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// DecoderConfigDescriptor: 子の DecoderSpecificInfo が宣言されたペイロードを超える
    ///
    /// どちらの入力にも子の続きのバイトがあるが、宣言されたペイロードの外なので子として読まない
    #[test]
    fn decoder_config_descriptor_child_exceeds_declared_payload() {
        // 子のヘッダーのうちタグだけが宣言されたペイロードに入っている
        let header_cut = [
            0x04, // tag = 4
            0x0E, // size = 14 (DecoderSpecificInfo の tag まで)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            0x05, // DecoderSpecificInfo の tag
            // ここから後ろは宣言されたペイロードの外
            0x02, // DecoderSpecificInfo の size = 2
            0x11, 0x90, // DecoderSpecificInfo の payload
        ];
        let err = DecoderConfigDescriptor::decode(&header_cut)
            .expect_err("子のヘッダーが宣言されたペイロードに収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);

        // 子の宣言されたペイロードが親の宣言されたペイロードを超える
        let payload_cut = [
            0x04, // tag = 4
            0x10, // size = 16 (DecoderSpecificInfo の payload の 1 バイト目まで)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            0x05, 0x02, // DecoderSpecificInfo の tag と size = 2
            0x11, // DecoderSpecificInfo の payload の 1 バイト目
            // ここから後ろは宣言されたペイロードの外
            0x90, // DecoderSpecificInfo の payload の 2 バイト目
        ];
        let err = DecoderConfigDescriptor::decode(&payload_cut)
            .expect_err("子のペイロードが親の宣言されたペイロードに収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
    }

    /// EsDescriptor: 入力が宣言されたペイロードの終端に達しない
    ///
    /// ペイロードの中を読む前にエラーにするため、中身が読める場合も読めない場合も `InsufficientBuffer` を返す
    #[test]
    fn es_descriptor_payload_exceeds_input() {
        // 中身だけなら読める入力（足りない入力でも中身が読めれば成功にする実装を検出する）
        let decodable = [
            0x03, // tag = 3
            0x16, // size = 22 (入力にはペイロードが 21 バイトしかない)
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x04, 0x0D, // DecoderConfigDescriptor の tag と size = 13
            0x40, 0x15, // object_type_indication、stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            0x06, 0x01, 0x02, // SlConfigDescriptor
        ];
        let err = EsDescriptor::decode(&decodable)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);

        // 中身を読むと子のサイズ欄の値が `usize` に収まらず `InvalidData` になる入力
        // （終端を確かめる前に中身を読む実装を検出する）。
        // サイズ欄は 32 ビット環境でも 64 ビット環境でも `usize` に収まらない長さにしている
        let broken = [
            0x03, // tag = 3
            0x40, // size = 64 (入力にはペイロードが 14 バイトしかない)
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x04, // DecoderConfigDescriptor の tag
            // DecoderConfigDescriptor のサイズ欄: 継続ビットあり、値 0x7F が 10 バイト
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        ];
        let err = EsDescriptor::decode(&broken)
            .expect_err("入力が宣言されたペイロードの終端に達しない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InsufficientBuffer);
    }

    /// EsDescriptor: URL 文字列が宣言されたペイロードを超える
    ///
    /// 入力には URL の 3〜4 バイト目があるが、宣言されたペイロードの外なので URL 文字列として読まない
    #[test]
    fn es_descriptor_url_exceeds_declared_payload() {
        let data = [
            0x03, // tag = 3
            0x06, // size = 6 (URL の 2 バイト目まで)
            0x00, 0x01, // es_id = 1
            0x40, // flags (URL_Flag = 1)
            0x04, // URL の長さ = 4
            0x61, 0x62, // URL の 1〜2 バイト目
            // ここから後ろは宣言されたペイロードの外
            0x63, 0x64, // URL の 3〜4 バイト目
        ];
        let err = EsDescriptor::decode(&data)
            .expect_err("宣言されたペイロードに URL 文字列が収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("URL string"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }

    /// EsDescriptor: 子の DecoderConfigDescriptor の宣言されたペイロードが親の宣言されたペイロードを超える
    ///
    /// 入力には子の続きのバイトがあるが、宣言されたペイロードの外なので子として読まない
    #[test]
    fn es_descriptor_child_exceeds_declared_payload() {
        let data = [
            0x03, // tag = 3
            0x0A, // size = 10 (DecoderConfigDescriptor の buffer_size_db まで)
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x04, 0x0D, // DecoderConfigDescriptor の tag と size = 13
            0x40, 0x15, // object_type_indication、stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            // ここから後ろは宣言されたペイロードの外
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            0x06, 0x01, 0x02, // SlConfigDescriptor
        ];
        let err = EsDescriptor::decode(&data)
            .expect_err("子のペイロードが親の宣言されたペイロードに収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
    }

    /// EsDescriptor: 宣言されたペイロードに SlConfigDescriptor が収まらない
    ///
    /// 入力には続きのバイトがあるが、宣言されたペイロードの外なので子として読まない
    #[test]
    fn es_descriptor_sl_config_outside_declared_payload() {
        let data = [
            0x03, // tag = 3
            0x12, // size = 18 (DecoderConfigDescriptor まで)
            0x00, 0x01, // es_id = 1
            0x00, // flags
            0x04, 0x0D, // DecoderConfigDescriptor の tag と size = 13
            0x40, 0x15, // object_type_indication、stream_type + up_stream
            0x00, 0x00, 0x00, // buffer_size_db
            0x00, 0x00, 0x00, 0x00, // max_bitrate
            0x00, 0x00, 0x00, 0x00, // avg_bitrate
            // ここから後ろは宣言されたペイロードの外
            0x06, 0x01, 0x02, // SlConfigDescriptor
        ];
        let err = EsDescriptor::decode(&data)
            .expect_err("宣言されたペイロードに SlConfigDescriptor が収まらない場合はエラーになる");
        assert_eq!(err.kind, ErrorKind::InvalidData);
        assert!(
            err.reason.contains("EsDescriptor"),
            "理由文字列が期待と違う: {}",
            err.reason
        );
    }
}
