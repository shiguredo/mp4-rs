//! HEV1 サンプルエントリー (`hev1`) の JSON 変換を提供するモジュール
//!
//! `hvcC` の変換は [`crate::boxes_hevc`] に集約しており、このモジュールは
//! `hev1` 固有の `kind` と型の対応だけを担当する。
use shiguredo_mp4::boxes::Hev1Box;

/// HEV1 サンプルエントリーを JSON オブジェクトとしてフォーマットする
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Hev1Box) -> std::fmt::Result {
    crate::boxes_hevc::fmt_json(f, "hev1", &data.visual, &data.hvcc_box)
}

/// JSON オブジェクトから HEV1 サンプルエントリーを構築する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Hev1Box, nojson::JsonParseError> {
    let (width, height, hvcc_box) = crate::boxes_hevc::parse_json(value, "hev1")?;
    Ok(Hev1Box {
        visual: crate::boxes::build_visual_sample_entry_fields(width, height),
        hvcc_box,
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hev1_kind_is_output() {
        let json_str = r#"{"kind":"hev1","width":640,"height":480,"generalProfileSpace":0,"generalTierFlag":0,"generalProfileIdc":2,"generalProfileCompatibilityFlags":1610612736,"generalConstraintIndicatorFlags":193514046488576,"generalLevelIdc":120,"chromaFormatIdc":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"minSpatialSegmentationIdc":0,"parallelismType":0,"avgFrameRate":0,"constantFrameRate":0,"numTemporalLayers":1,"temporalIdNested":0,"lengthSizeMinusOne":3,"naluArrays":[{"naluType":32,"units":[[64,1,12,1]]}]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な hev1 JSON");

        let output = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            output.contains(r#""kind":"hev1""#),
            "kind が hev1 であること: {output}"
        );
    }
}
