//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（vp09 用）

use c_api::boxes::Mp4SampleEntryVp09;

/// VP09（VP9）サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_vp09(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryVp09,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "vp09")?;
        f.member("width", data.width)?;
        f.member("height", data.height)?;
        f.member("profile", data.profile)?;
        f.member("level", data.level)?;
        f.member("bitDepth", data.bit_depth)?;
        f.member("chromaSubsampling", data.chroma_subsampling)?;
        f.member("videoFullRangeFlag", u8::from(data.video_full_range_flag))?;
        f.member("colourPrimaries", data.colour_primaries)?;
        f.member("transferCharacteristics", data.transfer_characteristics)?;
        f.member("matrixCoefficients", data.matrix_coefficients)
    })
}

/// JSON から Mp4SampleEntryVp09 に変換する
pub fn parse_json_mp4_sample_entry_vp09(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryVp09, nojson::JsonParseError> {
    Ok(Mp4SampleEntryVp09 {
        width: value.to_member("width")?.required()?.try_into()?,
        height: value.to_member("height")?.required()?.try_into()?,
        profile: value.to_member("profile")?.required()?.try_into()?,
        level: value.to_member("level")?.required()?.try_into()?,
        bit_depth: value.to_member("bitDepth")?.required()?.try_into()?,
        chroma_subsampling: value
            .to_member("chromaSubsampling")?
            .required()?
            .try_into()?,
        video_full_range_flag: value
            .to_member("videoFullRangeFlag")?
            .required()?
            .try_into()?,
        colour_primaries: value.to_member("colourPrimaries")?.required()?.try_into()?,
        transfer_characteristics: value
            .to_member("transferCharacteristics")?
            .required()?
            .try_into()?,
        matrix_coefficients: value
            .to_member("matrixCoefficients")?
            .required()?
            .try_into()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vp09_to_json() {
        let sample_entry = Mp4SampleEntryVp09 {
            width: 1280,
            height: 720,
            profile: 0,
            level: 31,
            bit_depth: 8,
            chroma_subsampling: 1, // 4:2:0
            video_full_range_flag: false,
            colour_primaries: 1,         // BT.709
            transfer_characteristics: 1, // BT.709
            matrix_coefficients: 1,      // BT.709
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_vp09(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"vp09""#));
        assert!(json.contains(r#""width":1280"#));
        assert!(json.contains(r#""height":720"#));
        assert!(json.contains(r#""profile":0"#));
        assert!(json.contains(r#""level":31"#));
        assert!(json.contains(r#""bitDepth":8"#));
        assert!(json.contains(r#""chromaSubsampling":1"#));
        assert!(json.contains(r#""videoFullRangeFlag":0"#));
        assert!(json.contains(r#""colourPrimaries":1"#));
        assert!(json.contains(r#""transferCharacteristics":1"#));
        assert!(json.contains(r#""matrixCoefficients":1"#));
    }

    #[test]
    fn test_json_to_vp09() {
        let json_str = r#"{
    "kind": "vp09",
    "width": 1280,
    "height": 720,
    "profile": 0,
    "level": 31,
    "bitDepth": 8,
    "chromaSubsampling": 1,
    "videoFullRangeFlag": false,
    "colourPrimaries": 1,
    "transferCharacteristics": 1,
    "matrixCoefficients": 1
}"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let sample_entry = parse_json_mp4_sample_entry_vp09(json.value()).expect("valid vp09 JSON");

        assert_eq!(sample_entry.width, 1280);
        assert_eq!(sample_entry.height, 720);
        assert_eq!(sample_entry.profile, 0);
        assert_eq!(sample_entry.level, 31);
        assert_eq!(sample_entry.bit_depth, 8);
        assert_eq!(sample_entry.chroma_subsampling, 1);
        assert!(!sample_entry.video_full_range_flag);
        assert_eq!(sample_entry.colour_primaries, 1);
        assert_eq!(sample_entry.transfer_characteristics, 1);
        assert_eq!(sample_entry.matrix_coefficients, 1);
    }
}
