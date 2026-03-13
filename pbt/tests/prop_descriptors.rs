//! ディスクリプター構造体の Property-Based Testing

use proptest::prelude::*;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

/// DecoderSpecificInfo を生成する Strategy
fn arb_decoder_specific_info() -> impl Strategy<Value = DecoderSpecificInfo> {
    prop::collection::vec(any::<u8>(), 0..50).prop_map(|payload| DecoderSpecificInfo { payload })
}

/// DecoderConfigDescriptor を生成する Strategy
fn arb_decoder_config_descriptor() -> impl Strategy<Value = DecoderConfigDescriptor> {
    (
        any::<u8>(),
        0u8..64, // stream_type: 6 bits
        any::<bool>(),
        any::<u32>().prop_map(|v| v & 0x00FFFFFF), // buffer_size_db: 24 bits
        any::<u32>(),
        any::<u32>(),
        prop::option::of(arb_decoder_specific_info()),
    )
        .prop_map(
            |(
                object_type_indication,
                stream_type,
                up_stream,
                buffer_size_db,
                max_bitrate,
                avg_bitrate,
                dec_specific_info,
            )| {
                DecoderConfigDescriptor {
                    object_type_indication,
                    stream_type: Uint::new(stream_type),
                    up_stream: Uint::new(up_stream as u8),
                    buffer_size_db: Uint::new(buffer_size_db),
                    max_bitrate,
                    avg_bitrate,
                    dec_specific_info,
                }
            },
        )
}

/// EsDescriptor を生成する Strategy
fn arb_es_descriptor() -> impl Strategy<Value = EsDescriptor> {
    (
        1u16..=u16::MAX,                       // es_id (0 は予約)
        0u8..32,                               // stream_priority: 5 bits
        prop::option::of(1u16..=u16::MAX),     // depends_on_es_id
        prop::option::of("[a-zA-Z0-9]{0,20}"), // url_string (ASCII のみ)
        prop::option::of(1u16..=u16::MAX),     // ocr_es_id
        arb_decoder_config_descriptor(),
    )
        .prop_map(
            |(
                es_id,
                stream_priority,
                depends_on_es_id,
                url_string,
                ocr_es_id,
                dec_config_descr,
            )| {
                EsDescriptor {
                    es_id,
                    stream_priority: Uint::new(stream_priority),
                    depends_on_es_id,
                    url_string,
                    ocr_es_id,
                    dec_config_descr,
                    sl_config_descr: SlConfigDescriptor,
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    // ===== DecoderSpecificInfo のテスト =====

    /// DecoderSpecificInfo の encode/decode roundtrip
    #[test]
    fn decoder_specific_info_roundtrip(payload in prop::collection::vec(any::<u8>(), 0..100)) {
        let info = DecoderSpecificInfo { payload: payload.clone() };
        let encoded = info.encode_to_vec().unwrap();
        let (decoded, _) = DecoderSpecificInfo::decode(&encoded).unwrap();

        prop_assert_eq!(decoded.payload, payload);
    }

    // ===== DecoderConfigDescriptor のテスト =====

    /// DecoderConfigDescriptor の encode/decode roundtrip
    #[test]
    fn decoder_config_descriptor_roundtrip(desc in arb_decoder_config_descriptor()) {
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded).unwrap();

        prop_assert_eq!(decoded.object_type_indication, desc.object_type_indication);
        prop_assert_eq!(decoded.stream_type.get(), desc.stream_type.get());
        prop_assert_eq!(decoded.up_stream.get(), desc.up_stream.get());
        prop_assert_eq!(decoded.buffer_size_db.get(), desc.buffer_size_db.get());
        prop_assert_eq!(decoded.max_bitrate, desc.max_bitrate);
        prop_assert_eq!(decoded.avg_bitrate, desc.avg_bitrate);
        prop_assert_eq!(decoded.dec_specific_info, desc.dec_specific_info);
    }

    // ===== EsDescriptor のテスト =====

    /// EsDescriptor の encode/decode roundtrip
    #[test]
    fn es_descriptor_roundtrip(desc in arb_es_descriptor()) {
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = EsDescriptor::decode(&encoded).unwrap();

        prop_assert_eq!(decoded.es_id, desc.es_id);
        prop_assert_eq!(decoded.stream_priority.get(), desc.stream_priority.get());
        prop_assert_eq!(decoded.depends_on_es_id, desc.depends_on_es_id);
        prop_assert_eq!(decoded.url_string, desc.url_string);
        prop_assert_eq!(decoded.ocr_es_id, desc.ocr_es_id);
        prop_assert_eq!(decoded.dec_config_descr.object_type_indication, desc.dec_config_descr.object_type_indication);
        prop_assert_eq!(decoded.dec_config_descr.stream_type.get(), desc.dec_config_descr.stream_type.get());
        prop_assert_eq!(decoded.dec_config_descr.max_bitrate, desc.dec_config_descr.max_bitrate);
        prop_assert_eq!(decoded.dec_config_descr.avg_bitrate, desc.dec_config_descr.avg_bitrate);
    }
}

// ===== 境界値テスト =====

mod boundary_tests {
    use super::*;

    /// DecoderSpecificInfo: 空のペイロード
    #[test]
    fn decoder_specific_info_empty() {
        let info = DecoderSpecificInfo { payload: vec![] };
        let encoded = info.encode_to_vec().unwrap();
        let (decoded, _) = DecoderSpecificInfo::decode(&encoded).unwrap();
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
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded).unwrap();
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
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = EsDescriptor::decode(&encoded).unwrap();
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
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = EsDescriptor::decode(&encoded).unwrap();
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
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = SlConfigDescriptor::decode(&encoded).unwrap();
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
        let encoded = desc.encode_to_vec().unwrap();
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded).unwrap();
        assert_eq!(decoded.stream_type.get(), 63);
        assert_eq!(decoded.up_stream.get(), 1);
    }
}
