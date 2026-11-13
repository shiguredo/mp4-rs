//! HEVC サンプルエントリー（`hvc1`）の JSON 変換を提供するモジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{Hvc1Box, HvccBox, HvccNalUintArray};

/// HVC1（H.265/HEVC）サンプルエントリーを JSON に変換する
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Hvc1Box) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "hvc1")?;
        f.member("width", data.visual.width)?;
        f.member("height", data.visual.height)?;
        f.member(
            "generalProfileSpace",
            data.hvcc_box.general_profile_space.get(),
        )?;
        f.member("generalTierFlag", data.hvcc_box.general_tier_flag.get())?;
        f.member("generalProfileIdc", data.hvcc_box.general_profile_idc.get())?;
        f.member(
            "generalProfileCompatibilityFlags",
            data.hvcc_box.general_profile_compatibility_flags,
        )?;
        f.member(
            "generalConstraintIndicatorFlags",
            data.hvcc_box.general_constraint_indicator_flags.get(),
        )?;
        f.member("generalLevelIdc", data.hvcc_box.general_level_idc)?;
        f.member("chromaFormatIdc", data.hvcc_box.chroma_format_idc.get())?;
        f.member(
            "bitDepthLumaMinus8",
            data.hvcc_box.bit_depth_luma_minus8.get(),
        )?;
        f.member(
            "bitDepthChromaMinus8",
            data.hvcc_box.bit_depth_chroma_minus8.get(),
        )?;
        f.member(
            "minSpatialSegmentationIdc",
            data.hvcc_box.min_spatial_segmentation_idc.get(),
        )?;
        f.member("parallelismType", data.hvcc_box.parallelism_type.get())?;
        f.member("avgFrameRate", data.hvcc_box.avg_frame_rate)?;
        f.member("constantFrameRate", data.hvcc_box.constant_frame_rate.get())?;
        f.member("numTemporalLayers", data.hvcc_box.num_temporal_layers.get())?;
        f.member("temporalIdNested", data.hvcc_box.temporal_id_nested.get())?;
        f.member(
            "lengthSizeMinusOne",
            data.hvcc_box.length_size_minus_one.get(),
        )?;
        f.member(
            "naluArrays",
            nojson::json(|f| fmt_nalu_arrays(f, &data.hvcc_box.nalu_arrays)),
        )
    })
}

/// `naluArrays` を JSON 配列に変換する
///
/// コアの [`HvccNalUintArray`] は `DisplayJson` を実装していないため、
/// JSON のキー（`naluType` / `units`）に対応する変換をここで行う
fn fmt_nalu_arrays(
    f: &mut nojson::JsonFormatter<'_, '_>,
    nalu_arrays: &[HvccNalUintArray],
) -> std::fmt::Result {
    f.array(|f| {
        for nalu_array in nalu_arrays {
            f.element(nojson::object(|f| {
                f.member("naluType", nalu_array.nal_unit_type.get())?;
                f.member("units", nalu_array.nalus.as_slice())
            }))?;
        }
        Ok(())
    })
}

/// JSON から HVC1 サンプルエントリーに変換する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Hvc1Box, nojson::JsonParseError> {
    let nalu_arrays_value = value.to_member("naluArrays")?.required()?;
    let mut nalu_arrays = Vec::new();
    for nalu_array in nalu_arrays_value.to_array()? {
        let nal_unit_type: u8 = nalu_array.to_member("naluType")?.required()?.try_into()?;
        let nalus: Vec<Vec<u8>> = nalu_array.to_member("units")?.required()?.try_into()?;
        nalu_arrays.push(HvccNalUintArray {
            // JSON には `arrayCompleteness` に対応するキーが無いため、
            // 「この NALU 型のすべてが配列に含まれるとは限らない」を表す 0 を補う
            array_completeness: Uint::new(0),
            nal_unit_type: Uint::new(nal_unit_type),
            nalus,
        });
    }

    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let general_profile_space: u8 = value
        .to_member("generalProfileSpace")?
        .required()?
        .try_into()?;
    let general_tier_flag: u8 = value.to_member("generalTierFlag")?.required()?.try_into()?;
    let general_profile_idc: u8 = value
        .to_member("generalProfileIdc")?
        .required()?
        .try_into()?;
    let general_profile_compatibility_flags: u32 = value
        .to_member("generalProfileCompatibilityFlags")?
        .required()?
        .try_into()?;
    let general_constraint_indicator_flags: u64 = value
        .to_member("generalConstraintIndicatorFlags")?
        .required()?
        .try_into()?;
    let general_level_idc: u8 = value.to_member("generalLevelIdc")?.required()?.try_into()?;
    let chroma_format_idc: u8 = value.to_member("chromaFormatIdc")?.required()?.try_into()?;
    let bit_depth_luma_minus8: u8 = value
        .to_member("bitDepthLumaMinus8")?
        .required()?
        .try_into()?;
    let bit_depth_chroma_minus8: u8 = value
        .to_member("bitDepthChromaMinus8")?
        .required()?
        .try_into()?;
    let min_spatial_segmentation_idc: u16 = value
        .to_member("minSpatialSegmentationIdc")?
        .required()?
        .try_into()?;
    let parallelism_type: u8 = value.to_member("parallelismType")?.required()?.try_into()?;
    let avg_frame_rate: u16 = value.to_member("avgFrameRate")?.required()?.try_into()?;
    let constant_frame_rate: u8 = value
        .to_member("constantFrameRate")?
        .required()?
        .try_into()?;
    let num_temporal_layers: u8 = value
        .to_member("numTemporalLayers")?
        .required()?
        .try_into()?;
    let temporal_id_nested: u8 = value
        .to_member("temporalIdNested")?
        .required()?
        .try_into()?;
    let length_size_minus_one: u8 = value
        .to_member("lengthSizeMinusOne")?
        .required()?
        .try_into()?;

    Ok(Hvc1Box {
        visual: crate::boxes::build_visual_sample_entry_fields(width, height),
        hvcc_box: HvccBox {
            general_profile_space: Uint::new(general_profile_space),
            general_tier_flag: Uint::new(general_tier_flag),
            general_profile_idc: Uint::new(general_profile_idc),
            general_profile_compatibility_flags,
            general_constraint_indicator_flags: Uint::new(general_constraint_indicator_flags),
            general_level_idc,
            min_spatial_segmentation_idc: Uint::new(min_spatial_segmentation_idc),
            parallelism_type: Uint::new(parallelism_type),
            chroma_format_idc: Uint::new(chroma_format_idc),
            bit_depth_luma_minus8: Uint::new(bit_depth_luma_minus8),
            bit_depth_chroma_minus8: Uint::new(bit_depth_chroma_minus8),
            avg_frame_rate,
            constant_frame_rate: Uint::new(constant_frame_rate),
            num_temporal_layers: Uint::new(num_temporal_layers),
            temporal_id_nested: Uint::new(temporal_id_nested),
            length_size_minus_one: Uint::new(length_size_minus_one),
            nalu_arrays,
        },
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::boxes::VisualSampleEntryFields;

    /// テスト用の VPS バイト列
    const VPS: &[u8] = &[0x40, 0x01, 0x0c, 0x01];

    /// テスト用の SPS バイト列
    const SPS: &[u8] = &[0x42, 0x01, 0x01, 0x01];

    /// テスト用の PPS バイト列
    const PPS: &[u8] = &[0x44, 0x01, 0x00];

    /// テスト用 JSON のスカラーフィールド（メンバー名と値の組）
    const TEST_SCALAR_FIELDS: &[(&str, &str)] = &[
        ("width", "1920"),
        ("height", "1080"),
        ("generalProfileSpace", "0"),
        ("generalTierFlag", "0"),
        ("generalProfileIdc", "2"),
        ("generalProfileCompatibilityFlags", "1610612736"),
        ("generalConstraintIndicatorFlags", "193514046488576"),
        ("generalLevelIdc", "120"),
        ("chromaFormatIdc", "1"),
        ("bitDepthLumaMinus8", "0"),
        ("bitDepthChromaMinus8", "0"),
        ("minSpatialSegmentationIdc", "0"),
        ("parallelismType", "0"),
        ("avgFrameRate", "0"),
        ("constantFrameRate", "0"),
        ("numTemporalLayers", "1"),
        ("temporalIdNested", "0"),
        ("lengthSizeMinusOne", "3"),
    ];

    /// テスト用の HVC1 サンプルエントリー JSON を組み立てる
    ///
    /// `omit_member` にメンバー名を渡すと、そのスカラーフィールドだけを欠落させる。
    /// 文字列置換ではなくフィールド表から組み立てるため、欠落させたいメンバーが
    /// 実際に消えていることを保証できる
    fn build_test_json(nalu_arrays_json: &str, omit_member: Option<&str>) -> String {
        let body = TEST_SCALAR_FIELDS
            .iter()
            .filter(|(name, _)| Some(*name) != omit_member)
            .map(|(name, value)| format!("\"{name}\": {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{\"kind\": \"hvc1\", {body}, \"naluArrays\": {nalu_arrays_json}}}")
    }

    /// テスト用の HVC1 サンプルエントリーを構築する
    fn test_hvc1_box() -> Hvc1Box {
        Hvc1Box {
            visual: crate::boxes::build_visual_sample_entry_fields(1920, 1080),
            hvcc_box: HvccBox {
                general_profile_space: Uint::new(0),
                general_tier_flag: Uint::new(0),
                general_profile_idc: Uint::new(2),
                general_profile_compatibility_flags: 0x6000_0000,
                general_constraint_indicator_flags: Uint::new(0x0000_b000_0000_0000),
                general_level_idc: 120,
                min_spatial_segmentation_idc: Uint::new(0),
                parallelism_type: Uint::new(0),
                chroma_format_idc: Uint::new(1),
                bit_depth_luma_minus8: Uint::new(0),
                bit_depth_chroma_minus8: Uint::new(0),
                avg_frame_rate: 0,
                constant_frame_rate: Uint::new(0),
                num_temporal_layers: Uint::new(1),
                temporal_id_nested: Uint::new(0),
                length_size_minus_one: Uint::new(3),
                nalu_arrays: vec![
                    HvccNalUintArray {
                        array_completeness: Uint::new(0),
                        nal_unit_type: Uint::new(32),
                        nalus: vec![VPS.to_vec()],
                    },
                    HvccNalUintArray {
                        array_completeness: Uint::new(0),
                        nal_unit_type: Uint::new(33),
                        nalus: vec![SPS.to_vec()],
                    },
                    HvccNalUintArray {
                        array_completeness: Uint::new(0),
                        nal_unit_type: Uint::new(34),
                        nalus: vec![PPS.to_vec()],
                    },
                ],
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_hvc1_to_json() {
        let sample_entry = test_hvc1_box();

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"hvc1""#),
            "kind が出力されること: {json}"
        );
        assert!(
            json.contains(r#""width":1920"#),
            "width が出力されること: {json}"
        );
        assert!(
            json.contains(r#""height":1080"#),
            "height が出力されること: {json}"
        );
        assert!(
            json.contains(r#""generalProfileIdc":2"#),
            "generalProfileIdc が出力されること: {json}"
        );
        assert!(
            json.contains(r#""generalConstraintIndicatorFlags":193514046488576"#),
            "generalConstraintIndicatorFlags が出力されること: {json}"
        );
        assert!(
            json.contains(r#""generalLevelIdc":120"#),
            "generalLevelIdc が出力されること: {json}"
        );
        assert!(
            json.contains(r#""lengthSizeMinusOne":3"#),
            "lengthSizeMinusOne が出力されること: {json}"
        );
        assert!(
            json.contains(
                r#""naluArrays":[{"naluType":32,"units":[[64,1,12,1]]},{"naluType":33,"units":[[66,1,1,1]]},{"naluType":34,"units":[[68,1,0]]}]"#
            ),
            "naluArrays が naluType と units の配列として出力されること: {json}"
        );
    }

    #[test]
    fn test_json_to_hvc1() {
        let json_str = build_test_json(
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な hvc1 JSON");

        // 既定値と照合して 18 個のスカラーフィールドが欠落・取り違えなく
        // 取り込まれることを検証する
        assert_eq!(sample_entry.visual.width, 1920);
        assert_eq!(sample_entry.visual.height, 1080);
        assert_eq!(sample_entry.hvcc_box.general_profile_space.get(), 0);
        assert_eq!(sample_entry.hvcc_box.general_tier_flag.get(), 0);
        assert_eq!(sample_entry.hvcc_box.general_profile_idc.get(), 2);
        assert_eq!(
            sample_entry.hvcc_box.general_profile_compatibility_flags,
            1_610_612_736
        );
        assert_eq!(
            sample_entry
                .hvcc_box
                .general_constraint_indicator_flags
                .get(),
            193_514_046_488_576
        );
        assert_eq!(sample_entry.hvcc_box.general_level_idc, 120);
        assert_eq!(sample_entry.hvcc_box.chroma_format_idc.get(), 1);
        assert_eq!(sample_entry.hvcc_box.bit_depth_luma_minus8.get(), 0);
        assert_eq!(sample_entry.hvcc_box.bit_depth_chroma_minus8.get(), 0);
        assert_eq!(sample_entry.hvcc_box.min_spatial_segmentation_idc.get(), 0);
        assert_eq!(sample_entry.hvcc_box.parallelism_type.get(), 0);
        assert_eq!(sample_entry.hvcc_box.avg_frame_rate, 0);
        assert_eq!(sample_entry.hvcc_box.constant_frame_rate.get(), 0);
        assert_eq!(sample_entry.hvcc_box.num_temporal_layers.get(), 1);
        assert_eq!(sample_entry.hvcc_box.temporal_id_nested.get(), 0);
        assert_eq!(sample_entry.hvcc_box.length_size_minus_one.get(), 3);

        // naluArrays は `Vec<HvccNalUintArray>` として取り込まれる
        assert_eq!(sample_entry.hvcc_box.nalu_arrays.len(), 3);
        assert_eq!(sample_entry.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
        assert_eq!(
            sample_entry.hvcc_box.nalu_arrays[0].nalus,
            vec![vec![64, 1, 12, 1]]
        );
        assert_eq!(sample_entry.hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
        assert_eq!(
            sample_entry.hvcc_box.nalu_arrays[1].nalus,
            vec![vec![66, 1, 1, 1]]
        );
        assert_eq!(sample_entry.hvcc_box.nalu_arrays[2].nal_unit_type.get(), 34);
        assert_eq!(
            sample_entry.hvcc_box.nalu_arrays[2].nalus,
            vec![vec![68, 1, 0]]
        );
        // JSON に arrayCompleteness が無いため 0 が設定される
        assert!(
            sample_entry
                .hvcc_box
                .nalu_arrays
                .iter()
                .all(|a| a.array_completeness.get() == 0)
        );

        assert!(
            sample_entry.unknown_boxes.is_empty(),
            "unknown_boxes は空であること"
        );
        assert_eq!(
            sample_entry.visual.data_reference_index,
            VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            "data_reference_index には既定値が設定されること"
        );
    }

    #[test]
    fn test_json_to_hvc1_rejects_missing_width_after_nalu_arrays() {
        // naluArrays を先に読み、その後に width を読む。必須フィールドが
        // 欠落していれば、その時点でパースが失敗することを確認する
        let json_str = build_test_json(
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]"#,
            Some("width"),
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let result = parse_json(json.value());
        assert!(result.is_err(), "width 欠落時はパース失敗すること");
    }

    /// 空の `units` 要素を持つ JSON を parse → 再出力する往復テスト
    ///
    /// `nalus` は `Vec<Vec<u8>>` なので空要素をそのまま保持できる。
    /// 空配列要素が `units:[[]]` として再出力されることを検証する
    #[test]
    fn test_json_to_hvc1_empty_nalu_element_roundtrip() {
        let json_str = build_test_json(
            r#"[
                {"naluType": 32, "units": [[]]}
            ]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("空 NALU 要素の hvc1 JSON");

        assert_eq!(sample_entry.hvcc_box.nalu_arrays.len(), 1);
        assert_eq!(sample_entry.hvcc_box.nalu_arrays[0].nalus, vec![Vec::new()]);

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""units":[[]]"#),
            "空 NALU 要素が units:[[]] として再出力されること: {out}"
        );
    }

    /// 1 つの配列に複数の NALU を持つ JSON を取り込むテスト
    ///
    /// `HvccNalUintArray` は配列単位で `Vec<Vec<u8>>` を保持するため、
    /// 個数と中身が配列単位のまま保たれることを検証する
    #[test]
    fn test_json_to_hvc1_multiple_units_per_array() {
        let json_str = build_test_json(
            r#"[
                {"naluType": 32, "units": [[1, 2], [3, 4]]}
            ]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な hvc1 JSON");

        assert_eq!(sample_entry.hvcc_box.nalu_arrays.len(), 1);
        assert_eq!(sample_entry.hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
        assert_eq!(
            sample_entry.hvcc_box.nalu_arrays[0].nalus,
            vec![vec![1, 2], vec![3, 4]]
        );
    }

    /// 空の `naluArrays` を parse → 再出力する境界値テスト
    #[test]
    fn test_json_to_hvc1_empty_nalu_arrays() {
        let json_str = build_test_json("[]", None);

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な hvc1 JSON");

        assert!(sample_entry.hvcc_box.nalu_arrays.is_empty());

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""naluArrays":[]"#),
            "空の naluArrays が [] として再出力されること: {out}"
        );
    }

    /// JSON へ出力して再パースした結果が元の値と一致することを検証する
    #[test]
    fn test_hvc1_json_roundtrip() {
        let sample_entry = test_hvc1_box();

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        let raw = nojson::RawJson::parse(&json).expect("有効な JSON");
        let parsed = parse_json(raw.value()).expect("有効な hvc1 JSON");

        assert_eq!(
            parsed, sample_entry,
            "JSON へ出力して再パースした結果が元の値と一致すること"
        );
    }
}
