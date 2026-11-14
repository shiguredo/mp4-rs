//! AVC サンプルエントリー（`avc1`）の JSON 変換を提供するモジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{Avc1Box, AvccBox};

/// AVC1（H.264）サンプルエントリーを JSON に変換する
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Avc1Box) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "avc1")?;
        crate::boxes::fmt_visual_sample_entry_fields(f, &data.visual)?;
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
        f.member("pps", data.avcc_box.pps_list.as_slice())?;
        f.member("spsExt", data.avcc_box.sps_ext_list.as_slice())?;
        crate::boxes::fmt_unknown_boxes(f, &data.unknown_boxes)
    })
}

/// JSON から AVC1 サンプルエントリーに変換する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Avc1Box, nojson::JsonParseError> {
    let visual = crate::boxes::parse_visual_sample_entry_fields(value)?;
    let sps_list: Vec<Vec<u8>> = value.to_member("sps")?.required()?.try_into()?;
    let pps_list: Vec<Vec<u8>> = value.to_member("pps")?.required()?.try_into()?;
    // `sps_ext_list` は省略可能
    let sps_ext_list: Vec<Vec<u8>> = match value.to_member("spsExt")?.optional() {
        None => Vec::new(),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Vec::new(),
        Some(member) => member.try_into()?,
    };
    // avcC の個数は SPS が 5 ビット、PPS が 8 ビットで表現される
    if sps_list.len() > 31 {
        return Err(crate::boxes::invalid_member(
            value,
            "sps",
            "sps must contain at most 31 entries",
        ));
    }
    if pps_list.len() > 255 {
        return Err(crate::boxes::invalid_member(
            value,
            "pps",
            "pps must contain at most 255 entries",
        ));
    }
    // avcC の NALU 長は 16 ビットであり、それ以上の要素はエンコードできない
    for nalu in &sps_list {
        if nalu.len() > usize::from(u16::MAX) {
            return Err(crate::boxes::invalid_member(
                value,
                "sps",
                "each SPS must be at most 65535 bytes",
            ));
        }
    }
    for nalu in &pps_list {
        if nalu.len() > usize::from(u16::MAX) {
            return Err(crate::boxes::invalid_member(
                value,
                "pps",
                "each PPS must be at most 65535 bytes",
            ));
        }
    }
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
    // 2 は予約値であり、NAL ユニット長として解釈できない
    if length_size_minus_one.get() == 2 {
        return Err(crate::boxes::invalid_member(
            value,
            "lengthSizeMinusOne",
            "lengthSizeMinusOne must be 0, 1, or 3",
        ));
    }
    // キーが無いオプションフィールドは `None` になる
    let chroma_format: Option<Uint<u8, 2>> =
        crate::boxes::optional_uint_u8_member(value, "chromaFormat")?;
    let bit_depth_luma_minus8: Option<Uint<u8, 3>> =
        crate::boxes::optional_uint_u8_member(value, "bitDepthLumaMinus8")?;
    let bit_depth_chroma_minus8: Option<Uint<u8, 3>> =
        crate::boxes::optional_uint_u8_member(value, "bitDepthChromaMinus8")?;

    // 66 / 77 / 88 以外のプロファイルは、avcC の拡張フィールドが必須である
    let high_profile = !matches!(avc_profile_indication, 66 | 77 | 88);
    if high_profile && chroma_format.is_none() {
        return Err(crate::boxes::invalid_member(
            value,
            "chromaFormat",
            "chromaFormat is required for this profile",
        ));
    }
    if high_profile && bit_depth_luma_minus8.is_none() {
        return Err(crate::boxes::invalid_member(
            value,
            "bitDepthLumaMinus8",
            "bitDepthLumaMinus8 is required for this profile",
        ));
    }
    if high_profile && bit_depth_chroma_minus8.is_none() {
        return Err(crate::boxes::invalid_member(
            value,
            "bitDepthChromaMinus8",
            "bitDepthChromaMinus8 is required for this profile",
        ));
    }
    // 66 / 77 / 88 では avcC に SPS 拡張を書かないため、指定されても無視される
    if !high_profile && !sps_ext_list.is_empty() {
        return Err(crate::boxes::invalid_member(
            value,
            "spsExt",
            "spsExt is only available for profiles other than 66, 77, and 88",
        ));
    }

    Ok(Avc1Box {
        visual,
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
            sps_ext_list,
        },
        unknown_boxes: crate::boxes::parse_unknown_boxes(value)?,
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
                avc_profile_indication: 66,
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
            json.contains(r#""avcProfileIndication":66"#),
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
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 66, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[103, 66, 0, 40]], "pps": [[104, 238, 60, 128]]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な avc1 JSON");

        assert_eq!(sample_entry.visual.width, 1920);
        assert_eq!(sample_entry.visual.height, 1080);
        assert_eq!(sample_entry.avcc_box.avc_profile_indication, 66);
        assert_eq!(sample_entry.avcc_box.profile_compatibility, 0);
        assert_eq!(sample_entry.avcc_box.avc_level_indication, 40);
        assert_eq!(sample_entry.avcc_box.length_size_minus_one.get(), 3);
        assert_eq!(sample_entry.avcc_box.sps_list, vec![vec![103, 66, 0, 40]]);
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
        let json_str = r#"{"kind": "avc1", "width": 1920, "height": 1080, "avcProfileIndication": 66, "profileCompatibility": 0, "avcLevelIndication": 40, "lengthSizeMinusOne": 3, "sps": [[]], "pps": [[]]}"#;

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

    /// 66 / 77 / 88 では SPS 拡張を指定できないこと
    #[test]
    fn test_json_to_avc1_rejects_sps_ext_for_baseline_profile() {
        let json_str = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":66,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"sps":[[1]],"pps":[[2]],"spsExt":[[3]]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        assert!(parse_json(json.value()).is_err());

        let json_str = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":100,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"chromaFormat":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"sps":[[1]],"pps":[[2]],"spsExt":[[3]]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let parsed = parse_json(json.value()).expect("高プロファイルでは受理されること");
        assert_eq!(parsed.avcc_box.sps_ext_list, vec![vec![3]]);
    }

    /// 未知の子ボックスとコンプレッサー名が往復すること
    #[test]
    fn test_avc1_json_roundtrip_preserves_metadata() {
        let mut sample_entry = test_avc1_box();
        let mut compressorname = [0u8; 32];
        compressorname[0] = 9;
        compressorname[1..10].copy_from_slice(b"test/avc1");
        sample_entry.visual.compressorname = compressorname;
        sample_entry.visual.frame_count = 1;
        sample_entry
            .unknown_boxes
            .push(shiguredo_mp4::boxes::UnknownBox {
                box_type: shiguredo_mp4::BoxType::Normal(*b"pasp"),
                box_size: shiguredo_mp4::BoxSize::U32(16),
                payload: vec![0, 0, 0, 1, 0, 0, 0, 1],
            });

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        let raw = nojson::RawJson::parse(&json).expect("有効な JSON");
        let parsed = parse_json(raw.value()).expect("有効な avc1 JSON");
        assert_eq!(parsed, sample_entry);
    }

    /// 66 / 77 / 88 以外のプロファイルでは avcC の拡張フィールドを必須にすること
    #[test]
    fn test_json_to_avc1_requires_optional_fields_for_high_profile() {
        let without = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":100,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"sps":[[1]],"pps":[[2]]}"#;
        let json = nojson::RawJson::parse(without).expect("有効な JSON");
        assert!(parse_json(json.value()).is_err());

        let with = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":100,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"chromaFormat":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"sps":[[1]],"pps":[[2]]}"#;
        let json = nojson::RawJson::parse(with).expect("有効な JSON");
        assert!(parse_json(json.value()).is_ok());

        // 66 は拡張フィールドなしでもよい
        let baseline = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":66,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"sps":[[1]],"pps":[[2]]}"#;
        let json = nojson::RawJson::parse(baseline).expect("有効な JSON");
        assert!(parse_json(json.value()).is_ok());
    }

    /// ビット幅を超える値は、隣接フィールドを壊す前に拒否すること
    #[test]
    fn test_json_to_avc1_rejects_out_of_range_bits() {
        const BASE: &str = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":100,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3,"chromaFormat":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"sps":[[1]],"pps":[[2]]}"#;
        let cases = [
            (r#""lengthSizeMinusOne":3"#, r#""lengthSizeMinusOne":4"#),
            (r#""lengthSizeMinusOne":3"#, r#""lengthSizeMinusOne":2"#),
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

    /// SPS / PPS の個数上限を parse 時に検査すること
    #[test]
    fn test_json_to_avc1_rejects_too_many_parameter_sets() {
        let prefix = r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":66,"profileCompatibility":0,"avcLevelIndication":40,"lengthSizeMinusOne":3"#;
        let sps = (0..32).map(|_| "[1]").collect::<Vec<_>>().join(",");
        let json_str = format!(r#"{prefix},"sps":[{sps}],"pps":[[2]]}}"#);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(
            parse_json(json.value()).is_err(),
            "SPS 32 個は拒否されること"
        );

        let pps = (0..256).map(|_| "[2]").collect::<Vec<_>>().join(",");
        let json_str = format!(r#"{prefix},"sps":[[1]],"pps":[{pps}]}}"#);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(
            parse_json(json.value()).is_err(),
            "PPS 256 個は拒否されること"
        );

        // 上限ちょうどは受理する
        let sps = (0..31).map(|_| "[1]").collect::<Vec<_>>().join(",");
        let json_str = format!(r#"{prefix},"sps":[{sps}],"pps":[[2]]}}"#);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(
            parse_json(json.value()).is_ok(),
            "SPS 31 個は受理されること"
        );

        let pps = (0..255).map(|_| "[2]").collect::<Vec<_>>().join(",");
        let json_str = format!(r#"{prefix},"sps":[[1]],"pps":[{pps}]}}"#);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(
            parse_json(json.value()).is_ok(),
            "PPS 255 個は受理されること"
        );
    }
}
