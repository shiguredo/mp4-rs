//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（hev1 用）

use c_api::boxes::Mp4SampleEntryHev1;

use crate::boxes::{
    HevcNaluArrays, HevcSampleEntryAllocated, free_hevc_sample_entry_fields,
    parse_json_hevc_sample_entry_fields,
};

/// HEV1（H.265/HEVC）サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_hev1(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryHev1,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "hev1")?;
        f.member("width", data.width)?;
        f.member("height", data.height)?;
        f.member("generalProfileSpace", data.general_profile_space)?;
        f.member("generalTierFlag", data.general_tier_flag)?;
        f.member("generalProfileIdc", data.general_profile_idc)?;
        f.member(
            "generalProfileCompatibilityFlags",
            data.general_profile_compatibility_flags,
        )?;
        f.member(
            "generalConstraintIndicatorFlags",
            data.general_constraint_indicator_flags,
        )?;
        f.member("generalLevelIdc", data.general_level_idc)?;
        f.member("chromaFormatIdc", data.chroma_format_idc)?;
        f.member("bitDepthLumaMinus8", data.bit_depth_luma_minus8)?;
        f.member("bitDepthChromaMinus8", data.bit_depth_chroma_minus8)?;
        f.member(
            "minSpatialSegmentationIdc",
            data.min_spatial_segmentation_idc,
        )?;
        f.member("parallelismType", data.parallelism_type)?;
        f.member("avgFrameRate", data.avg_frame_rate)?;
        f.member("constantFrameRate", data.constant_frame_rate)?;
        f.member("numTemporalLayers", data.num_temporal_layers)?;
        f.member("temporalIdNested", data.temporal_id_nested)?;
        f.member("lengthSizeMinusOne", data.length_size_minus_one)?;
        f.member(
            "naluArrays",
            HevcNaluArrays {
                nalu_types: data.nalu_types,
                nalu_counts: data.nalu_counts,
                nalu_data: data.nalu_data,
                nalu_sizes: data.nalu_sizes,
                nalu_array_count: data.nalu_array_count,
            },
        )
    })
}

/// JSON から Mp4SampleEntryHev1 に変換する
pub fn parse_json_mp4_sample_entry_hev1(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryHev1, nojson::JsonParseError> {
    Ok(hevc_fields_to_hev1(parse_json_hevc_sample_entry_fields(
        value,
    )?))
}

/// HEV1 サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_hev1()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_hev1_free(entry: &mut Mp4SampleEntryHev1) {
    free_hevc_sample_entry_fields(
        &mut entry.nalu_array_count,
        &mut entry.nalu_types,
        &mut entry.nalu_counts,
        &mut entry.nalu_data,
        &mut entry.nalu_sizes,
    );
}

/// 共通フィールドを `Mp4SampleEntryHev1` へ写し替える
fn hevc_fields_to_hev1(fields: HevcSampleEntryAllocated) -> Mp4SampleEntryHev1 {
    Mp4SampleEntryHev1 {
        width: fields.width,
        height: fields.height,
        general_profile_space: fields.general_profile_space,
        general_tier_flag: fields.general_tier_flag,
        general_profile_idc: fields.general_profile_idc,
        general_profile_compatibility_flags: fields.general_profile_compatibility_flags,
        general_constraint_indicator_flags: fields.general_constraint_indicator_flags,
        general_level_idc: fields.general_level_idc,
        chroma_format_idc: fields.chroma_format_idc,
        bit_depth_luma_minus8: fields.bit_depth_luma_minus8,
        bit_depth_chroma_minus8: fields.bit_depth_chroma_minus8,
        min_spatial_segmentation_idc: fields.min_spatial_segmentation_idc,
        parallelism_type: fields.parallelism_type,
        avg_frame_rate: fields.avg_frame_rate,
        constant_frame_rate: fields.constant_frame_rate,
        num_temporal_layers: fields.num_temporal_layers,
        temporal_id_nested: fields.temporal_id_nested,
        length_size_minus_one: fields.length_size_minus_one,
        nalu_array_count: fields.nalu_array_count,
        nalu_types: fields.nalu_types,
        nalu_counts: fields.nalu_counts,
        nalu_data: fields.nalu_data,
        nalu_sizes: fields.nalu_sizes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boxes::build_hevc_test_json;

    #[test]
    fn test_hev1_to_json() {
        static VPS: &[u8] = &[0x40, 0x01, 0x0c, 0x01];
        static SPS: &[u8] = &[0x42, 0x01, 0x01, 0x01];
        static PPS: &[u8] = &[0x44, 0x01, 0x00];

        // NALU 配列を構築: VPS, SPS, PPS の順序で格納
        let nalu_types = [32u8, 33u8, 34u8]; // VPS=32, SPS=33, PPS=34
        let nalu_counts = [1u32, 1u32, 1u32];
        let mut nalu_data = Vec::new();
        let mut nalu_sizes_vec = Vec::new();

        nalu_data.push(VPS.as_ptr());
        nalu_sizes_vec.push(VPS.len() as u32);
        nalu_data.push(SPS.as_ptr());
        nalu_sizes_vec.push(SPS.len() as u32);
        nalu_data.push(PPS.as_ptr());
        nalu_sizes_vec.push(PPS.len() as u32);

        let sample_entry = Mp4SampleEntryHev1 {
            width: 1920,
            height: 1080,
            general_profile_space: 0,
            general_tier_flag: 0,
            general_profile_idc: 2,
            general_profile_compatibility_flags: 0x60000000,
            general_constraint_indicator_flags: 0xb0000000_00000000,
            general_level_idc: 120,
            chroma_format_idc: 1,
            bit_depth_luma_minus8: 0,
            bit_depth_chroma_minus8: 0,
            min_spatial_segmentation_idc: 0,
            parallelism_type: 0,
            avg_frame_rate: 0,
            constant_frame_rate: 0,
            num_temporal_layers: 1,
            temporal_id_nested: 0,
            length_size_minus_one: 3,
            nalu_array_count: 3,
            nalu_types: nalu_types.as_ptr(),
            nalu_counts: nalu_counts.as_ptr(),
            nalu_data: nalu_data.as_ptr(),
            nalu_sizes: nalu_sizes_vec.as_ptr(),
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_hev1(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"hev1""#));
        assert!(json.contains(r#""width":1920"#));
        assert!(json.contains(r#""height":1080"#));
        assert!(json.contains(r#""generalProfileIdc":2"#));
        assert!(json.contains(r#""generalLevelIdc":120"#));
        assert!(json.contains(r#""lengthSizeMinusOne":3"#));
        assert!(json.contains(r#""naluArrays":"#));
    }

    #[test]
    fn test_json_to_hev1() {
        let json_str = build_hevc_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]"#,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_hev1(json.value()).expect("有効な hev1 JSON");

        assert_eq!(sample_entry.width, 1920);
        assert_eq!(sample_entry.height, 1080);
        assert_eq!(sample_entry.general_profile_idc, 2);
        assert_eq!(sample_entry.general_level_idc, 120);
        assert_eq!(sample_entry.length_size_minus_one, 3);
        assert_eq!(sample_entry.nalu_array_count, 3);

        // メモリ解放
        mp4_sample_entry_hev1_free(&mut sample_entry);
        assert_eq!(sample_entry.nalu_array_count, 0);
        assert!(sample_entry.nalu_types.is_null());
        assert!(sample_entry.nalu_counts.is_null());
        assert!(sample_entry.nalu_data.is_null());
        assert!(sample_entry.nalu_sizes.is_null());
    }

    #[test]
    fn test_json_to_hev1_rejects_missing_width_after_nalu_arrays() {
        // naluArrays は揃っているが後段の必須フィールド width が欠落している。
        // 全フィールドを Rust 型に落としてからメモリ確保する順序なので、
        // この失敗経路では確保処理に到達せず Err だけが返る
        let json_str = build_hevc_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]"#,
        )
        // 既定 JSON から width 行だけを取り除いて欠落ケースを作る
        .replace("            \"width\": 1920,\n", "");

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let result = parse_json_mp4_sample_entry_hev1(json.value());
        assert!(result.is_err(), "width 欠落時はパース失敗すること");
    }

    /// 1 配列に 2 個の NALU を持つ入力（総数 2 > 配列数 1）の parse → free 回帰テスト
    ///
    /// 修正前は `free_array_list` に配列数（1）を渡していたため、確保時の総数（2）と
    /// 食い違い、余剰バッファのリークと layout 不一致の `dealloc` を引き起こしていた
    #[test]
    fn test_json_to_hev1_free_more_nalus_than_arrays() {
        let json_str = build_hevc_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[1, 2], [3, 4]]}
            ]"#,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_hev1(json.value()).expect("有効な hev1 JSON");

        // 「配列数」は 1、平坦化した「NALU 総数」は 2 になっている
        assert_eq!(sample_entry.nalu_array_count, 1);

        // 回帰の網として parse → free を通す。UB の直接観測は保証しない
        // （wasm クレートは fuzz 対象外で、miri もアラインメント UB により実行できない）
        mp4_sample_entry_hev1_free(&mut sample_entry);
        assert_eq!(sample_entry.nalu_array_count, 0);
        assert!(sample_entry.nalu_types.is_null());
        assert!(sample_entry.nalu_counts.is_null());
        assert!(sample_entry.nalu_data.is_null());
        assert!(sample_entry.nalu_sizes.is_null());
    }

    /// 空配列を含む入力（総数 1 < 配列数 2）の parse → free 回帰テスト
    ///
    /// 修正前は `free_array_list` に配列数（2）を渡していたため、確保時の総数（1）と
    /// 食い違い、確保外の領域を読み出して不正なポインタを `mp4_free` に渡していた
    #[test]
    fn test_json_to_hev1_free_fewer_nalus_than_arrays() {
        let json_str = build_hevc_test_json(
            "hev1",
            r#"[
                {"naluType": 32, "units": [[1, 2]]},
                {"naluType": 33, "units": []}
            ]"#,
        );

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_hev1(json.value()).expect("有効な hev1 JSON");

        // 「配列数」は 2、平坦化した「NALU 総数」は 1 になっている
        assert_eq!(sample_entry.nalu_array_count, 2);

        mp4_sample_entry_hev1_free(&mut sample_entry);
        assert_eq!(sample_entry.nalu_array_count, 0);
        assert!(sample_entry.nalu_types.is_null());
        assert!(sample_entry.nalu_counts.is_null());
        assert!(sample_entry.nalu_data.is_null());
        assert!(sample_entry.nalu_sizes.is_null());
    }

    /// 空 `naluArrays`（`nalu_array_count == 0`）の parse → free 境界値テスト
    ///
    /// 3 つの `allocate_and_copy_bytes` / `allocate_and_copy_array_list` がすべて
    /// `(null, 0)` を返し、free 側の 3 ブロックが `is_null()` で素通りする経路を検証する
    #[test]
    fn test_json_to_hev1_free_empty_nalu_arrays() {
        let json_str = build_hevc_test_json("hev1", "[]");

        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_hev1(json.value()).expect("有効な hev1 JSON");

        // parse 直後: 配列個数 0、各ポインタは null
        assert_eq!(sample_entry.nalu_array_count, 0);
        assert!(sample_entry.nalu_types.is_null());
        assert!(sample_entry.nalu_counts.is_null());
        assert!(sample_entry.nalu_data.is_null());
        assert!(sample_entry.nalu_sizes.is_null());

        // free: 3 ブロックとも `is_null()` で素通りするだけ
        mp4_sample_entry_hev1_free(&mut sample_entry);
        assert_eq!(sample_entry.nalu_array_count, 0);
        assert!(sample_entry.nalu_types.is_null());
        assert!(sample_entry.nalu_counts.is_null());
        assert!(sample_entry.nalu_data.is_null());
        assert!(sample_entry.nalu_sizes.is_null());
    }
}
