//! MP4A（AAC）サンプルエントリー（`mp4a`）の JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::FixedPointNumber;
use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{AudioSampleEntryFields, EsdsBox, Mp4aBox};
use shiguredo_mp4::descriptors::{
    DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor,
};

/// MP4A（AAC）サンプルエントリーを JSON オブジェクトとしてフォーマットする
///
/// `sampleRate` は音声共通フィールドの 16.16 固定小数点数を、整数部と小数部を
/// 合わせた 1 つの実数として出力する。
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Mp4aBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "mp4a")?;
        f.member("channelCount", data.audio.channelcount)?;
        f.member("sampleRate", sample_rate_to_json(data.audio.samplerate))?;
        f.member("sampleSize", data.audio.samplesize)?;
        f.member(
            "bufferSizeDb",
            data.esds_box.es.dec_config_descr.buffer_size_db.get(),
        )?;
        f.member("maxBitrate", data.esds_box.es.dec_config_descr.max_bitrate)?;
        f.member("avgBitrate", data.esds_box.es.dec_config_descr.avg_bitrate)?;
        // AAC の AudioSpecificConfig などはバイト列をそのまま数値配列として出力する。
        // デコーダー固有情報を持たない場合は空配列にする
        let dec_specific_info = data
            .esds_box
            .es
            .dec_config_descr
            .dec_specific_info
            .as_ref()
            .map_or(&[][..], |info| info.payload.as_slice());
        f.member("decSpecificInfo", dec_specific_info)
    })
}

/// JSON オブジェクトから MP4A（AAC）サンプルエントリーを構築する
///
/// ES 記述子は AAC 音声向けの既定値で構築する。`decSpecificInfo` は
/// `DecoderSpecificInfo` のペイロードとしてそのまま格納する。
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4aBox, nojson::JsonParseError> {
    let channel_count: u16 = value.to_member("channelCount")?.required()?.try_into()?;
    let sample_rate = sample_rate_from_json(value)?;
    let sample_size: u16 = value.to_member("sampleSize")?.required()?.try_into()?;

    let buffer_size_db_value = value.to_member("bufferSizeDb")?.required()?;
    let buffer_size_db: u32 = buffer_size_db_value.try_into()?;
    if buffer_size_db > 0xFF_FFFF {
        // DecoderConfigDescriptor の buffer_size_db は 24 ビット幅で、
        // 収まらない値はエンコード時に上位バイトが捨てられる。黙って壊れるより入力時点で拒否する
        return Err(buffer_size_db_value.invalid("bufferSizeDb must fit in 24 bits (0..=16777215)"));
    }

    let max_bitrate: u32 = value.to_member("maxBitrate")?.required()?.try_into()?;
    let avg_bitrate: u32 = value.to_member("avgBitrate")?.required()?.try_into()?;
    let dec_specific_info: Vec<u8> = value.to_member("decSpecificInfo")?.required()?.try_into()?;

    let esds_box = EsdsBox {
        es: EsDescriptor {
            es_id: EsDescriptor::MIN_ES_ID,
            stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
            depends_on_es_id: None,
            url_string: None,
            ocr_es_id: None,
            dec_config_descr: DecoderConfigDescriptor {
                object_type_indication:
                    DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
                stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
                up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
                buffer_size_db: Uint::new(buffer_size_db),
                max_bitrate,
                avg_bitrate,
                // 空の decSpecificInfo も「デコーダー固有情報を持つ」として扱い、
                // 出力時に空配列へ戻せるようにする
                dec_specific_info: Some(DecoderSpecificInfo {
                    payload: dec_specific_info,
                }),
            },
            sl_config_descr: SlConfigDescriptor,
        },
    };

    Ok(Mp4aBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: channel_count,
            samplesize: sample_size,
            samplerate: sample_rate,
        },
        esds_box,
        unknown_boxes: Vec::new(),
    })
}

/// 音声共通フィールドのサンプリングレートを JSON の数値に変換する
///
/// [`AudioSampleEntryFields::samplerate`] は 16.16 固定小数点数なので、
/// 整数部と小数部を合わせて 1 つの実数として返す。
fn sample_rate_to_json(sample_rate: FixedPointNumber<u16, u16>) -> f64 {
    f64::from(sample_rate.integer) + f64::from(sample_rate.fraction) / 65536.0
}

/// JSON の `sampleRate` を音声共通フィールドのサンプリングレートに変換する
///
/// JSON では 16.16 固定小数点数を 1 つの実数として表現しているため、
/// 上位 16 ビットを整数部、下位 16 ビットを小数部として分解する。
fn sample_rate_from_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<FixedPointNumber<u16, u16>, nojson::JsonParseError> {
    let sample_rate_value = value.to_member("sampleRate")?.required()?;
    let sample_rate: f64 = sample_rate_value.try_into()?;
    if !(0.0..65536.0).contains(&sample_rate) {
        return Err(sample_rate_value.invalid("sampleRate must be in the range [0, 65536)"));
    }

    // 実数を 16.16 固定小数点のビット列に直してから、整数部と小数部を取り出す
    let fixed_point = (sample_rate * 65536.0).round() as u32;
    Ok(FixedPointNumber::new(
        (fixed_point >> 16) as u16,
        (fixed_point & 0xFFFF) as u16,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の MP4A（AAC）サンプルエントリーを構築する
    ///
    /// `dec_specific_info` には `Option` を渡すため、デコーダー固有情報を
    /// 持たないサンプルエントリーも構築できる。
    fn build_mp4a_box(dec_specific_info: Option<Vec<u8>>) -> Mp4aBox {
        Mp4aBox {
            audio: AudioSampleEntryFields {
                data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(44100, 0),
            },
            esds_box: EsdsBox {
                es: EsDescriptor {
                    es_id: EsDescriptor::MIN_ES_ID,
                    stream_priority: EsDescriptor::LOWEST_STREAM_PRIORITY,
                    depends_on_es_id: None,
                    url_string: None,
                    ocr_es_id: None,
                    dec_config_descr: DecoderConfigDescriptor {
                        object_type_indication:
                            DecoderConfigDescriptor::OBJECT_TYPE_INDICATION_AUDIO_ISO_IEC_14496_3,
                        stream_type: DecoderConfigDescriptor::STREAM_TYPE_AUDIO,
                        up_stream: DecoderConfigDescriptor::UP_STREAM_FALSE,
                        buffer_size_db: Uint::new(0),
                        max_bitrate: 128000,
                        avg_bitrate: 128000,
                        dec_specific_info: dec_specific_info
                            .map(|payload| DecoderSpecificInfo { payload }),
                    },
                    sl_config_descr: SlConfigDescriptor,
                },
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_mp4a_to_json() {
        // AAC-LC の AudioSpecificConfig の典型的な値をデコーダー固有情報として使う
        let sample_entry = build_mp4a_box(Some(vec![0x12, 0x10]));

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"mp4a""#),
            "kind が mp4a であること: {json}"
        );
        assert!(
            json.contains(r#""channelCount":2"#),
            "channelCount が 2 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleRate":44100"#),
            "sampleRate が 44100 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleSize":16"#),
            "sampleSize が 16 であること: {json}"
        );
        assert!(
            json.contains(r#""bufferSizeDb":0"#),
            "bufferSizeDb が 0 であること: {json}"
        );
        assert!(
            json.contains(r#""maxBitrate":128000"#),
            "maxBitrate が 128000 であること: {json}"
        );
        assert!(
            json.contains(r#""avgBitrate":128000"#),
            "avgBitrate が 128000 であること: {json}"
        );
        assert!(
            json.contains(r#""decSpecificInfo":[18,16]"#),
            "decSpecificInfo が数値配列で出力されること: {json}"
        );
    }

    #[test]
    fn test_mp4a_to_json_without_dec_specific_info() {
        // デコーダー固有情報を持たないサンプルエントリーでは空配列を出力する
        let sample_entry = build_mp4a_box(None);

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""decSpecificInfo":[]"#),
            "デコーダー固有情報を持たない場合は空配列が出力されること: {json}"
        );
    }

    #[test]
    fn test_json_to_mp4a() {
        let json_str = r#"{
            "kind": "mp4a",
            "channelCount": 2,
            "sampleRate": 44100,
            "sampleSize": 16,
            "bufferSizeDb": 0,
            "maxBitrate": 128000,
            "avgBitrate": 128000,
            "decSpecificInfo": [1, 2]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value()).expect("mp4a の JSON をパースできること");

        assert_eq!(
            sample_entry.audio.channelcount, 2,
            "channelCount が 2 であること"
        );
        assert_eq!(
            sample_entry.audio.samplerate,
            FixedPointNumber::new(44100, 0),
            "sampleRate が 44100 であること"
        );
        assert_eq!(
            sample_entry.audio.samplesize, 16,
            "sampleSize が 16 であること"
        );

        let dec_config_descr = &sample_entry.esds_box.es.dec_config_descr;
        assert_eq!(
            dec_config_descr.buffer_size_db.get(),
            0,
            "bufferSizeDb が 0 であること"
        );
        assert_eq!(
            dec_config_descr.max_bitrate, 128000,
            "maxBitrate が 128000 であること"
        );
        assert_eq!(
            dec_config_descr.avg_bitrate, 128000,
            "avgBitrate が 128000 であること"
        );
        let dec_specific_info = dec_config_descr
            .dec_specific_info
            .as_ref()
            .expect("デコーダー固有情報を持つこと");
        assert_eq!(
            dec_specific_info.payload,
            vec![1, 2],
            "decSpecificInfo が [1, 2] であること"
        );

        assert_eq!(
            sample_entry.audio.data_reference_index,
            AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            "data_reference_index が既定値であること"
        );
        assert!(
            sample_entry.unknown_boxes.is_empty(),
            "unknown_boxes が空であること"
        );
    }

    /// 空の decSpecificInfo を parse して JSON に再出力する往復テスト
    ///
    /// 空配列を渡した場合も、出力側で空配列として表現できることを検証する。
    #[test]
    fn test_json_to_mp4a_empty_dec_specific_info_roundtrip() {
        let json_str = r#"{
            "kind": "mp4a",
            "channelCount": 2,
            "sampleRate": 44100,
            "sampleSize": 16,
            "bufferSizeDb": 0,
            "maxBitrate": 128000,
            "avgBitrate": 128000,
            "decSpecificInfo": []
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry =
            parse_json(json.value()).expect("空 decSpecificInfo の mp4a JSON をパースできること");

        let dec_specific_info = sample_entry
            .esds_box
            .es
            .dec_config_descr
            .dec_specific_info
            .as_ref()
            .expect("デコーダー固有情報を持つこと");
        assert!(
            dec_specific_info.payload.is_empty(),
            "decSpecificInfo のペイロードが空であること"
        );

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""decSpecificInfo":[]"#),
            "空の decSpecificInfo が [] として再出力されること: {out}"
        );
    }

    #[test]
    fn test_json_to_mp4a_rejects_missing_channel_count() {
        // decSpecificInfo は揃っているが必須フィールドの channelCount が欠落している
        let json_str = r#"{
            "kind": "mp4a",
            "sampleRate": 44100,
            "sampleSize": 16,
            "bufferSizeDb": 0,
            "maxBitrate": 128000,
            "avgBitrate": 128000,
            "decSpecificInfo": [1, 2]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let result = parse_json(json.value());
        assert!(result.is_err(), "channelCount 欠落時はパース失敗すること");
    }

    #[test]
    fn test_json_to_mp4a_rejects_buffer_size_db_over_24_bits() {
        // buffer_size_db は 24 ビット幅なので、収まらない値は拒否する
        let json_str = r#"{
            "kind": "mp4a",
            "channelCount": 2,
            "sampleRate": 44100,
            "sampleSize": 16,
            "bufferSizeDb": 16777216,
            "maxBitrate": 128000,
            "avgBitrate": 128000,
            "decSpecificInfo": [1, 2]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let result = parse_json(json.value());
        assert!(
            result.is_err(),
            "bufferSizeDb が 24 ビットに収まらない場合はパース失敗すること"
        );
    }
}
