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
            raw_bytes_as_str(data, data.config_data, data.config_size),
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

/// `Mp4SampleEntryWvtt` の `*const u8 + u32` フィールドを `&str` に復元する
///
/// バイト列は必ず有効な UTF-8 でなければならない（`VttCBox::config: String` invariant で保証、
/// および JSON parse 経由でも valid UTF-8 が渡される）。invariant が壊れて
/// UTF-8 不正なバイト列が渡された場合は実装バグとして panic する。
/// 返り値のライフタイムは `entry` の借用に紐付いており、`entry` より長生きする
/// 参照は返せないことを型システムが保証する。
///
/// ただし `VttCBox::config: String` は Stpp の `Utf8String` と違い **interior null を許容する**
/// invariant のため、返り値は interior null を含み得る。JSON 出力パスでは
/// `nojson::JsonFormatter` の escape に委ねる
fn raw_bytes_as_str(_entry: &Mp4SampleEntryWvtt, data: *const u8, size: u32) -> &str {
    if size == 0 || data.is_null() {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, size as usize) };
    std::str::from_utf8(bytes).expect("Mp4SampleEntryWvtt field bytes must be valid UTF-8")
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
}
