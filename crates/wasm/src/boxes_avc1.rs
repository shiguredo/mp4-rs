//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（avc1 ボックス用）
use c_api::boxes::Mp4SampleEntryAvc1;

/// AVC1（H.264）サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_avc1(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryAvc1,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "avc1")?;
        f.member("width", data.width)?;
        f.member("height", data.height)?;
        f.member("avcProfileIndication", data.avc_profile_indication)?;
        f.member("profileCompatibility", data.profile_compatibility)?;
        f.member("avcLevelIndication", data.avc_level_indication)?;
        f.member("lengthSizeMinusOne", data.length_size_minus_one)?;
        if data.is_chroma_format_present {
            f.member("chromaFormat", data.chroma_format)?;
        }
        if data.is_bit_depth_luma_minus8_present {
            f.member("bitDepthLumaMinus8", data.bit_depth_luma_minus8)?;
        }
        if data.is_bit_depth_chroma_minus8_present {
            f.member("bitDepthChromaMinus8", data.bit_depth_chroma_minus8)?;
        }
        f.member(
            "sps",
            NaluList {
                data_ptr: data.sps_data,
                sizes_ptr: data.sps_sizes,
                count: data.sps_count,
            },
        )?;
        f.member(
            "pps",
            NaluList {
                data_ptr: data.pps_data,
                sizes_ptr: data.pps_sizes,
                count: data.pps_count,
            },
        )
    })
}

/// JSON から Mp4SampleEntryAvc1 に変換する
pub fn parse_json_mp4_sample_entry_avc1(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryAvc1, nojson::JsonParseError> {
    // パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
    // 確保済みバッファがリークする。まず全フィールドを Rust 型に落としてから
    // 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする

    // フェーズ 1: すべての JSON フィールドを Rust 型に落とす（メモリ確保前）
    let sps_vec: Vec<Vec<u8>> = value
        .to_member("sps")?
        .required()?
        .to_array()?
        .map(|v| v.try_into())
        .collect::<Result<_, _>>()?;
    let pps_vec: Vec<Vec<u8>> = value
        .to_member("pps")?
        .required()?
        .to_array()?
        .map(|v| v.try_into())
        .collect::<Result<_, _>>()?;
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let avc_profile_indication: u8 = value
        .to_member("avcProfileIndication")?
        .required()?
        .try_into()?;
    let profile_compatibility: u8 = value
        .to_member("profileCompatibility")?
        .required()?
        .try_into()?;
    let avc_level_indication: u8 = value
        .to_member("avcLevelIndication")?
        .required()?
        .try_into()?;
    let length_size_minus_one: u8 = value
        .to_member("lengthSizeMinusOne")?
        .required()?
        .try_into()?;
    let is_chroma_format_present = value.to_member("chromaFormat")?.optional().is_some();
    let chroma_format: u8 = value
        .to_member("chromaFormat")?
        .map(|v| v.try_into())?
        .unwrap_or(0);
    let is_bit_depth_luma_minus8_present =
        value.to_member("bitDepthLumaMinus8")?.optional().is_some();
    let bit_depth_luma_minus8: u8 = value
        .to_member("bitDepthLumaMinus8")?
        .map(|v| v.try_into())?
        .unwrap_or(0);
    let is_bit_depth_chroma_minus8_present = value
        .to_member("bitDepthChromaMinus8")?
        .optional()
        .is_some();
    let bit_depth_chroma_minus8: u8 = value
        .to_member("bitDepthChromaMinus8")?
        .map(|v| v.try_into())?
        .unwrap_or(0);

    // フェーズ 2: すべてのパースが成功したときだけメモリを確保する
    let (sps_data, sps_sizes, sps_count) = crate::boxes::allocate_and_copy_array_list(&sps_vec);
    let (pps_data, pps_sizes, pps_count) = crate::boxes::allocate_and_copy_array_list(&pps_vec);

    Ok(Mp4SampleEntryAvc1 {
        width,
        height,
        avc_profile_indication,
        profile_compatibility,
        avc_level_indication,
        length_size_minus_one,
        sps_data,
        sps_sizes,
        sps_count,
        pps_data,
        pps_sizes,
        pps_count,
        is_chroma_format_present,
        chroma_format,
        is_bit_depth_luma_minus8_present,
        bit_depth_luma_minus8,
        is_bit_depth_chroma_minus8_present,
        bit_depth_chroma_minus8,
    })
}

/// AVC1 サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_avc1()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_avc1_free(entry: &mut Mp4SampleEntryAvc1) {
    unsafe {
        crate::boxes::free_array_list(
            entry.sps_data as *mut *mut u8,
            entry.sps_sizes as *mut u32,
            entry.sps_count,
        );
        entry.sps_data = std::ptr::null();
        entry.sps_sizes = std::ptr::null();
        entry.sps_count = 0;

        crate::boxes::free_array_list(
            entry.pps_data as *mut *mut u8,
            entry.pps_sizes as *mut u32,
            entry.pps_count,
        );
        entry.pps_data = std::ptr::null();
        entry.pps_sizes = std::ptr::null();
        entry.pps_count = 0;
    }
}

/// AVC SPS/PPS リストの JSON シリアライズ用構造体
struct NaluList {
    data_ptr: *const *const u8,
    sizes_ptr: *const u32,
    count: u32,
}

impl nojson::DisplayJson for NaluList {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.array(|f| {
            for i in 0..self.count as usize {
                let nalu_ptr = unsafe { *self.data_ptr.add(i) };
                let nalu_size = unsafe { *self.sizes_ptr.add(i) } as usize;
                // パース時に格納されたポインタ／サイズを読む（ここでは確保しない）。
                // 空要素は (null, 0)。allocate_and_copy_array_list はポインタに .0 だけ・
                // サイズに array.len() を使うため、非空要素の確保失敗後は (null, 非ゼロ)
                // も残り得る。いずれも from_raw_parts に null を渡すと UB なのでガードし、
                // その場合は空配列として出力する（フォーマット側ではエラーにはしない）
                let nalu = if nalu_size == 0 || nalu_ptr.is_null() {
                    &[][..]
                } else {
                    unsafe { std::slice::from_raw_parts(nalu_ptr, nalu_size) }
                };
                f.element(nalu)?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avc1_to_json() {
        static SPS: &[u8] = &[0x67, 0x64, 0x00, 0x28];
        static PPS: &[u8] = &[0x68, 0xee, 0x3c, 0x80];

        let sps_data = [SPS.as_ptr()];
        let sps_sizes = [SPS.len() as u32];
        let pps_data = [PPS.as_ptr()];
        let pps_sizes = [PPS.len() as u32];

        let sample_entry = Mp4SampleEntryAvc1 {
            width: 1920,
            height: 1080,
            avc_profile_indication: 100,
            profile_compatibility: 0,
            avc_level_indication: 40,
            length_size_minus_one: 3,
            sps_data: sps_data.as_ptr(),
            sps_sizes: sps_sizes.as_ptr(),
            sps_count: 1,
            pps_data: pps_data.as_ptr(),
            pps_sizes: pps_sizes.as_ptr(),
            pps_count: 1,
            is_chroma_format_present: false,
            chroma_format: 0,
            is_bit_depth_luma_minus8_present: false,
            bit_depth_luma_minus8: 0,
            is_bit_depth_chroma_minus8_present: false,
            bit_depth_chroma_minus8: 0,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_avc1(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"avc1""#));
        assert!(json.contains(r#""width":1920"#));
        assert!(json.contains(r#""height":1080"#));
        assert!(json.contains(r#""avcProfileIndication":100"#));
        assert!(json.contains(r#""profileCompatibility":0"#));
        assert!(json.contains(r#""avcLevelIndication":40"#));
        assert!(json.contains(r#""lengthSizeMinusOne":3"#));
        assert!(json.contains(r#""sps":"#));
        assert!(json.contains(r#""pps":"#));
    }

    #[test]
    fn test_json_to_avc1() {
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[103, 100, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_avc1(json.value()).expect("valid avc1 JSON");

        assert_eq!(sample_entry.width, 1920);
        assert_eq!(sample_entry.height, 1080);
        assert_eq!(sample_entry.avc_profile_indication, 100);
        assert_eq!(sample_entry.avc_level_indication, 40);
        assert_eq!(sample_entry.sps_count, 1);
        assert_eq!(sample_entry.pps_count, 1);

        // メモリ解放
        mp4_sample_entry_avc1_free(&mut sample_entry);
        assert_eq!(sample_entry.sps_count, 0);
    }

    /// 空の SPS / PPS 要素を含む JSON を parse → 再出力する往復テスト
    ///
    /// `allocate_and_copy_array_list` は空要素を `(null, 0)` にする。
    /// `NaluList::fmt` が `from_raw_parts(null, 0)` を呼ぶと UB になるため、
    /// ガード後も空配列要素として再出力できることを検証する
    #[test]
    fn test_json_to_avc1_empty_nalu_element_roundtrip() {
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[]], "pps": [[]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_avc1(json.value()).expect("空 NALU 要素の avc1 JSON");

        assert_eq!(sample_entry.sps_count, 1);
        assert_eq!(sample_entry.pps_count, 1);
        assert_eq!(unsafe { *sample_entry.sps_sizes }, 0);
        assert!(unsafe { (*sample_entry.sps_data).is_null() });
        assert_eq!(unsafe { *sample_entry.pps_sizes }, 0);
        assert!(unsafe { (*sample_entry.pps_data).is_null() });

        let out = nojson::json(|f| fmt_json_mp4_sample_entry_avc1(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""sps":[[]]"#),
            "空 SPS 要素が [[]] として再出力されること: {out}"
        );
        assert!(
            out.contains(r#""pps":[[]]"#),
            "空 PPS 要素が [[]] として再出力されること: {out}"
        );

        mp4_sample_entry_avc1_free(&mut sample_entry);
        assert_eq!(sample_entry.sps_count, 0);
        assert_eq!(sample_entry.pps_count, 0);
    }

    #[test]
    fn test_json_to_avc1_rejects_missing_width_after_sps_pps() {
        // sps / pps は揃っているが後段の必須フィールド width が欠落している。
        // 全フィールドを Rust 型に落としてからメモリ確保する順序なので、
        // この失敗経路では確保処理に到達せず Err だけが返る
        let json_str = r#"{"kind": "avc1", "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[103, 100, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json_mp4_sample_entry_avc1(json.value());
        assert!(result.is_err(), "width 欠落時はパース失敗すること");
    }
}
