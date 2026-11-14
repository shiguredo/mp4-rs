//! wvtt サンプルエントリーの JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::boxes::{VttCBox, WvttBox};

/// wvtt サンプルエントリーを JSON フォーマットする
///
/// WebVTT の設定テキストは [`WvttBox::vttc_box`] の [`VttCBox::config`] が
/// 保持しているため、そこから取り出して書き出す
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &WvttBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "wvtt")?;
        f.member("dataReferenceIndex", data.data_reference_index.get())?;
        f.member("config", data.vttc_box.config.as_str())?;
        crate::boxes::fmt_unknown_boxes(f, &data.unknown_boxes)
    })
}

/// JSON から [`WvttBox`] に変換する
///
/// `config` は UTF-8 文字列として解釈し、interior null もそのまま保持する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<WvttBox, nojson::JsonParseError> {
    let config: String = value.to_member("config")?.required()?.try_into()?;

    Ok(WvttBox {
        data_reference_index: crate::boxes::parse_non_zero_u16_member(
            value,
            "dataReferenceIndex",
            WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
        )?,
        vttc_box: VttCBox { config },
        unknown_boxes: crate::boxes::parse_unknown_boxes(value)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wvtt_to_json() {
        let sample_entry = WvttBox {
            data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
            vttc_box: VttCBox {
                config: "WEBVTT".to_string(),
            },
            unknown_boxes: Vec::new(),
        };

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"wvtt""#));
        assert!(json.contains(r#""config":"WEBVTT""#));
    }

    #[test]
    fn test_json_to_wvtt() {
        let json_str = r#"{
            "kind": "wvtt",
            "config": "WEBVTT\n\nSTYLE"
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な wvtt JSON");

        // config フィールドが期待通りの文字列であることを検証する
        assert_eq!(sample_entry.vttc_box.config, "WEBVTT\n\nSTYLE");

        // data_reference_index は既定値、unknown_boxes は空であることを検証する
        assert_eq!(
            sample_entry.data_reference_index,
            WvttBox::DEFAULT_DATA_REFERENCE_INDEX
        );
        assert!(sample_entry.unknown_boxes.is_empty());
    }

    #[test]
    fn test_wvtt_json_roundtrip_with_interior_null() {
        // VttCBox::config は String で保持されるため interior null を許容する。
        // JSON 経路（fmt_json → parse_json）でも interior null が保持されることを担保する
        let original_config = "WEBVTT\n\0\nSTYLE";

        let sample_entry = WvttBox {
            data_reference_index: WvttBox::DEFAULT_DATA_REFERENCE_INDEX,
            vttc_box: VttCBox {
                config: original_config.to_string(),
            },
            unknown_boxes: Vec::new(),
        };

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();

        let parsed_json = nojson::RawJson::parse(&json).expect("有効な JSON");
        let roundtripped = parse_json(parsed_json.value()).expect("有効な wvtt JSON");

        assert_eq!(
            roundtripped.vttc_box.config, original_config,
            "ラウンドトリップで interior null が保持されること"
        );
    }
}
