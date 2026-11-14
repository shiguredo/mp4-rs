//! AVC サンプルエントリー（`avc1`）の JSON 変換を提供するモジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{Avc1Box, AvccBox};

/// AVC1（H.264）サンプルエントリーを JSON に変換する
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Avc1Box) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "avc1")?;
        f.member("width", data.visual.width)?;
        f.member("height", data.visual.height)?;
        f.member("avcProfileIndication", data.avcc_box.avc_profile_indication)?;
        f.member("profileCompatibility", data.avcc_box.profile_compatibility)?;
        f.member("avcLevelIndication", data.avcc_box.avc_level_indication)?;
        f.member(
            "lengthSizeMinusOne",
            data.avcc_box.length_size_minus_one.get(),
        )?;
        // コアでは `None` が「JSON にキーが無い」に対応するため、
        // `Some` のときだけキーを出力する
        if let Some(chroma_format) = data.avcc_box.chroma_format {
            f.member("chromaFormat", chroma_format.get())?;
        }
        if let Some(bit_depth_luma_minus8) = data.avcc_box.bit_depth_luma_minus8 {
            f.member("bitDepthLumaMinus8", bit_depth_luma_minus8.get())?;
        }
        if let Some(bit_depth_chroma_minus8) = data.avcc_box.bit_depth_chroma_minus8 {
            f.member("bitDepthChromaMinus8", bit_depth_chroma_minus8.get())?;
        }
        // `sps_list` / `pps_list` はどちらも `Vec<Vec<u8>>` なので、
        // 数値の配列の配列としてそのまま出力できる
        f.member("sps", data.avcc_box.sps_list.as_slice())?;
        f.member("pps", data.avcc_box.pps_list.as_slice())
    })
}

/// JSON から AVC1 サンプルエントリーに変換する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Avc1Box, nojson::JsonParseError> {
    let sps_list: Vec<Vec<u8>> = value.to_member("sps")?.required()?.try_into()?;
    let pps_list: Vec<Vec<u8>> = value.to_member("pps")?.required()?.try_into()?;
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
    let length_size_minus_one: Uint<u8, 2> =
        crate::boxes::uint_u8_member(value, "lengthSizeMinusOne")?;
    // キーが無いオプションフィールドは `None` になる
    let chroma_format: Option<Uint<u8, 2>> =
        crate::boxes::optional_uint_u8_member(value, "chromaFormat")?;
    let bit_depth_luma_minus8: Option<Uint<u8, 3>> =
        crate::boxes::optional_uint_u8_member(value, "bitDepthLumaMinus8")?;
    let bit_depth_chroma_minus8: Option<Uint<u8, 3>> =
        crate::boxes::optional_uint_u8_member(value, "bitDepthChromaMinus8")?;

    Ok(Avc1Box {
        visual: crate::boxes::build_visual_sample_entry_fields(width, height),
        avcc_box: AvccBox {
            avc_profile_indication,
            profile_compatibility,
            avc_level_indication,
            length_size_minus_one,
            sps_list,
            pps_list,
            chroma_format,
            bit_depth_luma_minus8,
            bit_depth_chroma_minus8,
            // JSON では SPS 拡張を扱わないため空にする
            sps_ext_list: Vec::new(),
        },
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::boxes::VisualSampleEntryFields;

    /// テスト用の SPS バイト列
    const SPS: &[u8] = &[0x67, 0x64, 0x00, 0x28];

    /// テスト用の PPS バイト列
    const PPS: &[u8] = &[0x68, 0xee, 0x3c, 0x80];

    /// テスト用の AVC1 サンプルエントリーを構築する
    ///
    /// オプションフィールドは `None` にしておき、必要なテストで個別に設定する
    fn test_avc1_box() -> Avc1Box {
        Avc1Box {
            visual: crate::boxes::build_visual_sample_entry_fields(1920, 1080),
            avcc_box: AvccBox {
                avc_profile_indication: 100,
                profile_compatibility: 0,
                avc_level_indication: 40,
                length_size_minus_one: Uint::new(3),
                sps_list: vec![SPS.to_vec()],
                pps_list: vec![PPS.to_vec()],
                chroma_format: None,
                bit_depth_luma_minus8: None,
                bit_depth_chroma_minus8: None,
                sps_ext_list: Vec::new(),
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_avc1_to_json() {
        let sample_entry = test_avc1_box();

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"avc1""#),
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
            json.contains(r#""avcProfileIndication":100"#),
            "avcProfileIndication が出力されること: {json}"
        );
        assert!(
            json.contains(r#""profileCompatibility":0"#),
            "profileCompatibility が出力されること: {json}"
        );
        assert!(
            json.contains(r#""avcLevelIndication":40"#),
            "avcLevelIndication が出力されること: {json}"
        );
        assert!(
            json.contains(r#""lengthSizeMinusOne":3"#),
            "lengthSizeMinusOne が出力されること: {json}"
        );
        assert!(
            json.contains(r#""sps":[[103,100,0,40]]"#),
            "sps が数値配列の配列として出力されること: {json}"
        );
        assert!(
            json.contains(r#""pps":[[104,238,60,128]]"#),
            "pps が数値配列の配列として出力されること: {json}"
        );
        // コアで `None` のフィールドはキーごと出力しない
        assert!(
            !json.contains("chromaFormat"),
            "chromaFormat が None のときは出力されないこと: {json}"
        );
        assert!(
            !json.contains("bitDepthLumaMinus8"),
            "bitDepthLumaMinus8 が None のときは出力されないこと: {json}"
        );
        assert!(
            !json.contains("bitDepthChromaMinus8"),
            "bitDepthChromaMinus8 が None のときは出力されないこと: {json}"
        );
    }

    #[test]
    fn test_avc1_to_json_with_optional_fields() {
        let mut sample_entry = test_avc1_box();
        sample_entry.avcc_box.chroma_format = Some(Uint::new(1));
        sample_entry.avcc_box.bit_depth_luma_minus8 = Some(Uint::new(2));
        sample_entry.avcc_box.bit_depth_chroma_minus8 = Some(Uint::new(2));

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""chromaFormat":1"#),
            "chromaFormat が Some のときは出力されること: {json}"
        );
        assert!(
            json.contains(r#""bitDepthLumaMinus8":2"#),
            "bitDepthLumaMinus8 が Some のときは出力されること: {json}"
        );
        assert!(
            json.contains(r#""bitDepthChromaMinus8":2"#),
            "bitDepthChromaMinus8 が Some のときは出力されること: {json}"
        );
    }

    #[test]
    fn test_json_to_avc1() {
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[103, 100, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な avc1 JSON");

        assert_eq!(sample_entry.visual.width, 1920);
        assert_eq!(sample_entry.visual.height, 1080);
        assert_eq!(sample_entry.avcc_box.avc_profile_indication, 100);
        assert_eq!(sample_entry.avcc_box.profile_compatibility, 0);
        assert_eq!(sample_entry.avcc_box.avc_level_indication, 40);
        assert_eq!(sample_entry.avcc_box.length_size_minus_one.get(), 3);
        assert_eq!(sample_entry.avcc_box.sps_list, vec![vec![103, 100, 0, 40]]);
        assert_eq!(
            sample_entry.avcc_box.pps_list,
            vec![vec![104, 238, 60, 128]]
        );
        // キーが無いオプションフィールドは `None` になる
        assert_eq!(sample_entry.avcc_box.chroma_format, None);
        assert_eq!(sample_entry.avcc_box.bit_depth_luma_minus8, None);
        assert_eq!(sample_entry.avcc_box.bit_depth_chroma_minus8, None);
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
    fn test_json_to_avc1_with_optional_fields() {
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "chromaFormat": 1, "bitDepthLumaMinus8": 2, "bitDepthChromaMinus8": 2, "sps": [[103, 100, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な avc1 JSON");

        assert_eq!(sample_entry.avcc_box.chroma_format, Some(Uint::new(1)));
        assert_eq!(
            sample_entry.avcc_box.bit_depth_luma_minus8,
            Some(Uint::new(2))
        );
        assert_eq!(
            sample_entry.avcc_box.bit_depth_chroma_minus8,
            Some(Uint::new(2))
        );
    }

    /// 空の SPS / PPS 要素を含む JSON を parse → 再出力する往復テスト
    ///
    /// `sps_list` / `pps_list` は `Vec<Vec<u8>>` なので空要素をそのまま保持できる。
    /// 空配列要素が `[[]]` として再出力されることを検証する
    #[test]
    fn test_json_to_avc1_empty_nalu_element_roundtrip() {
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[]], "pps": [[]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("空 NALU 要素の avc1 JSON");

        assert_eq!(sample_entry.avcc_box.sps_list, vec![Vec::new()]);
        assert_eq!(sample_entry.avcc_box.pps_list, vec![Vec::new()]);

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""sps":[[]]"#),
            "空 SPS 要素が [[]] として再出力されること: {out}"
        );
        assert!(
            out.contains(r#""pps":[[]]"#),
            "空 PPS 要素が [[]] として再出力されること: {out}"
        );
    }

    /// JSON へ出力して再パースした結果が元の値と一致することを検証する
    #[test]
    fn test_avc1_json_roundtrip() {
        let sample_entry = test_avc1_box();

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        let raw = nojson::RawJson::parse(&json).expect("有効な JSON");
        let parsed = parse_json(raw.value()).expect("有効な avc1 JSON");

        assert_eq!(
            parsed, sample_entry,
            "JSON へ出力して再パースした結果が元の値と一致すること"
        );
    }

    #[test]
    fn test_json_to_avc1_rejects_missing_width_after_sps_pps() {
        // sps / pps を先に読み、その後に width を読む。必須フィールドが
        // 欠落していれば、その時点でパースが失敗することを確認する
        let json_str = r#"{"kind": "avc1", "height": 1080, "avcProfileIndication": 100, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[103, 100, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json(json.value());
        assert!(result.is_err(), "width 欠落時はパース失敗すること");
    }

    /// ビット幅を超える値は、隣接フィールドを壊す前に拒否すること
    #[test]
    fn test_json_to_avc1_rejects_out_of_range_bits() {
        const BASE: &str = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":100,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"chromaFormat":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"sps":[[1]],"pps":[[2]]}"#;
        let cases = [
            (r#""lengthSizeMinusOne":3"#, r#""lengthSizeMinusOne":4"#),
            (r#""chromaFormat":1"#, r#""chromaFormat":4"#),
            (r#""bitDepthLumaMinus8":0"#, r#""bitDepthLumaMinus8":8"#),
            (r#""bitDepthChromaMinus8":0"#, r#""bitDepthChromaMinus8":8"#),
        ];
        for (from, to) in cases {
            let json_str = BASE.replace(from, to);
            let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
            assert!(parse_json(json.value()).is_err(), "{to} は拒否されること");
        }
    }
}
