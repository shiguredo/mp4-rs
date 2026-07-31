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
        // パース時の allocate_and_copy_bytes は空入力で (null, 0) を格納し得る。
        // ここではその結果を読むだけだが、サイズ 0 を from_raw_parts に渡すと
        // null ポインタ経由の UB になり得るのでガードし、その場合は空配列として出力する
        // （フォーマット側ではエラーにはしない）
        let streaminfo = if data.streaminfo_size == 0 {
            &[][..]
        } else {
            unsafe {
                std::slice::from_raw_parts(data.streaminfo_data, data.streaminfo_size as usize)
            }
        };
        f.member("streaminfoData", streaminfo)
    })
}

/// JSON から Mp4SampleEntryFlac に変換する
pub fn parse_json_mp4_sample_entry_flac(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryFlac, nojson::JsonParseError> {
    // パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
    // 確保済みバッファがリークする。まず全フィールドを Rust 型に落としてから
    // 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする

    // フェーズ 1: すべての JSON フィールドを Rust 型に落とす（メモリ確保前）
    let streaminfo_data_vec: Vec<u8> = value.to_member("streaminfoData")?.required()?.try_into()?;
    let channel_count: u8 = value.to_member("channelCount")?.required()?.try_into()?;
    let sample_rate: u16 = value.to_member("sampleRate")?.required()?.try_into()?;
    let sample_size: u16 = value.to_member("sampleSize")?.required()?.try_into()?;

    // フェーズ 2: すべてのパースが成功したときだけメモリを確保する
    let (streaminfo_data, streaminfo_size) =
        crate::boxes::allocate_and_copy_bytes(&streaminfo_data_vec);

    Ok(Mp4SampleEntryFlac {
        channel_count,
        sample_rate,
        sample_size,
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

    /// 空の streaminfoData を parse → JSON 再出力する往復テスト
    ///
    /// `allocate_and_copy_bytes` は空入力で `(null, 0)` を返す。
    /// fmt 側が `from_raw_parts(null, 0)` を呼ぶと UB になるため、
    /// ガード後も空配列として再出力できることを検証する
    #[test]
    fn test_json_to_flac_empty_streaminfo_roundtrip() {
        let json_str = r#"{"kind": "flac", "channelCount": 2, "sampleRate": 44100, "sampleSize": 16, "streaminfoData": []}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_flac(json.value()).expect("空 streaminfoData の flac JSON");

        assert_eq!(sample_entry.streaminfo_size, 0);
        assert!(sample_entry.streaminfo_data.is_null());

        let out = nojson::json(|f| fmt_json_mp4_sample_entry_flac(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""streaminfoData":[]"#),
            "空 streaminfoData が [] として再出力されること: {out}"
        );

        mp4_sample_entry_flac_free(&mut sample_entry);
        assert_eq!(sample_entry.streaminfo_size, 0);
        assert!(sample_entry.streaminfo_data.is_null());
    }

    #[test]
    fn test_json_to_flac_rejects_missing_channel_count_after_streaminfo() {
        // streaminfoData は揃っているが後段の必須フィールド channelCount が欠落している。
        // 全フィールドを Rust 型に落としてからメモリ確保する順序なので、
        // この失敗経路では確保処理に到達せず Err だけが返る
        let json_str = r#"{"kind": "flac", "sampleRate": 44100, "sampleSize": 16, "streaminfoData": [0, 16, 0, 16]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json_mp4_sample_entry_flac(json.value());
        assert!(result.is_err(), "channelCount 欠落時はパース失敗すること");
    }
}
