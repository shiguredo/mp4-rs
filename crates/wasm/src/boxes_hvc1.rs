//! HVC1 サンプルエントリー (`hvc1`) の JSON 変換を提供するモジュール
//!
//! `hvcC` の変換は [`crate::boxes_hevc`] に集約しており、このモジュールは
//! `hvc1` 固有の `kind` と型の対応だけを担当する。
use shiguredo_mp4::boxes::Hvc1Box;

/// HVC1 サンプルエントリーを JSON オブジェクトとしてフォーマットする
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Hvc1Box) -> std::fmt::Result {
    crate::boxes_hevc::fmt_json(f, "hvc1", &data.visual, &data.hvcc_box, &data.unknown_boxes)
}

/// JSON オブジェクトから HVC1 サンプルエントリーを構築する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Hvc1Box, nojson::JsonParseError> {
    let (visual, hvcc_box) = crate::boxes_hevc::parse_json(value, "hvc1")?;
    Ok(Hvc1Box {
        visual,
        hvcc_box,
        unknown_boxes: crate::boxes::parse_unknown_boxes(value, &[b"hvcC"])?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hvc1_kind_is_output() {
        let json_str = r#"{"kind":"hvc1","width":640,"height":480,"generalProfileSpace":0,"generalTierFlag":0,"generalProfileIdc":2,"generalProfileCompatibilityFlags":1610612736,"generalConstraintIndicatorFlags":193514046488576,"generalLevelIdc":120,"chromaFormatIdc":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"minSpatialSegmentationIdc":0,"parallelismType":0,"avgFrameRate":0,"constantFrameRate":0,"numTemporalLayers":1,"temporalIdNested":0,"lengthSizeMinusOne":3,"naluArrays":[{"naluType":32,"units":[[64,1,12,1]]}]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な hvc1 JSON");

        let output = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            output.contains(r#""kind":"hvc1""#),
            "kind が hvc1 であること: {output}"
        );
    }
}
