//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（stpp 用）

use c_api::boxes::Mp4SampleEntryStpp;

/// stpp サンプルエントリーを JSON フォーマットする
///
/// 3 本の文字列フィールドは UTF-8 バイト列として保持されているため、
/// JSON 出力時に `str::from_utf8` で復元して文字列として書き出す
pub fn fmt_json_mp4_sample_entry_stpp(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryStpp,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "stpp")?;
        f.member(
            "namespace",
            crate::boxes::raw_bytes_as_str(data, data.namespace_data, data.namespace_size),
        )?;
        f.member(
            "schemaLocation",
            crate::boxes::raw_bytes_as_str(
                data,
                data.schema_location_data,
                data.schema_location_size,
            ),
        )?;
        f.member(
            "auxiliaryMimeTypes",
            crate::boxes::raw_bytes_as_str(
                data,
                data.auxiliary_mime_types_data,
                data.auxiliary_mime_types_size,
            ),
        )
    })
}

/// JSON から Mp4SampleEntryStpp に変換する
///
/// 返り値の Mp4SampleEntryStpp は内部でメモリを確保する。
/// 不要になったら [`mp4_sample_entry_stpp_free()`] で解放すること
pub fn parse_json_mp4_sample_entry_stpp(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryStpp, nojson::JsonParseError> {
    // パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
    // 確保済みバッファがリークする。まず 3 本の &str を全部取り出してから
    // 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする
    let namespace_str = value
        .to_member("namespace")?
        .required()?
        .to_unquoted_string_str()?;
    let schema_location_str = value
        .to_member("schemaLocation")?
        .required()?
        .to_unquoted_string_str()?;
    let auxiliary_mime_types_str = value
        .to_member("auxiliaryMimeTypes")?
        .required()?
        .to_unquoted_string_str()?;

    let (namespace_data, namespace_size) =
        crate::boxes::allocate_and_copy_bytes(namespace_str.as_bytes());
    let (schema_location_data, schema_location_size) =
        crate::boxes::allocate_and_copy_bytes(schema_location_str.as_bytes());
    let (auxiliary_mime_types_data, auxiliary_mime_types_size) =
        crate::boxes::allocate_and_copy_bytes(auxiliary_mime_types_str.as_bytes());

    Ok(Mp4SampleEntryStpp {
        namespace_data,
        namespace_size,
        schema_location_data,
        schema_location_size,
        auxiliary_mime_types_data,
        auxiliary_mime_types_size,
    })
}

/// stpp サンプルエントリーのメモリを解放する
///
/// [`parse_json_mp4_sample_entry_stpp()`] で割り当てられたバッファを解放する
pub fn mp4_sample_entry_stpp_free(entry: &mut Mp4SampleEntryStpp) {
    // 3 本の文字列フィールドは `allocate_and_copy_bytes` の契約により
    // `(null, 0)` か `(非 null, 非 0)` の対になる。`mp4_free` は
    // null / size 0 のいずれでも noop なので無条件に呼んでよい
    unsafe {
        crate::mp4_free(entry.namespace_data.cast_mut(), entry.namespace_size);
        crate::mp4_free(
            entry.schema_location_data.cast_mut(),
            entry.schema_location_size,
        );
        crate::mp4_free(
            entry.auxiliary_mime_types_data.cast_mut(),
            entry.auxiliary_mime_types_size,
        );
    }
    entry.namespace_data = std::ptr::null();
    entry.namespace_size = 0;
    entry.schema_location_data = std::ptr::null();
    entry.schema_location_size = 0;
    entry.auxiliary_mime_types_data = std::ptr::null();
    entry.auxiliary_mime_types_size = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stpp_to_json() {
        static NAMESPACE: &[u8] = b"http://www.w3.org/ns/ttml";
        static SCHEMA_LOCATION: &[u8] = b"";
        static AUX_MIME: &[u8] = b"";

        let sample_entry = Mp4SampleEntryStpp {
            namespace_data: NAMESPACE.as_ptr(),
            namespace_size: NAMESPACE.len() as u32,
            schema_location_data: SCHEMA_LOCATION.as_ptr(),
            schema_location_size: SCHEMA_LOCATION.len() as u32,
            auxiliary_mime_types_data: AUX_MIME.as_ptr(),
            auxiliary_mime_types_size: AUX_MIME.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_stpp(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"stpp""#));
        assert!(json.contains(r#""namespace":"http://www.w3.org/ns/ttml""#));
        assert!(json.contains(r#""schemaLocation":"""#));
        assert!(json.contains(r#""auxiliaryMimeTypes":"""#));
    }

    #[test]
    fn test_json_to_stpp_and_free() {
        let json_str = r#"{
            "kind": "stpp",
            "namespace": "http://www.w3.org/ns/ttml",
            "schemaLocation": "https://example.com/ttml.xsd",
            "auxiliaryMimeTypes": ""
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_stpp(json.value()).expect("有効な stpp JSON");

        // 各フィールドが期待通りのバイト列であることを検証する
        let ns = unsafe {
            std::slice::from_raw_parts(
                sample_entry.namespace_data,
                sample_entry.namespace_size as usize,
            )
        };
        assert_eq!(ns, b"http://www.w3.org/ns/ttml");
        assert_eq!(
            sample_entry.schema_location_size,
            b"https://example.com/ttml.xsd".len() as u32,
        );
        assert_eq!(sample_entry.auxiliary_mime_types_size, 0);

        // メモリ解放が全フィールドに対して行われることを検証する
        mp4_sample_entry_stpp_free(&mut sample_entry);
        assert_eq!(sample_entry.namespace_size, 0);
        assert!(sample_entry.namespace_data.is_null());
        assert_eq!(sample_entry.schema_location_size, 0);
        assert!(sample_entry.schema_location_data.is_null());
        assert_eq!(sample_entry.auxiliary_mime_types_size, 0);
        assert!(sample_entry.auxiliary_mime_types_data.is_null());
    }
}
