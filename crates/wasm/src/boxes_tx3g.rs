//! tx3g サンプルエントリーの JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::boxes::{BoxRecord, FontRecord, FtabBox, StyleRecord, Tx3gBox};

/// tx3g サンプルエントリーを JSON フォーマットする
///
/// `default_style` は入れ子オブジェクト、`ftab` は `font_id` + `font_name` (バイト配列) の
/// オブジェクト配列として書き出す。`font_name` は 3GPP TS 26.245 が文字エンコーディングを
/// 明示していないため UTF-8 を保証しない生バイト列であり、JSON 文字列ではなく数値配列で表現する
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Tx3gBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "tx3g")?;
        f.member("display_flags", data.display_flags)?;
        f.member("horizontal_justification", data.horizontal_justification)?;
        f.member("vertical_justification", data.vertical_justification)?;
        f.member("background_color_rgba", data.background_color_rgba)?;
        // 既存の JSON 表現を変えないよう、BoxRecord は [top, left, bottom, right] の配列で書き出す
        f.member(
            "default_text_box",
            [
                data.default_text_box.top,
                data.default_text_box.left,
                data.default_text_box.bottom,
                data.default_text_box.right,
            ],
        )?;
        f.member(
            "default_style",
            nojson::json(|f| {
                f.object(|f| {
                    f.member("start_char", data.default_style.start_char)?;
                    f.member("end_char", data.default_style.end_char)?;
                    f.member("font_id", data.default_style.font_id)?;
                    f.member("face_style_flags", data.default_style.face_style_flags)?;
                    f.member("font_size", data.default_style.font_size)?;
                    f.member("text_color_rgba", data.default_style.text_color_rgba)
                })
            }),
        )?;
        f.member(
            "ftab",
            nojson::json(|f| {
                f.array(|f| {
                    for entry in &data.ftab_box.entries {
                        f.element(nojson::json(|f| {
                            f.object(|f| {
                                f.member("font_id", entry.font_id)?;
                                f.member("font_name", entry.font_name.as_slice())
                            })
                        }))?;
                    }
                    Ok(())
                })
            }),
        )
    })
}

/// JSON から [`Tx3gBox`] に変換する
///
/// `default_text_box` は `[top, left, bottom, right]` の配列、
/// `default_style` はスタイルを表すオブジェクト、`ftab` は
/// `{ "font_id": u16, "font_name": [u8; N] }` オブジェクトの配列として解釈する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Tx3gBox, nojson::JsonParseError> {
    // default_style は入れ子オブジェクトのため、先に親の値を取得しておく
    let default_style_value = value.to_member("default_style")?.required()?;

    // ftab は `font_id` と `font_name` を持つオブジェクトの配列のため、
    // 要素ごとに FontRecord へ変換する
    // ftab の entry_count は 16 ビット
    if value.to_member("ftab")?.required()?.to_array()?.count() > usize::from(u16::MAX) {
        return Err(crate::boxes::invalid_member(
            value,
            "ftab",
            "ftab must contain at most 65535 entries",
        ));
    }
    let ftab_entries = value
        .to_member("ftab")?
        .required()?
        .to_array()?
        .map(|entry| {
            let font_id: u16 = entry.to_member("font_id")?.required()?.try_into()?;
            let font_name_value = entry.to_member("font_name")?.required()?;
            let font_name: Vec<u8> = font_name_value.try_into()?;
            // FontRecord の font_name_length は 8 ビット
            if font_name.len() > usize::from(u8::MAX) {
                return Err(font_name_value.invalid("font_name must be at most 255 bytes"));
            }
            Ok(FontRecord { font_id, font_name })
        })
        .collect::<Result<Vec<_>, nojson::JsonParseError>>()?;

    let display_flags: u32 = value.to_member("display_flags")?.required()?.try_into()?;
    let horizontal_justification: i8 = value
        .to_member("horizontal_justification")?
        .required()?
        .try_into()?;
    let vertical_justification: i8 = value
        .to_member("vertical_justification")?
        .required()?
        .try_into()?;
    let background_color_rgba: [u8; 4] = value
        .to_member("background_color_rgba")?
        .required()?
        .try_into()?;
    let default_text_box: [i16; 4] = value
        .to_member("default_text_box")?
        .required()?
        .try_into()?;

    let default_style_start_char: u16 = default_style_value
        .to_member("start_char")?
        .required()?
        .try_into()?;
    let default_style_end_char: u16 = default_style_value
        .to_member("end_char")?
        .required()?
        .try_into()?;
    let default_style_font_id: u16 = default_style_value
        .to_member("font_id")?
        .required()?
        .try_into()?;
    let default_style_face_style_flags: u8 = default_style_value
        .to_member("face_style_flags")?
        .required()?
        .try_into()?;
    let default_style_font_size: u8 = default_style_value
        .to_member("font_size")?
        .required()?
        .try_into()?;
    let default_style_text_color_rgba: [u8; 4] = default_style_value
        .to_member("text_color_rgba")?
        .required()?
        .try_into()?;

    Ok(Tx3gBox {
        data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
        display_flags,
        horizontal_justification,
        vertical_justification,
        background_color_rgba,
        default_text_box: BoxRecord {
            top: default_text_box[0],
            left: default_text_box[1],
            bottom: default_text_box[2],
            right: default_text_box[3],
        },
        default_style: StyleRecord {
            start_char: default_style_start_char,
            end_char: default_style_end_char,
            font_id: default_style_font_id,
            face_style_flags: default_style_face_style_flags,
            font_size: default_style_font_size,
            text_color_rgba: default_style_text_color_rgba,
        },
        ftab_box: FtabBox {
            entries: ftab_entries,
        },
        // tx3g は型付き実装を持つ子ボックスを扱わないため、常に空にする
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx3g_to_json() {
        let sample_entry = Tx3gBox {
            data_reference_index: Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX,
            display_flags: 0,
            horizontal_justification: 0,
            vertical_justification: 0,
            background_color_rgba: [0, 0, 0, 255],
            default_text_box: BoxRecord {
                top: 0,
                left: 0,
                bottom: 240,
                right: 320,
            },
            default_style: StyleRecord {
                start_char: 0,
                end_char: 0,
                font_id: 1,
                face_style_flags: 0,
                font_size: 12,
                text_color_rgba: [255, 255, 255, 255],
            },
            ftab_box: FtabBox {
                entries: vec![FontRecord {
                    font_id: 1,
                    font_name: b"Serif".to_vec(),
                }],
            },
            unknown_boxes: Vec::new(),
        };

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"tx3g""#));
        assert!(json.contains(r#""display_flags":0"#));
        assert!(json.contains(r#""horizontal_justification":0"#));
        assert!(json.contains(r#""vertical_justification":0"#));
        assert!(json.contains(r#""background_color_rgba":[0,0,0,255]"#));
        assert!(json.contains(r#""default_text_box":[0,0,240,320]"#));
        assert!(json.contains(r#""default_style":{"start_char":0"#));
        assert!(json.contains(r#""font_id":1"#));
        assert!(json.contains(r#""font_name":[83,101,114,105,102]"#));
    }

    #[test]
    fn test_json_to_tx3g() {
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 255],
            "default_text_box": [0, 0, 240, 320],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 12,
                "text_color_rgba": [255, 255, 255, 255]
            },
            "ftab": [
                { "font_id": 1, "font_name": [83, 101, 114, 105, 102] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な tx3g JSON");

        assert_eq!(sample_entry.display_flags, 0);
        assert_eq!(sample_entry.horizontal_justification, 0);
        assert_eq!(sample_entry.background_color_rgba, [0, 0, 0, 255]);
        assert_eq!(sample_entry.default_text_box.top, 0);
        assert_eq!(sample_entry.default_text_box.left, 0);
        assert_eq!(sample_entry.default_text_box.bottom, 240);
        assert_eq!(sample_entry.default_text_box.right, 320);
        assert_eq!(sample_entry.default_style.font_id, 1);
        assert_eq!(sample_entry.default_style.font_size, 12);

        // ftab の中身を検証する
        assert_eq!(sample_entry.ftab_box.entries.len(), 1);
        assert_eq!(sample_entry.ftab_box.entries[0].font_id, 1);
        assert_eq!(sample_entry.ftab_box.entries[0].font_name, b"Serif");

        // data_reference_index は既定値、unknown_boxes は空であることを検証する
        assert_eq!(
            sample_entry.data_reference_index,
            Tx3gBox::DEFAULT_DATA_REFERENCE_INDEX
        );
        assert!(sample_entry.unknown_boxes.is_empty());
    }

    #[test]
    fn test_tx3g_json_roundtrip_with_empty_ftab() {
        // ftab が空のケースでもラウンドトリップが成立する
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 0,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": []
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な tx3g JSON");
        assert!(sample_entry.ftab_box.entries.is_empty());

        // 空 ftab のラウンドトリップも問題なく成立する
        let out_json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(out_json.contains(r#""ftab":[]"#));
    }

    /// 空の font_name 要素を含む ftab を parse → JSON 再出力する往復テスト
    ///
    /// font_name は空のバイト列も保持できるため、空配列として再出力できることを検証する
    #[test]
    fn test_tx3g_json_roundtrip_with_empty_font_name() {
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": [
                { "font_id": 1, "font_name": [] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("空 font_name の tx3g JSON");

        assert_eq!(sample_entry.ftab_box.entries.len(), 1);
        assert!(sample_entry.ftab_box.entries[0].font_name.is_empty());

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""font_name":[]"#),
            "空 font_name が [] として再出力されること: {out}"
        );
    }

    #[test]
    fn test_tx3g_json_roundtrip_with_multiple_fonts() {
        // ftab に複数エントリを持たせて、順序と値が保持されることを検証する
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 0,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": [
                { "font_id": 1, "font_name": [65, 66] },
                { "font_id": 2, "font_name": [67, 68, 69] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な tx3g JSON");

        // 順序と値の検証
        assert_eq!(sample_entry.ftab_box.entries.len(), 2);
        assert_eq!(sample_entry.ftab_box.entries[0].font_id, 1);
        assert_eq!(sample_entry.ftab_box.entries[0].font_name, b"AB");
        assert_eq!(sample_entry.ftab_box.entries[1].font_id, 2);
        assert_eq!(sample_entry.ftab_box.entries[1].font_name, b"CDE");
    }

    #[test]
    fn test_json_to_tx3g_rejects_missing_display_flags() {
        // display_flags 以外の必須フィールドは揃っているが、display_flags が
        // 欠落しているためパースに失敗することを検証する
        let json_str = r#"{
            "kind": "tx3g",
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 255],
            "default_text_box": [0, 0, 240, 320],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 12,
                "text_color_rgba": [255, 255, 255, 255]
            },
            "ftab": [
                { "font_id": 1, "font_name": [83, 101, 114, 105, 102] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json(json.value());
        assert!(result.is_err(), "display_flags 欠落時はパース失敗すること");
    }

    /// ftab の個数上限を parse 時に検査すること
    ///
    /// 65536 要素の JSON を構築するため Miri では実行しない。
    #[test]
    #[cfg(not(miri))]
    fn test_json_to_tx3g_rejects_too_many_ftab_entries() {
        let entries = (0..65536)
            .map(|_| r#"{"font_id":1,"font_name":[65]}"#)
            .collect::<Vec<_>>()
            .join(",");
        let json_str = format!(
            r#"{{"kind":"tx3g","display_flags":0,"horizontal_justification":0,"vertical_justification":0,"background_color_rgba":[0,0,0,255],"default_text_box":[0,0,240,320],"default_style":{{"start_char":0,"end_char":0,"font_id":1,"face_style_flags":0,"font_size":12,"text_color_rgba":[255,255,255,255]}},"ftab":[{entries}]}}"#
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let error = parse_json(json.value()).expect_err("上限超過は拒否されること");
        assert!(error.to_string().contains("at most 65535"), "{error}");
    }

    /// font_name の長さ上限を parse 時に検査すること
    #[test]
    fn test_json_to_tx3g_rejects_too_long_font_name() {
        let font_name = (0..256).map(|_| "65").collect::<Vec<_>>().join(",");
        let json_str = format!(
            r#"{{"kind":"tx3g","display_flags":0,"horizontal_justification":0,"vertical_justification":0,"background_color_rgba":[0,0,0,255],"default_text_box":[0,0,240,320],"default_style":{{"start_char":0,"end_char":0,"font_id":1,"face_style_flags":0,"font_size":12,"text_color_rgba":[255,255,255,255]}},"ftab":[{{"font_id":1,"font_name":[{font_name}]}}]}}"#
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let error = parse_json(json.value()).expect_err("上限超過は拒否されること");
        assert!(error.to_string().contains("at most 255"), "{error}");
    }
}
