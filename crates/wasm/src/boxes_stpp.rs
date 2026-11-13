//! stpp サンプルエントリーの JSON 変換を提供する wasm 専用モジュール

use shiguredo_mp4::Utf8String;
use shiguredo_mp4::boxes::StppBox;

/// stpp サンプルエントリーを JSON フォーマットする
///
/// 3 本の文字列フィールドは [`Utf8String`] で保持されているため、
/// [`Utf8String::get()`] で取り出した文字列を書き出す
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &StppBox) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "stpp")?;
        f.member("namespace", data.namespace.get())?;
        f.member("schemaLocation", data.schema_location.get())?;
        f.member("auxiliaryMimeTypes", data.auxiliary_mime_types.get())
    })
}

/// JSON から [`StppBox`] に変換する
///
/// [`Utf8String`] は null 文字を保持できないため、null 文字を含む文字列が
/// 指定された場合はパースエラーになる
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<StppBox, nojson::JsonParseError> {
    // どのフィールドで null 文字が検出されたかをエラーメッセージから分かるように、
    // フィールドごとに JSON 文字列の取得と Utf8String への変換を行う
    let namespace_value = value.to_member("namespace")?.required()?;
    let namespace_str = namespace_value.to_unquoted_string_str()?;
    let namespace = Utf8String::new(&namespace_str)
        .ok_or_else(|| namespace_value.invalid("namespace must not contain null characters"))?;

    let schema_location_value = value.to_member("schemaLocation")?.required()?;
    let schema_location_str = schema_location_value.to_unquoted_string_str()?;
    let schema_location = Utf8String::new(&schema_location_str).ok_or_else(|| {
        schema_location_value.invalid("schemaLocation must not contain null characters")
    })?;

    let auxiliary_mime_types_value = value.to_member("auxiliaryMimeTypes")?.required()?;
    let auxiliary_mime_types_str = auxiliary_mime_types_value.to_unquoted_string_str()?;
    let auxiliary_mime_types = Utf8String::new(&auxiliary_mime_types_str).ok_or_else(|| {
        auxiliary_mime_types_value.invalid("auxiliaryMimeTypes must not contain null characters")
    })?;

    Ok(StppBox {
        data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
        namespace,
        schema_location,
        auxiliary_mime_types,
        // stpp は型付き実装を持つ子ボックスを扱わないため、常に空にする
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stpp_to_json() {
        let sample_entry = StppBox {
            data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
            namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
            schema_location: Utf8String::new("").expect("null 文字を含まない"),
            auxiliary_mime_types: Utf8String::new("").expect("null 文字を含まない"),
            unknown_boxes: Vec::new(),
        };

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"stpp""#));
        assert!(json.contains(r#""namespace":"http://www.w3.org/ns/ttml""#));
        assert!(json.contains(r#""schemaLocation":"""#));
        assert!(json.contains(r#""auxiliaryMimeTypes":"""#));
    }

    #[test]
    fn test_json_to_stpp() {
        let json_str = r#"{
            "kind": "stpp",
            "namespace": "http://www.w3.org/ns/ttml",
            "schemaLocation": "https://example.com/ttml.xsd",
            "auxiliaryMimeTypes": ""
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let sample_entry = parse_json(json.value()).expect("有効な stpp JSON");

        // 各文字列フィールドが期待通りの文字列であることを検証する
        assert_eq!(sample_entry.namespace.get(), "http://www.w3.org/ns/ttml");
        assert_eq!(
            sample_entry.schema_location.get(),
            "https://example.com/ttml.xsd"
        );
        assert_eq!(sample_entry.auxiliary_mime_types.get(), "");

        // data_reference_index は既定値、unknown_boxes は空であることを検証する
        assert_eq!(
            sample_entry.data_reference_index,
            StppBox::DEFAULT_DATA_REFERENCE_INDEX
        );
        assert!(sample_entry.unknown_boxes.is_empty());
    }

    #[test]
    fn test_json_to_stpp_rejects_null_character() {
        // Utf8String は null 文字を保持できないため、JSON 文字列に \u0000 が
        // 含まれる場合はパースエラーになる
        let json_str = r#"{
            "kind": "stpp",
            "namespace": "http://www.w3.org/ns/\u0000ttml",
            "schemaLocation": "",
            "auxiliaryMimeTypes": ""
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json(json.value());
        assert!(
            result.is_err(),
            "namespace に null 文字を含む場合はパース失敗すること"
        );
    }
}
