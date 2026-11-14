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
/// JSON のキー (`naluType` / `arrayCompleteness` / `units`) に対応する変換をここで行う。
fn fmt_nalu_arrays(
    f: &mut nojson::JsonFormatter<'_, '_>,
    nalu_arrays: &[HvccNalUintArray],
) -> std::fmt::Result {
    f.array(|f| {
        for nalu_array in nalu_arrays {
            f.element(nojson::object(|f| {
                f.member("naluType", nalu_array.nal_unit_type.get())?;
                f.member("arrayCompleteness", nalu_array.array_completeness.get())?;
                f.member("units", nalu_array.nalus.as_slice())
            }))?;
        }
        Ok(())
    })
}

/// `arrayCompleteness` が省略された場合の既定値を返す
///
/// `hvc1` はパラメータセットを `hvcC` に完全な形で持つことを表す fourcc であり、
/// VPS / SPS / PPS の配列は `array_completeness` を 1 にする必要がある。
/// `hev1` はパラメータセットをサンプル側に持てるため 0 を既定値にする。
fn default_array_completeness(kind: &str, nal_unit_type: u8) -> u8 {
    if kind == "hvc1" && matches!(nal_unit_type, 32..=34) {
        1
    } else {
        0
    }
}

/// JSON オブジェクトから HEVC の共通フィールドを構築する
///
/// 幅・高さと `hvcC` を返す。呼び出し側が `hev1` / `hvc1` の型に包む。
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
    kind: &str,
) -> Result<(u16, u16, HvccBox), nojson::JsonParseError> {
    let nalu_arrays_value = value.to_member("naluArrays")?.required()?;
    let mut nalu_arrays = Vec::new();
    // hvcC の numOfArrays は 8 ビット
    if nalu_arrays_value.to_array()?.count() > 255 {
        return Err(crate::boxes::invalid_member(
            value,
            "naluArrays",
            "naluArrays must contain at most 255 arrays",
        ));
    }
    for nalu_array in nalu_arrays_value.to_array()? {
        let nal_unit_type: Uint<u8, 6> = crate::boxes::uint_u8_member(nalu_array, "naluType")?;
        // hvcC の numNalus は 16 ビット。変換で大きな Vec を確保する前に検査する
        let units_value = nalu_array.to_member("units")?.required()?;
        if units_value.to_array()?.count() > usize::from(u16::MAX) {
            return Err(units_value.invalid("units must contain at most 65535 NALUs"));
        }
        let nalus: Vec<Vec<u8>> = units_value.try_into()?;
        // hvcC の NALU 長は 16 ビットであり、それ以上の要素はエンコードできない
        if nalus.iter().any(|nalu| nalu.len() > usize::from(u16::MAX)) {
            return Err(nalu_array
                .to_member("units")
                .and_then(|member| member.required())
                .map(|value| value.invalid("each NALU must be at most 65535 bytes"))
                .unwrap_or_else(|error| error));
        }
        let array_completeness = match nalu_array.to_member("arrayCompleteness")?.optional() {
            None => default_array_completeness(kind, nal_unit_type.get()),
            Some(member) if member.kind() == nojson::JsonValueKind::Null => {
                default_array_completeness(kind, nal_unit_type.get())
            }
            Some(member) => {
                let completeness: u8 = member.try_into()?;
                if completeness > 1 {
                    return Err(member.invalid("arrayCompleteness must be 0 or 1"));
                }
                // hvc1 はパラメータセット（VPS / SPS / PPS）だけ 1 が必須で、
                // それ以外の配列は 0 が必須である。hev1 は既定値 0 で 1 も許される
                if kind == "hvc1" {
                    let expected = default_array_completeness(kind, nal_unit_type.get());
                    if completeness != expected {
                        return Err(member.invalid(if expected == 0 {
                            "arrayCompleteness must be 0 for hvc1 non-parameter-set arrays"
                        } else {
                            "arrayCompleteness must be 1 for hvc1 parameter set arrays"
                        }));
                    }
                }
                completeness
            }
        };
        nalu_arrays.push(HvccNalUintArray {
            array_completeness: Uint::new(array_completeness),
            nal_unit_type,
            nalus,
        });
    }

    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let general_profile_space: Uint<u8, 2, 6> =
        crate::boxes::uint_u8_member(value, "generalProfileSpace")?;
    let general_tier_flag: Uint<u8, 1, 5> = crate::boxes::uint_u8_member(value, "generalTierFlag")?;
    let general_profile_idc: Uint<u8, 5, 0> =
        crate::boxes::uint_u8_member(value, "generalProfileIdc")?;
    let general_profile_compatibility_flags: u32 = value
        .to_member("generalProfileCompatibilityFlags")?
        .required()?
        .try_into()?;
    let general_constraint_indicator_flags: Uint<u64, 48> =
        crate::boxes::uint_u64_member(value, "generalConstraintIndicatorFlags")?;
    let general_level_idc: u8 = value.to_member("generalLevelIdc")?.required()?.try_into()?;
    let chroma_format_idc: Uint<u8, 2> = crate::boxes::uint_u8_member(value, "chromaFormatIdc")?;
    let bit_depth_luma_minus8: Uint<u8, 3> =
        crate::boxes::uint_u8_member(value, "bitDepthLumaMinus8")?;
    let bit_depth_chroma_minus8: Uint<u8, 3> =
        crate::boxes::uint_u8_member(value, "bitDepthChromaMinus8")?;
    let min_spatial_segmentation_idc: Uint<u16, 12> =
        crate::boxes::uint_u16_member(value, "minSpatialSegmentationIdc")?;
    let parallelism_type: Uint<u8, 2> = crate::boxes::uint_u8_member(value, "parallelismType")?;
    let avg_frame_rate: u16 = value.to_member("avgFrameRate")?.required()?.try_into()?;
    let constant_frame_rate: Uint<u8, 2, 6> =
        crate::boxes::uint_u8_member(value, "constantFrameRate")?;
    let num_temporal_layers: Uint<u8, 3, 3> =
        crate::boxes::uint_u8_member(value, "numTemporalLayers")?;
    let temporal_id_nested: Uint<u8, 1, 2> =
        crate::boxes::uint_u8_member(value, "temporalIdNested")?;
    let length_size_minus_one: Uint<u8, 2, 0> =
        crate::boxes::uint_u8_member(value, "lengthSizeMinusOne")?;
    // 2 は予約値であり、NAL ユニット長として解釈できない
    if length_size_minus_one.get() == 2 {
        return Err(crate::boxes::invalid_member(
            value,
            "lengthSizeMinusOne",
            "lengthSizeMinusOne must be 0, 1, or 3",
        ));
    }
    // 3 は予約値
    if constant_frame_rate.get() == 3 {
        return Err(crate::boxes::invalid_member(
            value,
            "constantFrameRate",
            "constantFrameRate must be 0, 1, or 2",
        ));
    }

    let hvcc_box = HvccBox {
        general_profile_space,
        general_tier_flag,
        general_profile_idc,
        general_profile_compatibility_flags,
        general_constraint_indicator_flags,
        general_level_idc,
        min_spatial_segmentation_idc,
        parallelism_type,
        chroma_format_idc,
        bit_depth_luma_minus8,
        bit_depth_chroma_minus8,
        avg_frame_rate,
        constant_frame_rate,
        num_temporal_layers,
        temporal_id_nested,
        length_size_minus_one,
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
            json.contains(r#""naluArrays":[{"naluType":32,"arrayCompleteness":0,"units":[[64,1,12,1]]},{"naluType":33,"arrayCompleteness":0,"units":[[66,1,1,1]]},{"naluType":34,"arrayCompleteness":0,"units":[[68,1,0]]}]"#),
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
        let (width, height, hvcc_box) = parse_json(json.value(), "hev1").expect("有効な HEVC JSON");

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
            parse_json(json.value(), "hev1").is_err(),
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
                parse_json(json.value(), "hev1").is_err(),
                "{name} 欠落時はパース失敗すること"
            );
        }
    }

    #[test]
    fn test_json_to_hevc_empty_nalu_element_roundtrip() {
        let json_str = build_test_json("hev1", r#"[{"naluType": 32, "units": [[]]}]"#, None);

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (width, height, hvcc_box) =
            parse_json(json.value(), "hev1").expect("空 NALU 要素の JSON");
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
        let (_, _, hvcc_box) = parse_json(json.value(), "hev1").expect("有効な JSON");
        assert_eq!(hvcc_box.nalu_arrays[0].nalus, vec![vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn test_json_to_hevc_empty_nalu_arrays() {
        let json_str = build_test_json("hev1", "[]", None);

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (width, height, hvcc_box) = parse_json(json.value(), "hev1").expect("有効な JSON");
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

    #[test]
    fn test_hvc1_defaults_array_completeness_to_one() {
        // hvc1 は VPS / SPS / PPS を hvcC に完全な形で持つため、
        // arrayCompleteness が省略された場合は 1 を補う
        let json_str = build_test_json(
            "hvc1",
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]},
                {"naluType": 39, "units": [[5, 1]]}
            ]"#,
            None,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (_, _, hvcc_box) = parse_json(json.value(), "hvc1").expect("有効な hvc1 JSON");

        assert_eq!(hvcc_box.nalu_arrays[0].array_completeness.get(), 1);
        assert_eq!(hvcc_box.nalu_arrays[1].array_completeness.get(), 1);
        assert_eq!(hvcc_box.nalu_arrays[2].array_completeness.get(), 1);
        // パラメータセット以外は 0 のまま
        assert_eq!(hvcc_box.nalu_arrays[3].array_completeness.get(), 0);
    }

    #[test]
    fn test_array_completeness_must_match_the_kind() {
        // hvc1 のパラメータセットは 1 が必須。明示された 1 はそのまま使う
        let json_str = build_test_json(
            "hvc1",
            r#"[{"naluType": 32, "arrayCompleteness": 1, "units": [[64, 1, 12, 1]]}]"#,
            None,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let (_, _, hvcc_box) = parse_json(json.value(), "hvc1").expect("有効な hvc1 JSON");
        assert_eq!(hvcc_box.nalu_arrays[0].array_completeness.get(), 1);

        // hvc1 のパラメータセットに 0 は指定できない
        let json_str = build_test_json(
            "hvc1",
            r#"[{"naluType": 32, "arrayCompleteness": 0, "units": [[64, 1, 12, 1]]}]"#,
            None,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hvc1").is_err());

        // hvc1 の非パラメータセットに 1 は指定できない
        let json_str = build_test_json(
            "hvc1",
            r#"[{"naluType": 39, "arrayCompleteness": 1, "units": [[5, 1]]}]"#,
            None,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hvc1").is_err());

        // hev1 は 0 / 1 のどちらも受理する
        for completeness in [0, 1] {
            let json_str = build_test_json(
                "hev1",
                &format!(
                    r#"[{{"naluType": 32, "arrayCompleteness": {completeness}, "units": [[64, 1, 12, 1]]}}]"#
                ),
                None,
            );
            let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
            let (_, _, hvcc_box) =
                parse_json(json.value(), "hev1").expect("hev1 では 0 / 1 を受理すること");
            assert_eq!(
                hvcc_box.nalu_arrays[0].array_completeness.get(),
                completeness
            );
        }

        // 2 はどちらでも拒否する
        let json_str = build_test_json(
            "hvc1",
            r#"[{"naluType": 32, "arrayCompleteness": 2, "units": [[64, 1, 12, 1]]}]"#,
            None,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hvc1").is_err());
    }

    /// JSON へ出力して再パースした結果が元の値と一致することを検証する
    #[test]
    fn test_hevc_json_roundtrip() {
        let (visual, hvcc_box) = test_hvcc_box();

        let json = nojson::json(|f| fmt_json(f, "hev1", &visual, &hvcc_box)).to_string();
        let raw = nojson::RawJson::parse(&json).expect("有効な JSON");
        let (width, height, parsed_hvcc_box) =
            parse_json(raw.value(), "hev1").expect("有効な JSON");

        assert_eq!(
            crate::boxes::build_visual_sample_entry_fields(width, height),
            visual
        );
        assert_eq!(parsed_hvcc_box, hvcc_box);
    }

    /// ビット幅を超える値は、隣接フィールドを壊す前に拒否すること
    #[test]
    fn test_json_to_hevc_rejects_out_of_range_bits() {
        let base = build_test_json("hev1", r#"[{"naluType":32,"units":[[1]]}]"#, None);
        let cases = [
            (r#""generalProfileSpace": 0"#, r#""generalProfileSpace": 4"#),
            (r#""generalTierFlag": 0"#, r#""generalTierFlag": 2"#),
            (r#""generalProfileIdc": 2"#, r#""generalProfileIdc": 32"#),
            (
                r#""generalConstraintIndicatorFlags": 193514046488576"#,
                r#""generalConstraintIndicatorFlags": 281474976710656"#,
            ),
            (r#""chromaFormatIdc": 1"#, r#""chromaFormatIdc": 4"#),
            (r#""bitDepthLumaMinus8": 0"#, r#""bitDepthLumaMinus8": 8"#),
            (
                r#""minSpatialSegmentationIdc": 0"#,
                r#""minSpatialSegmentationIdc": 4096"#,
            ),
            (r#""numTemporalLayers": 1"#, r#""numTemporalLayers": 8"#),
            (r#""lengthSizeMinusOne": 3"#, r#""lengthSizeMinusOne": 4"#),
            (r#""lengthSizeMinusOne": 3"#, r#""lengthSizeMinusOne": 2"#),
            (r#""constantFrameRate": 0"#, r#""constantFrameRate": 3"#),
        ];
        for (from, to) in cases {
            let json_str = base.replace(from, to);
            let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
            assert!(
                parse_json(json.value(), "hev1").is_err(),
                "{to} は拒否されること"
            );
        }

        // naluType は 6 ビット
        let json_str = build_test_json("hev1", r#"[{"naluType":64,"units":[[1]]}]"#, None);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hev1").is_err());
    }

    /// NALU 配列数の上限を parse 時に検査すること
    #[test]
    fn test_json_to_hevc_rejects_too_many_nalu_arrays() {
        let arrays = (0..256)
            .map(|_| r#"{"naluType":32,"arrayCompleteness":0,"units":[[1]]}"#)
            .collect::<Vec<_>>()
            .join(",");
        let json_str = build_test_json("hev1", &format!("[{arrays}]"), None);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hev1").is_err());

        // 上限ちょうどは受理する
        let arrays = (0..255)
            .map(|_| r#"{"naluType":32,"arrayCompleteness":0,"units":[[1]]}"#)
            .collect::<Vec<_>>()
            .join(",");
        let json_str = build_test_json("hev1", &format!("[{arrays}]"), None);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value(), "hev1").is_ok());
    }

    /// 1 配列あたりの NALU 数の上限を parse 時に検査すること
    ///
    /// 65536 要素の JSON を構築するため Miri では実行しない。
    #[test]
    #[cfg(not(miri))]
    fn test_json_to_hevc_rejects_too_many_units() {
        let units = (0..65536).map(|_| "[0]").collect::<Vec<_>>().join(",");
        let json_str = build_test_json(
            "hev1",
            &format!(r#"[{{"naluType":32,"arrayCompleteness":0,"units":[{units}]}}]"#),
            None,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let error = parse_json(json.value(), "hev1").expect_err("上限超過は拒否されること");
        assert!(error.to_string().contains("at most 65535"), "{error}");
    }
}
