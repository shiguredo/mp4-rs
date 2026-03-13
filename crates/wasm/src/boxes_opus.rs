//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（opus 用）

use c_api::boxes::Mp4SampleEntryOpus;

/// Opus サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_opus(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryOpus,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "opus")?;
        f.member("channelCount", data.channel_count)?;
        f.member("sampleRate", data.sample_rate)?;
        f.member("sampleSize", data.sample_size)?;
        f.member("preSkip", data.pre_skip)?;
        f.member("inputSampleRate", data.input_sample_rate)?;
        f.member("outputGain", data.output_gain)
    })
}

/// JSON から Mp4SampleEntryOpus に変換する
pub fn parse_json_mp4_sample_entry_opus(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryOpus, nojson::JsonParseError> {
    Ok(Mp4SampleEntryOpus {
        channel_count: value.to_member("channelCount")?.required()?.try_into()?,
        sample_rate: value.to_member("sampleRate")?.required()?.try_into()?,
        sample_size: value.to_member("sampleSize")?.required()?.try_into()?,
        pre_skip: value.to_member("preSkip")?.required()?.try_into()?,
        input_sample_rate: value.to_member("inputSampleRate")?.required()?.try_into()?,
        output_gain: value.to_member("outputGain")?.required()?.try_into()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_to_json() {
        let sample_entry = Mp4SampleEntryOpus {
            channel_count: 2,
            sample_rate: 48000,
            sample_size: 16,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_opus(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"opus""#));
        assert!(json.contains(r#""channelCount":2"#));
        assert!(json.contains(r#""sampleRate":48000"#));
        assert!(json.contains(r#""preSkip":312"#));
        assert!(json.contains(r#""inputSampleRate":48000"#));
        assert!(json.contains(r#""outputGain":0"#));
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

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let sample_entry = parse_json_mp4_sample_entry_opus(json.value()).expect("valid opus JSON");

        assert_eq!(sample_entry.channel_count, 2);
        assert_eq!(sample_entry.sample_rate, 48000);
        assert_eq!(sample_entry.sample_size, 16);
        assert_eq!(sample_entry.pre_skip, 312);
        assert_eq!(sample_entry.input_sample_rate, 48000);
        assert_eq!(sample_entry.output_gain, 0);
    }
}
