//! VP9 サンプルエントリー（`vp09`）の JSON 変換を提供するモジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{Vp09Box, VpccBox};

/// VP9 サンプルエントリーを JSON オブジェクトとしてフォーマットする
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Vp09Box) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "vp09")?;
        f.member("width", data.visual.width)?;
        f.member("height", data.visual.height)?;
        f.member("profile", data.vpcc_box.profile)?;
        f.member("level", data.vpcc_box.level)?;
        f.member("bitDepth", data.vpcc_box.bit_depth.get())?;
        f.member("chromaSubsampling", data.vpcc_box.chroma_subsampling.get())?;
        f.member(
            "videoFullRangeFlag",
            u8::from(data.vpcc_box.video_full_range_flag.as_bool()),
        )?;
        f.member("colourPrimaries", data.vpcc_box.colour_primaries)?;
        f.member(
            "transferCharacteristics",
            data.vpcc_box.transfer_characteristics,
        )?;
        f.member("matrixCoefficients", data.vpcc_box.matrix_coefficients)
    })
}

/// JSON オブジェクトから VP9 サンプルエントリーを構築する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Vp09Box, nojson::JsonParseError> {
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let profile: u8 = value.to_member("profile")?.required()?.try_into()?;
    let level: u8 = value.to_member("level")?.required()?.try_into()?;
    let bit_depth: u8 = value.to_member("bitDepth")?.required()?.try_into()?;
    let chroma_subsampling: u8 = value
        .to_member("chromaSubsampling")?
        .required()?
        .try_into()?;
    let video_full_range_flag: bool = value
        .to_member("videoFullRangeFlag")?
        .required()?
        .try_into()?;
    let colour_primaries: u8 = value.to_member("colourPrimaries")?.required()?.try_into()?;
    let transfer_characteristics: u8 = value
        .to_member("transferCharacteristics")?
        .required()?
        .try_into()?;
    let matrix_coefficients: u8 = value
        .to_member("matrixCoefficients")?
        .required()?
        .try_into()?;

    let vpcc_box = VpccBox {
        profile,
        level,
        bit_depth: Uint::new(bit_depth),
        chroma_subsampling: Uint::new(chroma_subsampling),
        video_full_range_flag: Uint::new(u8::from(video_full_range_flag)),
        colour_primaries,
        transfer_characteristics,
        matrix_coefficients,
        // VP9 の vpcC は codec_initialization_data を常に空とする
        codec_initialization_data: Vec::new(),
    };

    Ok(Vp09Box {
        visual: crate::boxes::build_visual_sample_entry_fields(width, height),
        vpcc_box,
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::boxes::VisualSampleEntryFields;

    #[test]
    fn test_vp09_to_json() {
        let sample_entry = Vp09Box {
            visual: crate::boxes::build_visual_sample_entry_fields(1280, 720),
            vpcc_box: VpccBox {
                profile: 0,
                level: 31,
                bit_depth: Uint::new(8),
                chroma_subsampling: Uint::new(1), // 4:2:0
                video_full_range_flag: Uint::new(0),
                colour_primaries: 1,         // BT.709
                transfer_characteristics: 1, // BT.709
                matrix_coefficients: 1,      // BT.709
                codec_initialization_data: Vec::new(),
            },
            unknown_boxes: Vec::new(),
        };

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"vp09""#),
            "kind が vp09 であること: {json}"
        );
        assert!(
            json.contains(r#""width":1280"#),
            "width が 1280 であること: {json}"
        );
        assert!(
            json.contains(r#""height":720"#),
            "height が 720 であること: {json}"
        );
        assert!(
            json.contains(r#""profile":0"#),
            "profile が 0 であること: {json}"
        );
        assert!(
            json.contains(r#""level":31"#),
            "level が 31 であること: {json}"
        );
        assert!(
            json.contains(r#""bitDepth":8"#),
            "bitDepth が 8 であること: {json}"
        );
        assert!(
            json.contains(r#""chromaSubsampling":1"#),
            "chromaSubsampling が 1 であること: {json}"
        );
        assert!(
            json.contains(r#""videoFullRangeFlag":0"#),
            "videoFullRangeFlag が数値の 0 であること: {json}"
        );
        assert!(
            json.contains(r#""colourPrimaries":1"#),
            "colourPrimaries が 1 であること: {json}"
        );
        assert!(
            json.contains(r#""transferCharacteristics":1"#),
            "transferCharacteristics が 1 であること: {json}"
        );
        assert!(
            json.contains(r#""matrixCoefficients":1"#),
            "matrixCoefficients が 1 であること: {json}"
        );
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

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value()).expect("vp09 の JSON をパースできること");

        assert_eq!(sample_entry.visual.width, 1280, "width が 1280 であること");
        assert_eq!(sample_entry.visual.height, 720, "height が 720 であること");
        assert_eq!(
            sample_entry.visual.data_reference_index,
            VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            "data_reference_index が既定値であること"
        );
        assert_eq!(sample_entry.vpcc_box.profile, 0, "profile が 0 であること");
        assert_eq!(sample_entry.vpcc_box.level, 31, "level が 31 であること");
        assert_eq!(
            sample_entry.vpcc_box.bit_depth.get(),
            8,
            "bitDepth が 8 であること"
        );
        assert_eq!(
            sample_entry.vpcc_box.chroma_subsampling.get(),
            1,
            "chromaSubsampling が 1 であること"
        );
        assert!(
            !sample_entry.vpcc_box.video_full_range_flag.as_bool(),
            "videoFullRangeFlag が false であること"
        );
        assert_eq!(
            sample_entry.vpcc_box.colour_primaries, 1,
            "colourPrimaries が 1 であること"
        );
        assert_eq!(
            sample_entry.vpcc_box.transfer_characteristics, 1,
            "transferCharacteristics が 1 であること"
        );
        assert_eq!(
            sample_entry.vpcc_box.matrix_coefficients, 1,
            "matrixCoefficients が 1 であること"
        );
        // JSON 表現を持たないフィールドは VP9 の固定値で補完される
        assert!(
            sample_entry.vpcc_box.codec_initialization_data.is_empty(),
            "codec_initialization_data が空であること"
        );
        assert!(
            sample_entry.unknown_boxes.is_empty(),
            "unknown_boxes が空であること"
        );
    }
}
