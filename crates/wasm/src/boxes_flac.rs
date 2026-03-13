//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（flac 用）

use c_api::boxes::Mp4SampleEntryFlac;

/// FLAC サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_flac(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryFlac,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "flac")?;
        f.member("channelCount", data.channel_count)?;
        f.member("sampleRate", data.sample_rate)?;
        f.member("sampleSize", data.sample_size)?;
        let streaminfo = unsafe {
            std::slice::from_raw_parts(data.streaminfo_data, data.streaminfo_size as usize)
        };
        f.member("streaminfoData", streaminfo)
    })
}

/// JSON から Mp4SampleEntryFlac に変換する
pub fn parse_json_mp4_sample_entry_flac(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryFlac, nojson::JsonParseError> {
    let streaminfo_data_value = value.to_member("streaminfoData")?.required()?;
    let streaminfo_data_vec: Vec<u8> = streaminfo_data_value.try_into()?;
    let (streaminfo_data, streaminfo_size) =
        crate::boxes::allocate_and_copy_bytes(&streaminfo_data_vec);

    Ok(Mp4SampleEntryFlac {
        channel_count: value.to_member("channelCount")?.required()?.try_into()?,
        sample_rate: value.to_member("sampleRate")?.required()?.try_into()?,
        sample_size: value.to_member("sampleSize")?.required()?.try_into()?,
        streaminfo_data,
        streaminfo_size,
    })
}

/// FLAC サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_flac()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_flac_free(entry: &mut Mp4SampleEntryFlac) {
    if !entry.streaminfo_data.is_null() && entry.streaminfo_size > 0 {
        unsafe {
            crate::mp4_free(entry.streaminfo_data.cast_mut(), entry.streaminfo_size);
        }
        entry.streaminfo_data = std::ptr::null();
        entry.streaminfo_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flac_to_json() {
        static STREAMINFO: &[u8] = &[0x00, 0x10, 0x00, 0x10];

        let sample_entry = Mp4SampleEntryFlac {
            channel_count: 2,
            sample_rate: 44100,
            sample_size: 16,
            streaminfo_data: STREAMINFO.as_ptr(),
            streaminfo_size: STREAMINFO.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_flac(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"flac""#));
        assert!(json.contains(r#""channelCount":2"#));
        assert!(json.contains(r#""sampleRate":44100"#));
        assert!(json.contains(r#""sampleSize":16"#));
        assert!(json.contains(r#""streaminfoData":"#));
    }

    #[test]
    fn test_json_to_flac() {
        let json_str = r#"{"kind": "flac", "channelCount": 2, "sampleRate": 44100, "sampleSize": 16, "streaminfoData": [0, 16, 0, 16]}"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_flac(json.value()).expect("valid flac JSON");

        assert_eq!(sample_entry.channel_count, 2);
        assert_eq!(sample_entry.sample_rate, 44100);
        assert_eq!(sample_entry.sample_size, 16);
        assert_eq!(sample_entry.streaminfo_size, 4);
        assert!(!sample_entry.streaminfo_data.is_null());
        let data = unsafe {
            std::slice::from_raw_parts(
                sample_entry.streaminfo_data,
                sample_entry.streaminfo_size as usize,
            )
        };
        assert_eq!(data, &[0, 16, 0, 16]);

        // メモリ解放
        mp4_sample_entry_flac_free(&mut sample_entry);
        assert_eq!(sample_entry.streaminfo_size, 0);
        assert!(sample_entry.streaminfo_data.is_null());
    }
}
