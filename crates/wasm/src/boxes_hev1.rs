//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（hev1 用）

use c_api::boxes::Mp4SampleEntryHev1;

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
            NaluArrays {
                nalu_types: data.nalu_types,
                nalu_counts: data.nalu_counts,
                nalu_data: data.nalu_data,
                nalu_sizes: data.nalu_sizes,
                nalu_array_count: data.nalu_array_count,
            },
        )
    })
}

/// HEVC NALU 配列の JSON シリアライズ用構造体
struct NaluArrays {
    nalu_types: *const u8,
    nalu_counts: *const u32,
    nalu_data: *const *const u8,
    nalu_sizes: *const u32,
    nalu_array_count: u32,
}

impl nojson::DisplayJson for NaluArrays {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.array(|f| {
            let mut nalu_index_base = 0u32;
            for i in 0..self.nalu_array_count as usize {
                let nalu_type = unsafe { *self.nalu_types.add(i) };
                let nalu_count = unsafe { *self.nalu_counts.add(i) };

                f.element(nojson::object(|f| {
                    f.member("naluType", nalu_type)?;
                    f.member(
                        "units",
                        nojson::array(|f| {
                            for j in 0..nalu_count {
                                let nalu_index = nalu_index_base + j;
                                let nalu_ptr = unsafe { *self.nalu_data.add(nalu_index as usize) };
                                let nalu_size =
                                    unsafe { *self.nalu_sizes.add(nalu_index as usize) } as usize;
                                let nalu =
                                    unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) };
                                f.element(nalu)?;
                            }
                            Ok(())
                        }),
                    )
                }))?;

                nalu_index_base += nalu_count;
            }
            Ok(())
        })
    }
}

/// JSON から Mp4SampleEntryHev1 に変換する
pub fn parse_json_mp4_sample_entry_hev1(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryHev1, nojson::JsonParseError> {
    // NALU 配列を解析
    let nalu_arrays_value = value.to_member("naluArrays")?.required()?;

    let mut nalu_types_vec = Vec::new();
    let mut nalu_counts_vec = Vec::new();
    let mut nalu_data_vec = Vec::new();

    for nalu_array in nalu_arrays_value.to_array()? {
        // NALU タイプを取得
        let nalu_type: u8 = nalu_array.to_member("naluType")?.required()?.try_into()?;
        nalu_types_vec.push(nalu_type);

        // NALU ユニットを処理
        let units_value = nalu_array.to_member("units")?.required()?;

        let mut nalu_count = 0u32;
        for unit in units_value.to_array()? {
            let nalu_bytes: Vec<u8> = unit.try_into()?;
            nalu_data_vec.push(nalu_bytes);
            nalu_count += 1;
        }
        nalu_counts_vec.push(nalu_count);
    }

    // nalu_types をメモリに割り当ててコピー
    let (nalu_types, _) = crate::boxes::allocate_and_copy_bytes(unsafe {
        std::slice::from_raw_parts(nalu_types_vec.as_ptr(), nalu_types_vec.len())
    });

    // nalu_counts をメモリに割り当ててコピー
    let (nalu_counts, _) = crate::boxes::allocate_and_copy_bytes(unsafe {
        std::slice::from_raw_parts(
            nalu_counts_vec.as_ptr() as *const u8,
            nalu_counts_vec.len() * std::mem::size_of::<u32>(),
        )
    });

    // nalu_data と nalu_sizes を割り当ててコピー
    let (nalu_data, nalu_sizes, _) = crate::boxes::allocate_and_copy_array_list(&nalu_data_vec);

    Ok(Mp4SampleEntryHev1 {
        width: value.to_member("width")?.required()?.try_into()?,
        height: value.to_member("height")?.required()?.try_into()?,
        general_profile_space: value
            .to_member("generalProfileSpace")?
            .required()?
            .try_into()?,
        general_tier_flag: value.to_member("generalTierFlag")?.required()?.try_into()?,
        general_profile_idc: value
            .to_member("generalProfileIdc")?
            .required()?
            .try_into()?,
        general_profile_compatibility_flags: value
            .to_member("generalProfileCompatibilityFlags")?
            .required()?
            .try_into()?,
        general_constraint_indicator_flags: value
            .to_member("generalConstraintIndicatorFlags")?
            .required()?
            .try_into()?,
        general_level_idc: value.to_member("generalLevelIdc")?.required()?.try_into()?,
        chroma_format_idc: value.to_member("chromaFormatIdc")?.required()?.try_into()?,
        bit_depth_luma_minus8: value
            .to_member("bitDepthLumaMinus8")?
            .required()?
            .try_into()?,
        bit_depth_chroma_minus8: value
            .to_member("bitDepthChromaMinus8")?
            .required()?
            .try_into()?,
        min_spatial_segmentation_idc: value
            .to_member("minSpatialSegmentationIdc")?
            .required()?
            .try_into()?,
        parallelism_type: value.to_member("parallelismType")?.required()?.try_into()?,
        avg_frame_rate: value.to_member("avgFrameRate")?.required()?.try_into()?,
        constant_frame_rate: value
            .to_member("constantFrameRate")?
            .required()?
            .try_into()?,
        num_temporal_layers: value
            .to_member("numTemporalLayers")?
            .required()?
            .try_into()?,
        temporal_id_nested: value
            .to_member("temporalIdNested")?
            .required()?
            .try_into()?,
        length_size_minus_one: value
            .to_member("lengthSizeMinusOne")?
            .required()?
            .try_into()?,
        nalu_array_count: nalu_types_vec.len() as u32,
        nalu_types,
        nalu_counts: nalu_counts as *const u32,
        nalu_data,
        nalu_sizes,
    })
}

/// HEV1 サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_hev1()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_hev1_free(entry: &mut Mp4SampleEntryHev1) {
    if !entry.nalu_types.is_null() {
        unsafe {
            crate::mp4_free(entry.nalu_types.cast_mut(), 0);
        }
        entry.nalu_types = std::ptr::null();
    }

    if !entry.nalu_counts.is_null() {
        unsafe {
            crate::mp4_free(entry.nalu_counts.cast_mut() as *mut u8, 0);
        }
        entry.nalu_counts = std::ptr::null();
    }

    if !entry.nalu_data.is_null() {
        unsafe {
            crate::boxes::free_array_list(
                entry.nalu_data as *mut *mut u8,
                entry.nalu_sizes as *mut u32,
                entry.nalu_array_count,
            );
            entry.nalu_data = std::ptr::null();
            entry.nalu_sizes = std::ptr::null();
        }
    }

    entry.nalu_array_count = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let json_str = r#"{
            "kind": "hev1",
            "width": 1920,
            "height": 1080,
            "generalProfileSpace": 0,
            "generalTierFlag": 0,
            "generalProfileIdc": 2,
            "generalProfileCompatibilityFlags": 1610612736,
            "generalConstraintIndicatorFlags": 12682136550675546112,
            "generalLevelIdc": 120,
            "chromaFormatIdc": 1,
            "bitDepthLumaMinus8": 0,
            "bitDepthChromaMinus8": 0,
            "minSpatialSegmentationIdc": 0,
            "parallelismType": 0,
            "avgFrameRate": 0,
            "constantFrameRate": 0,
            "numTemporalLayers": 1,
            "temporalIdNested": 0,
            "lengthSizeMinusOne": 3,
            "naluArrays": [
                {"naluType": 32, "units": [[64, 1, 12, 1]]},
                {"naluType": 33, "units": [[66, 1, 1, 1]]},
                {"naluType": 34, "units": [[68, 1, 0]]}
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_hev1(json.value()).expect("valid hev1 JSON");

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
    }
}
