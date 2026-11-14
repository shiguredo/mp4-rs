//! Opus サンプルエントリー（`Opus`）の JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::boxes::{AudioSampleEntryFields, DopsBox, OpusBox};

/// Opus サンプルエントリーを JSON オブジェクトとしてフォーマットする
///
/// `sampleRate` は音声共通フィールドの 16.16 固定小数点数を、整数部と小数部を
/// 合わせた 1 つの実数として出力する。
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &OpusBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "opus")?;
        f.member("channelCount", data.audio.channelcount)?;
        f.member(
            "sampleRate",
            crate::boxes::sample_rate_to_json(data.audio.samplerate),
        )?;
        f.member("sampleSize", data.audio.samplesize)?;
        f.member("preSkip", data.dops_box.pre_skip)?;
        f.member("inputSampleRate", data.dops_box.input_sample_rate)?;
        f.member("outputGain", data.dops_box.output_gain)
    })
}

/// JSON オブジェクトから Opus サンプルエントリーを構築する
///
/// `channelCount` は音声共通フィールドと `dOps` の出力チャンネル数の両方に設定する。
/// `dOps` 側は `u8` なので、表現できない値はパースエラーになる。
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<OpusBox, nojson::JsonParseError> {
    let channel_count_value = value.to_member("channelCount")?.required()?;
    let channel_count: u16 = channel_count_value.try_into()?;
    let output_channel_count = u8::try_from(channel_count)
        .map_err(|_| channel_count_value.invalid("channelCount must fit in 8 bits for Opus"))?;

    let sample_rate = crate::boxes::sample_rate_from_json(value)?;
    let sample_size: u16 = value.to_member("sampleSize")?.required()?.try_into()?;
    let pre_skip: u16 = value.to_member("preSkip")?.required()?.try_into()?;
    let input_sample_rate: u32 = value.to_member("inputSampleRate")?.required()?.try_into()?;
    let output_gain: i16 = value.to_member("outputGain")?.required()?.try_into()?;

    Ok(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: channel_count,
            samplesize: sample_size,
            samplerate: sample_rate,
        },
        dops_box: DopsBox {
            output_channel_count,
            pre_skip,
            input_sample_rate,
            output_gain,
        },
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::FixedPointNumber;

    /// テスト用の Opus サンプルエントリーを構築する
    fn build_opus_box() -> OpusBox {
        OpusBox {
            audio: AudioSampleEntryFields {
                data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
                channelcount: 2,
                samplesize: 16,
                samplerate: FixedPointNumber::new(48000, 0),
            },
            dops_box: DopsBox {
                output_channel_count: 2,
                pre_skip: 312,
                input_sample_rate: 48000,
                output_gain: 0,
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_opus_to_json() {
        let sample_entry = build_opus_box();

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"opus""#),
            "kind が opus であること: {json}"
        );
        assert!(
            json.contains(r#""channelCount":2"#),
            "channelCount が 2 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleRate":48000"#),
            "sampleRate が 48000 であること: {json}"
        );
        assert!(
            json.contains(r#""sampleSize":16"#),
            "sampleSize が 16 であること: {json}"
        );
        assert!(
            json.contains(r#""preSkip":312"#),
            "preSkip が 312 であること: {json}"
        );
        assert!(
            json.contains(r#""inputSampleRate":48000"#),
            "inputSampleRate が 48000 であること: {json}"
        );
        assert!(
            json.contains(r#""outputGain":0"#),
            "outputGain が 0 であること: {json}"
        );
    }

    #[test]
    fn test_json_to_opus() {
        let json_str = r#"{
            "kind": "opus",
            "channelCount": 2,
            "sampleRate": 48000,
            "sampleSize": 16,
            "preSkip": 312,
            "inputSampleRate": 48000,
            "outputGain": 0
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value()).expect("opus の JSON をパースできること");

        assert_eq!(
            sample_entry.audio.channelcount, 2,
            "channelCount が 2 であること"
        );
        assert_eq!(
            sample_entry.audio.samplerate,
            FixedPointNumber::new(48000, 0),
            "sampleRate が 48000 であること"
        );
        assert_eq!(
            sample_entry.audio.samplesize, 16,
            "sampleSize が 16 であること"
        );
        assert_eq!(
            sample_entry.dops_box.output_channel_count, 2,
            "dOps の出力チャンネル数が 2 であること"
        );
        assert_eq!(
            sample_entry.dops_box.pre_skip, 312,
            "preSkip が 312 であること"
        );
        assert_eq!(
            sample_entry.dops_box.input_sample_rate, 48000,
            "inputSampleRate が 48000 であること"
        );
        assert_eq!(
            sample_entry.dops_box.output_gain, 0,
            "outputGain が 0 であること"
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

    #[test]
    fn test_opus_sample_rate_fraction_roundtrip() {
        // 16.16 固定小数点の小数部を持つ sampleRate が、整数部と小数部を合わせた
        // 実数として出力され、パースで元の固定小数点数に戻ることを検証する
        let mut sample_entry = build_opus_box();
        sample_entry.audio.samplerate = FixedPointNumber::new(48000, 32768);

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""sampleRate":48000.5"#),
            "小数部を持つ sampleRate が実数として出力されること: {json}"
        );

        let parsed_json = nojson::RawJson::parse(&json).expect("出力した JSON をパースできること");
        let parsed = parse_json(parsed_json.value()).expect("opus の JSON をパースできること");
        assert_eq!(
            parsed.audio.samplerate,
            FixedPointNumber::new(48000, 32768),
            "小数部を含めて往復できること"
        );
    }

    #[test]
    fn test_json_to_opus_rejects_channel_count_over_u8() {
        // dOps の出力チャンネル数は u8 なので、u8 に収まらない channelCount は拒否する
        let json_str = r#"{
            "kind": "opus",
            "channelCount": 256,
            "sampleRate": 48000,
            "sampleSize": 16,
            "preSkip": 312,
            "inputSampleRate": 48000,
            "outputGain": 0
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let result = parse_json(json.value());
        assert!(
            result.is_err(),
            "channelCount が u8 に収まらない場合はパース失敗すること"
        );
    }
}
