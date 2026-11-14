//! HEVC サンプルエントリー (`hev1` / `hvc1`) の共通 JSON 変換
//!
//! `hev1` と `hvc1` はボックスの型が違うだけで、`hvcC` の中身と JSON 表現は同じである。
//! 片方だけを修正して片方を忘れる事故を防ぐため、変換の実装はこのモジュールに集約する。
use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{HvccBox, HvccNalUintArray, VisualSampleEntryFields};

/// HEVC サンプルエントリーを JSON オブジェクトとしてフォーマットする
///
/// `kind` は `"hev1"` か `"hvc1"` を渡す。
pub(crate) fn fmt_json(
    f: &mut nojson::JsonFormatter<'_, '_>,
    kind: &str,
    visual: &VisualSampleEntryFields,
    hvcc: &HvccBox,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", kind)?;
        f.member("width", visual.width)?;
        f.member("height", visual.height)?;
        f.member("generalProfileSpace", hvcc.general_profile_space.get())?;
        f.member("generalTierFlag", hvcc.general_tier_flag.get())?;
        f.member("generalProfileIdc", hvcc.general_profile_idc.get())?;
        f.member(
            "generalProfileCompatibilityFlags",
            hvcc.general_profile_compatibility_flags,
        )?;
        f.member(
            "generalConstraintIndicatorFlags",
            hvcc.general_constraint_indicator_flags.get(),
        )?;
        f.member("generalLevelIdc", hvcc.general_level_idc)?;
        f.member("chromaFormatIdc", hvcc.chroma_format_idc.get())?;
        f.member("bitDepthLumaMinus8", hvcc.bit_depth_luma_minus8.get())?;
        f.member("bitDepthChromaMinus8", hvcc.bit_depth_chroma_minus8.get())?;
        f.member(
            "minSpatialSegmentationIdc",
            hvcc.min_spatial_segmentation_idc.get(),
        )?;
        f.member("parallelismType", hvcc.parallelism_type.get())?;
        f.member("avgFrameRate", hvcc.avg_frame_rate)?;
        f.member("constantFrameRate", hvcc.constant_frame_rate.get())?;
        f.member("numTemporalLayers", hvcc.num_temporal_layers.get())?;
        f.member("temporalIdNested", hvcc.temporal_id_nested.get())?;
        f.member("lengthSizeMinusOne", hvcc.length_size_minus_one.get())?;
        f.member(
            "naluArrays",
            nojson::json(|f| fmt_nalu_arrays(f, &hvcc.nalu_arrays)),
        )
    })
}

/// `naluArrays` を JSON 配列に変換する
///
/// コアの [`HvccNalUintArray`] は `DisplayJson` を実装していないため、
/// JSON のキー (`naluType` / `units`) に対応する変換をここで行う。
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

/// JSON オブジェクトから HEVC の共通フィールドを構築する
///
/// 幅・高さと `hvcC` を返す。呼び出し側が `hev1` / `hvc1` の型に包む。
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<(u16, u16, HvccBox), nojson::JsonParseError> {
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

    let hvcc_box = HvccBox {
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
    };

    Ok((width, height, hvcc_box))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の VPS / SPS / PPS バイト列
    const VPS: &[u8] = &[0x40, 0x01, 0x0c, 0x01];
    const SPS: &[u8] = &[0x42, 0x01, 0x01, 0x01];
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

    /// テスト用の HEVC サンプルエントリー JSON を組み立てる
    ///
    /// `omit_member` にメンバー名を渡すと、そのスカラーフィールドだけを欠落させる。
    fn build_test_json(kind: &str, nalu_arrays_json: &str, omit_member: Option<&str>) -> String {
        let body = TEST_SCALAR_FIELDS
            .iter()
            .filter(|(name, _)| Some(*name) != omit_member)
            .map(|(name, value)| format!("\"{name}\": {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{\"kind\": \"{kind}\", {body}, \"naluArrays\": {nalu_arrays_json}}}")
    }

    /// テスト用の `hvcC` を構築する
    fn test_hvcc_box() -> (VisualSampleEntryFields, HvccBox) {
        (
            crate::boxes::build_visual_sample_entry_fields(1920, 1080),
            HvccBox {
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
        )
    }

    #[test]
    fn test_hevc_to_json() {
        let (visual, hvcc_box) = test_hvcc_box();

        let json = nojson::json(|f| fmt_json(f, "hev1", &visual, &hvcc_box)).to_string();
        assert!(
            json.contains(r#""kind":"hev1""#),
            "kind が出力されること: {json}"
        );
        assert!(
            json.contains(r#""generalProfileIdc":2"#),
            "generalProfileIdc が出力されること: {json}"
        );
        assert!(
            json.contains(r#""naluArrays":[{"naluType":32,"units":[[64,1,12,1]]},{"naluType":33,"units":[[66,1,1,1]]},{"naluType":34,"units":[[68,1,0]]}]"#),
            "naluArrays が naluType と units の配列として出力されること: {json}"
        );
    }

    #[test]
    fn test_json_to_hevc() {
        let json_str = build_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (width, height, hvcc_box) = parse_json(json.value()).expect("有効な HEVC JSON");

        // 18 個のスカラーフィールドが欠落・取り違えなく取り込まれることを検証する
        assert_eq!(width, 1920);
        assert_eq!(height, 1080);
        assert_eq!(hvcc_box.general_profile_space.get(), 0);
        assert_eq!(hvcc_box.general_tier_flag.get(), 0);
        assert_eq!(hvcc_box.general_profile_idc.get(), 2);
        assert_eq!(hvcc_box.general_profile_compatibility_flags, 1_610_612_736);
        assert_eq!(
            hvcc_box.general_constraint_indicator_flags.get(),
            193_514_046_488_576
        );
        assert_eq!(hvcc_box.general_level_idc, 120);
        assert_eq!(hvcc_box.chroma_format_idc.get(), 1);
        assert_eq!(hvcc_box.bit_depth_luma_minus8.get(), 0);
        assert_eq!(hvcc_box.bit_depth_chroma_minus8.get(), 0);
        assert_eq!(hvcc_box.min_spatial_segmentation_idc.get(), 0);
        assert_eq!(hvcc_box.parallelism_type.get(), 0);
        assert_eq!(hvcc_box.avg_frame_rate, 0);
        assert_eq!(hvcc_box.constant_frame_rate.get(), 0);
        assert_eq!(hvcc_box.num_temporal_layers.get(), 1);
        assert_eq!(hvcc_box.temporal_id_nested.get(), 0);
        assert_eq!(hvcc_box.length_size_minus_one.get(), 3);

        assert_eq!(hvcc_box.nalu_arrays.len(), 3);
        assert_eq!(hvcc_box.nalu_arrays[0].nal_unit_type.get(), 32);
        assert_eq!(hvcc_box.nalu_arrays[0].nalus, vec![vec![64, 1, 12, 1]]);
        assert_eq!(hvcc_box.nalu_arrays[1].nal_unit_type.get(), 33);
        assert_eq!(hvcc_box.nalu_arrays[2].nal_unit_type.get(), 34);
        // JSON に arrayCompleteness が無いため 0 が設定される
        assert!(
            hvcc_box
                .nalu_arrays
                .iter()
                .all(|a| a.array_completeness.get() == 0)
        );
    }

    #[test]
    fn test_json_to_hevc_rejects_missing_width() {
        let json_str = build_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]}
            ]"#,
            Some("width"),
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(
            parse_json(json.value()).is_err(),
            "width 欠落時はパース失敗すること"
        );
    }

    /// `naluArrays` 以外のスカラーフィールドが欠落したときに拒否することを表で確認する
    #[test]
    fn test_json_to_hevc_rejects_each_missing_scalar_field() {
        for (name, _) in TEST_SCALAR_FIELDS {
            let json_str = build_test_json("hev1", "[]", Some(name));
            let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
            assert!(
                parse_json(json.value()).is_err(),
                "{name} 欠落時はパース失敗すること"
            );
        }
    }

    #[test]
    fn test_json_to_hevc_empty_nalu_element_roundtrip() {
        let json_str = build_test_json("hev1", r#"[{"naluType": 32, "units": [[]]}]"#, None);

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (width, height, hvcc_box) = parse_json(json.value()).expect("空 NALU 要素の JSON");
        assert_eq!(hvcc_box.nalu_arrays[0].nalus, vec![Vec::new()]);

        let out = nojson::json(|f| {
            fmt_json(
                f,
                "hev1",
                &crate::boxes::build_visual_sample_entry_fields(width, height),
                &hvcc_box,
            )
        })
        .to_string();
        assert!(
            out.contains(r#""units":[[]]"#),
            "空 NALU 要素が units:[[]] として再出力されること: {out}"
        );
    }

    #[test]
    fn test_json_to_hevc_multiple_units_per_array() {
        let json_str = build_test_json(
            "hev1",
            r#"[{"naluType": 32, "units": [[1, 2], [3, 4]]}]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (_, _, hvcc_box) = parse_json(json.value()).expect("有効な JSON");
        assert_eq!(hvcc_box.nalu_arrays[0].nalus, vec![vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn test_json_to_hevc_empty_nalu_arrays() {
        let json_str = build_test_json("hev1", "[]", None);

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (width, height, hvcc_box) = parse_json(json.value()).expect("有効な JSON");
        assert!(hvcc_box.nalu_arrays.is_empty());

        let out = nojson::json(|f| {
            fmt_json(
                f,
                "hev1",
                &crate::boxes::build_visual_sample_entry_fields(width, height),
                &hvcc_box,
            )
        })
        .to_string();
        assert!(
            out.contains(r#""naluArrays":[]"#),
            "空の naluArrays が [] として再出力されること: {out}"
        );
    }

    /// JSON へ出力して再パースした結果が元の値と一致することを検証する
    #[test]
    fn test_hevc_json_roundtrip() {
        let (visual, hvcc_box) = test_hvcc_box();

        let json = nojson::json(|f| fmt_json(f, "hvc1", &visual, &hvcc_box)).to_string();
        let raw = nojson::RawJson::parse(&json).expect("有効な JSON");
        let (width, height, parsed_hvcc_box) = parse_json(raw.value()).expect("有効な JSON");

        assert_eq!(
            crate::boxes::build_visual_sample_entry_fields(width, height),
            visual
        );
        assert_eq!(parsed_hvcc_box, hvcc_box);
    }
}
