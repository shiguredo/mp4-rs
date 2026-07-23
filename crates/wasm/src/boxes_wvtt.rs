//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（wvtt 用）

use c_api::boxes::Mp4SampleEntryWvtt;

/// wvtt サンプルエントリーを JSON フォーマットする
///
/// `config` フィールドは UTF-8 バイト列として保持されているため、
/// JSON 出力時に `str::from_utf8` で復元して文字列として書き出す
pub fn fmt_json_mp4_sample_entry_wvtt(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryWvtt,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "wvtt")?;
        f.member(
            "config",
            crate::boxes::raw_bytes_as_str(data, data.config_data, data.config_size),
        )
    })
}

/// JSON から Mp4SampleEntryWvtt に変換する
///
/// 返り値の Mp4SampleEntryWvtt は内部でメモリを確保する。
/// 不要になったら [`mp4_sample_entry_wvtt_free()`] で解放すること
pub fn parse_json_mp4_sample_entry_wvtt(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryWvtt, nojson::JsonParseError> {
    // wvtt は config の 1 本のみ扱うため、部分失敗リーク対策の順序制約は不要
    let config_str = value
        .to_member("config")?
        .required()?
        .to_unquoted_string_str()?;

    let (config_data, config_size) = crate::boxes::allocate_and_copy_bytes(config_str.as_bytes());

    Ok(Mp4SampleEntryWvtt {
        config_data,
        config_size,
    })
}

/// wvtt サンプルエントリーのメモリを解放する
///
/// [`parse_json_mp4_sample_entry_wvtt()`] で割り当てられたバッファを解放する
pub fn mp4_sample_entry_wvtt_free(entry: &mut Mp4SampleEntryWvtt) {
    if !entry.config_data.is_null() && entry.config_size > 0 {
        unsafe {
            crate::mp4_free(entry.config_data.cast_mut(), entry.config_size);
        }
        entry.config_data = std::ptr::null();
        entry.config_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wvtt_to_json() {
        static CONFIG: &[u8] = b"WEBVTT";

        let sample_entry = Mp4SampleEntryWvtt {
            config_data: CONFIG.as_ptr(),
            config_size: CONFIG.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_wvtt(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"wvtt""#));
        assert!(json.contains(r#""config":"WEBVTT""#));
    }

    #[test]
    fn test_json_to_wvtt_and_free() {
        let json_str = r#"{
            "kind": "wvtt",
            "config": "WEBVTT\n\nSTYLE"
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_wvtt(json.value()).expect("有効な wvtt JSON");

        // config フィールドが期待通りのバイト列であることを検証する
        let config = unsafe {
            std::slice::from_raw_parts(sample_entry.config_data, sample_entry.config_size as usize)
        };
        assert_eq!(config, b"WEBVTT\n\nSTYLE");

        // config フィールドのメモリが解放されることを検証する
        mp4_sample_entry_wvtt_free(&mut sample_entry);
        assert_eq!(sample_entry.config_size, 0);
        assert!(sample_entry.config_data.is_null());
    }

    #[test]
    fn test_wvtt_json_roundtrip_with_interior_null() {
        // VttCBox::config は Stpp の Utf8String と違い interior null を許容する。
        // JSON 経路（fmt_json → parse_json）でも interior null が保持されることを担保する
        let original_config: &[u8] = b"WEBVTT\n\x00\nSTYLE";

        let sample_entry = Mp4SampleEntryWvtt {
            config_data: original_config.as_ptr(),
            config_size: original_config.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_wvtt(f, &sample_entry)).to_string();

        let parsed_json = nojson::RawJson::parse(&json).expect("有効な JSON");
        let mut roundtripped =
            parse_json_mp4_sample_entry_wvtt(parsed_json.value()).expect("有効な wvtt JSON");

        let roundtripped_bytes = unsafe {
            std::slice::from_raw_parts(roundtripped.config_data, roundtripped.config_size as usize)
        };
        assert_eq!(
            roundtripped_bytes, original_config,
            "ラウンドトリップで interior null が保持されること"
        );

        mp4_sample_entry_wvtt_free(&mut roundtripped);
    }
}
