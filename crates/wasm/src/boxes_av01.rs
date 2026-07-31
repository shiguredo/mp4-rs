//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（av01 用）

use c_api::boxes::Mp4SampleEntryAv01;

/// AV01（AV1）サンプルエントリーを JSON フォーマットする
pub fn fmt_json_mp4_sample_entry_av01(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryAv01,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "av01")?;
        f.member("width", data.width)?;
        f.member("height", data.height)?;
        f.member("seqProfile", data.seq_profile)?;
        f.member("seqLevelIdx0", data.seq_level_idx_0)?;
        f.member("seqTier0", data.seq_tier_0)?;
        f.member("highBitdepth", data.high_bitdepth)?;
        f.member("twelveBit", data.twelve_bit)?;
        f.member("monochrome", data.monochrome)?;
        f.member("chromaSubsamplingX", data.chroma_subsampling_x)?;
        f.member("chromaSubsamplingY", data.chroma_subsampling_y)?;
        f.member("chromaSamplePosition", data.chroma_sample_position)?;
        if data.initial_presentation_delay_present {
            f.member(
                "initialPresentationDelayMinusOne",
                data.initial_presentation_delay_minus_one,
            )?;
        }
        // パース時の allocate_and_copy_bytes は空入力で (null, 0) を格納し得る。
        // ここではその結果を読むだけだが、サイズ 0 を from_raw_parts に渡すと
        // null ポインタ経由の UB になり得るのでガードし、その場合は空配列として出力する
        // （フォーマット側ではエラーにはしない）
        let config_obus = if data.config_obus_size == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(data.config_obus, data.config_obus_size as usize) }
        };
        f.member("configObus", config_obus)
    })
}

/// JSON から Mp4SampleEntryAv01 に変換する
pub fn parse_json_mp4_sample_entry_av01(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryAv01, nojson::JsonParseError> {
    // パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
    // 確保済みバッファがリークする。まず全フィールドを Rust 型に落としてから
    // 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする

    // フェーズ 1: すべての JSON フィールドを Rust 型に落とす（メモリ確保前）
    let config_obus_vec: Vec<u8> = value.to_member("configObus")?.required()?.try_into()?;
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let seq_profile: u8 = value.to_member("seqProfile")?.required()?.try_into()?;
    let seq_level_idx_0: u8 = value.to_member("seqLevelIdx0")?.required()?.try_into()?;
    let seq_tier_0: u8 = value.to_member("seqTier0")?.required()?.try_into()?;
    let high_bitdepth: u8 = value.to_member("highBitdepth")?.required()?.try_into()?;
    let twelve_bit: u8 = value.to_member("twelveBit")?.required()?.try_into()?;
    let monochrome: u8 = value.to_member("monochrome")?.required()?.try_into()?;
    let chroma_subsampling_x: u8 = value
        .to_member("chromaSubsamplingX")?
        .required()?
        .try_into()?;
    let chroma_subsampling_y: u8 = value
        .to_member("chromaSubsamplingY")?
        .required()?
        .try_into()?;
    let chroma_sample_position: u8 = value
        .to_member("chromaSamplePosition")?
        .required()?
        .try_into()?;
    let initial_presentation_delay_present = value
        .to_member("initialPresentationDelayMinusOne")?
        .optional()
        .is_some();
    let initial_presentation_delay_minus_one: u8 = value
        .to_member("initialPresentationDelayMinusOne")?
        .map(|v| v.try_into())?
        .unwrap_or(0);

    // フェーズ 2: すべてのパースが成功したときだけメモリを確保する
    let (config_obus, config_obus_size) = crate::boxes::allocate_and_copy_bytes(&config_obus_vec);

    Ok(Mp4SampleEntryAv01 {
        width,
        height,
        seq_profile,
        seq_level_idx_0,
        seq_tier_0,
        high_bitdepth,
        twelve_bit,
        monochrome,
        chroma_subsampling_x,
        chroma_subsampling_y,
        chroma_sample_position,
        initial_presentation_delay_present,
        initial_presentation_delay_minus_one,
        config_obus,
        config_obus_size,
    })
}

/// AV01 サンプルエントリーのメモリを解放する
///
/// `parse_json_mp4_sample_entry_av01()` で割り当てられたメモリを解放する
pub fn mp4_sample_entry_av01_free(entry: &mut Mp4SampleEntryAv01) {
    if !entry.config_obus.is_null() && entry.config_obus_size > 0 {
        unsafe {
            crate::mp4_free(entry.config_obus.cast_mut(), entry.config_obus_size);
        }
        entry.config_obus = std::ptr::null();
        entry.config_obus_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_av01_to_json() {
        static CONFIG_OBUS: &[u8] = &[0x0a, 0x0b, 0x00, 0x00];

        let sample_entry = Mp4SampleEntryAv01 {
            width: 3840,
            height: 2160,
            seq_profile: 0,
            seq_level_idx_0: 13,
            seq_tier_0: 0,
            high_bitdepth: 0,
            twelve_bit: 0,
            monochrome: 0,
            chroma_subsampling_x: 1,
            chroma_subsampling_y: 1,
            chroma_sample_position: 0,
            initial_presentation_delay_present: false,
            initial_presentation_delay_minus_one: 0,
            config_obus: CONFIG_OBUS.as_ptr(),
            config_obus_size: CONFIG_OBUS.len() as u32,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_av01(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"av01""#));
        assert!(json.contains(r#""width":3840"#));
        assert!(json.contains(r#""height":2160"#));
        assert!(json.contains(r#""seqProfile":0"#));
        assert!(json.contains(r#""seqLevelIdx0":13"#));
        assert!(json.contains(r#""seqTier0":0"#));
        assert!(json.contains(r#""configObus":"#));
    }

    #[test]
    fn test_json_to_av01() {
        let json_str = r#"{"kind": "av01", "width": 3840, "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": [10, 11, 0, 0]}"#;

        let json = nojson::RawJson::parse(json_str).expect("valid JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_av01(json.value()).expect("valid av01 JSON");

        assert_eq!(sample_entry.width, 3840);
        assert_eq!(sample_entry.height, 2160);
        assert_eq!(sample_entry.seq_profile, 0);
        assert_eq!(sample_entry.seq_level_idx_0, 13);
        assert_eq!(sample_entry.seq_tier_0, 0);
        assert_eq!(sample_entry.high_bitdepth, 0);
        assert_eq!(sample_entry.twelve_bit, 0);
        assert_eq!(sample_entry.monochrome, 0);
        assert_eq!(sample_entry.chroma_subsampling_x, 1);
        assert_eq!(sample_entry.chroma_subsampling_y, 1);
        assert_eq!(sample_entry.chroma_sample_position, 0);
        assert_eq!(sample_entry.config_obus_size, 4);
        assert!(!sample_entry.config_obus.is_null());
        let data = unsafe {
            std::slice::from_raw_parts(
                sample_entry.config_obus,
                sample_entry.config_obus_size as usize,
            )
        };
        assert_eq!(data, &[10, 11, 0, 0]);

        // メモリ解放
        mp4_sample_entry_av01_free(&mut sample_entry);
        assert_eq!(sample_entry.config_obus_size, 0);
        assert!(sample_entry.config_obus.is_null());
    }

    /// 空の configObus を parse → JSON 再出力する往復テスト
    ///
    /// `allocate_and_copy_bytes` は空入力で `(null, 0)` を返す。
    /// fmt 側が `from_raw_parts(null, 0)` を呼ぶと UB になるため、
    /// ガード後も空配列として再出力できることを検証する
    #[test]
    fn test_json_to_av01_empty_config_obus_roundtrip() {
        let json_str = r#"{"kind": "av01", "width": 3840, "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": []}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_av01(json.value()).expect("空 configObus の av01 JSON");

        assert_eq!(sample_entry.config_obus_size, 0);
        assert!(sample_entry.config_obus.is_null());

        let out = nojson::json(|f| fmt_json_mp4_sample_entry_av01(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""configObus":[]"#),
            "空 configObus が [] として再出力されること: {out}"
        );

        mp4_sample_entry_av01_free(&mut sample_entry);
        assert_eq!(sample_entry.config_obus_size, 0);
        assert!(sample_entry.config_obus.is_null());
    }

    #[test]
    fn test_json_to_av01_rejects_missing_width_after_config_obus() {
        // configObus は揃っているが後段の必須フィールド width が欠落している。
        // 全フィールドを Rust 型に落としてからメモリ確保する順序なので、
        // この失敗経路では確保処理に到達せず Err だけが返る
        let json_str = r#"{"kind": "av01", "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": [10, 11, 0, 0]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json_mp4_sample_entry_av01(json.value());
        assert!(result.is_err(), "width 欠落時はパース失敗すること");
    }
}
