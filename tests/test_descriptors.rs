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
        Decode, Encode, Uint,
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

    /// DecoderConfigDescriptor: buffer_size_db がバッファ境界を超過
    #[test]
    fn decoder_config_descriptor_buffer_size_exceeds_boundary() {
        let data = [
            0x04, // tag = 4
            0x02, // size = 2 (小さすぎ)
            0x40, // object_type_indication
            0x15, // stream_type + up_stream
                  // buffer_size_db の 3 バイトがない
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
            0xFF, 0x01, // size = 129 (境界超過)
            0x11, 0x90, // 2 バイトしかない
        ];
        let result = DecoderSpecificInfo::decode(&data);
        assert!(result.is_err());
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
        let result = SlConfigDescriptor::decode(&data);
        assert!(result.is_err());
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
