//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（mp4a 用）

use c_api::boxes::Mp4SampleEntryMp4a;

/// MP4A（AAC）サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_mp4a(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryMp4a,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "mp4a")?;
        f.member("channelCount", data.channel_count)?;
        f.member("sampleRate", data.sample_rate)?;
        f.member("sampleSize", data.sample_size)?;
        f.member("bufferSizeDb", data.buffer_size_db)?;
        f.member("maxBitrate", data.max_bitrate)?;
        f.member("avgBitrate", data.avg_bitrate)?;
        let dec_specific_info = unsafe {
            std::slice::from_raw_parts(data.dec_specific_info, data.dec_specific_info_size as usize)
        };
        f.member("decSpecificInfo", dec_specific_info)
    })
}

/// JSON から Mp4SampleEntryMp4a に変換する
pub fn parse_json_mp4_sample_entry_mp4a(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryMp4a, nojson::JsonParseError> {
    let dec_specific_info_value = value.to_member("decSpecificInfo")?.required()?;
    let dec_specific_info_vec: Vec<u8> = dec_specific_info_value.try_into()?;
    let (dec_specific_info, dec_specific_info_size) =
        crate::boxes::allocate_and_copy_bytes(&dec_specific_info_vec);

    Ok(Mp4SampleEntryMp4a {
        channel_count: value.to_member("channelCount")?.required()?.try_into()?,
        sample_rate: value.to_member("sampleRate")?.required()?.try_into()?,
        sample_size: value.to_member("sampleSize")?.required()?.try_into()?,
        buffer_size_db: value.to_member("bufferSizeDb")?.required()?.try_into()?,
        max_bitrate: value.to_member("maxBitrate")?.required()?.try_into()?,
        avg_bitrate: value.to_member("avgBitrate")?.required()?.try_into()?,
        dec_specific_info,
        dec_specific_info_size,
    })
}

/// MP4A サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_mp4a()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_mp4a_free(entry: &mut Mp4SampleEntryMp4a) {
    if !entry.dec_specific_info.is_null() && entry.dec_specific_info_size > 0 {
        unsafe {
            crate::mp4_free(
                entry.dec_specific_info.cast_mut(),
                entry.dec_specific_info_size,
            );
        }
        entry.dec_specific_info = std::ptr::null();
        entry.dec_specific_info_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mp4a_to_json() {
        // MP4A（AAC）サンプルエントリーの作成
        // dec_specific_info は AAC-LC の場合の典型的な値
        static DEC_SPECIFIC_INFO: &[u8] = &[0x12, 0x10];

        let sample_entry = Mp4SampleEntryMp4a {
            channel_count: 2,
            sample_rate: 44100,
            sample_size: 16,
            buffer_size_db: 0,
            max_bitrate: 128000,
            avg_bitrate: 128000,
            dec_specific_info: DEC_SPECIFIC_INFO.as_ptr(),
            dec_specific_info_size: DEC_SPECIFIC_INFO.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_mp4a(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"mp4a""#));
        assert!(json.contains(r#""channelCount":2"#));
        assert!(json.contains(r#""sampleRate":44100"#));
        assert!(json.contains(r#""sampleSize":16"#));
        assert!(json.contains(r#""bufferSizeDb":0"#));
        assert!(json.contains(r#""maxBitrate":128000"#));
        assert!(json.contains(r#""avgBitrate":128000"#));
        assert!(json.contains(r#""decSpecificInfo":"#));
    }

    #[test]
    fn test_json_to_mp4a() {
        let json_str = r#"{
            "kind": "mp4a",
            "channelCount": 2,
            "sampleRate": 44100,
            "sampleSize": 16,
            "bufferSizeDb": 0,
            "maxBitrate": 128000,
            "avgBitrate": 128000,
            "decSpecificInfo": [1, 2]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_mp4a(json.value()).expect("valid mp4a JSON");

        assert_eq!(sample_entry.channel_count, 2);
        assert_eq!(sample_entry.sample_rate, 44100);
        assert_eq!(sample_entry.sample_size, 16);
        assert_eq!(sample_entry.buffer_size_db, 0);
        assert_eq!(sample_entry.max_bitrate, 128000);
        assert_eq!(sample_entry.avg_bitrate, 128000);
        assert_eq!(sample_entry.dec_specific_info_size, 2);
        assert!(!sample_entry.dec_specific_info.is_null());
        let data = unsafe {
            std::slice::from_raw_parts(
                sample_entry.dec_specific_info,
                sample_entry.dec_specific_info_size as usize,
            )
        };
        assert_eq!(data, &[1, 2]);

        // メモリ解放
        mp4_sample_entry_mp4a_free(&mut sample_entry);
        assert_eq!(sample_entry.dec_specific_info_size, 0);
        assert!(sample_entry.dec_specific_info.is_null());
    }
}
