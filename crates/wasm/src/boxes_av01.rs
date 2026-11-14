//! AV1 サンプルエントリー（`av01`）の JSON 変換を提供するモジュール

use shiguredo_mp4::Uint;
use shiguredo_mp4::boxes::{Av01Box, Av1cBox};

/// AV1 サンプルエントリーを JSON オブジェクトとしてフォーマットする
pub(crate) fn fmt_json(f: &mut nojson::JsonFormatter<'_, '_>, data: &Av01Box) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "av01")?;
        f.member("width", data.visual.width)?;
        f.member("height", data.visual.height)?;
        f.member("seqProfile", data.av1c_box.seq_profile.get())?;
        f.member("seqLevelIdx0", data.av1c_box.seq_level_idx_0.get())?;
        f.member("seqTier0", data.av1c_box.seq_tier_0.get())?;
        f.member("highBitdepth", data.av1c_box.high_bitdepth.get())?;
        f.member("twelveBit", data.av1c_box.twelve_bit.get())?;
        f.member("monochrome", data.av1c_box.monochrome.get())?;
        f.member(
            "chromaSubsamplingX",
            data.av1c_box.chroma_subsampling_x.get(),
        )?;
        f.member(
            "chromaSubsamplingY",
            data.av1c_box.chroma_subsampling_y.get(),
        )?;
        f.member(
            "chromaSamplePosition",
            data.av1c_box.chroma_sample_position.get(),
        )?;
        // 初期表示遅延は省略可能なので、値が存在する場合だけキーを出力する
        if let Some(delay) = data.av1c_box.initial_presentation_delay_minus_one {
            f.member("initialPresentationDelayMinusOne", delay.get())?;
        }
        // configOBUs はバイト列をそのまま数値配列として出力する
        f.member("configObus", data.av1c_box.config_obus.as_slice())
    })
}

/// JSON オブジェクトから AV1 サンプルエントリーを構築する
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Av01Box, nojson::JsonParseError> {
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let seq_profile: Uint<u8, 3, 5> = crate::boxes::uint_u8_member(value, "seqProfile")?;
    // 3..=7 は予約値
    if seq_profile.get() > 2 {
        return Err(crate::boxes::invalid_member(
            value,
            "seqProfile",
            "seqProfile must be 0, 1, or 2",
        ));
    }
    let seq_level_idx_0: Uint<u8, 5, 0> = crate::boxes::uint_u8_member(value, "seqLevelIdx0")?;
    let seq_tier_0: Uint<u8, 1, 7> = crate::boxes::uint_u8_member(value, "seqTier0")?;
    let high_bitdepth: Uint<u8, 1, 6> = crate::boxes::uint_u8_member(value, "highBitdepth")?;
    let twelve_bit: Uint<u8, 1, 5> = crate::boxes::uint_u8_member(value, "twelveBit")?;
    let monochrome: Uint<u8, 1, 4> = crate::boxes::uint_u8_member(value, "monochrome")?;
    let chroma_subsampling_x: Uint<u8, 1, 3> =
        crate::boxes::uint_u8_member(value, "chromaSubsamplingX")?;
    let chroma_subsampling_y: Uint<u8, 1, 2> =
        crate::boxes::uint_u8_member(value, "chromaSubsamplingY")?;
    let chroma_sample_position: Uint<u8, 2, 0> =
        crate::boxes::uint_u8_member(value, "chromaSamplePosition")?;
    // 3 は予約値
    if chroma_sample_position.get() == 3 {
        return Err(crate::boxes::invalid_member(
            value,
            "chromaSamplePosition",
            "chromaSamplePosition must be 0, 1, or 2",
        ));
    }
    // キーがない省略可能なフィールドは None になる
    let initial_presentation_delay_minus_one: Option<Uint<u8, 4, 0>> =
        crate::boxes::optional_uint_u8_member(value, "initialPresentationDelayMinusOne")?;
    let config_obus: Vec<u8> = value.to_member("configObus")?.required()?.try_into()?;

    let av1c_box = Av1cBox {
        seq_profile,
        seq_level_idx_0,
        seq_tier_0,
        high_bitdepth,
        twelve_bit,
        monochrome,
        chroma_subsampling_x,
        chroma_subsampling_y,
        chroma_sample_position,
        initial_presentation_delay_minus_one,
        config_obus,
    };

    Ok(Av01Box {
        visual: crate::boxes::build_visual_sample_entry_fields(width, height),
        av1c_box,
        unknown_boxes: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::boxes::VisualSampleEntryFields;

    /// テスト用の AV1 サンプルエントリーを構築する
    ///
    /// `initial_presentation_delay_minus_one` に `None` を渡すと、
    /// 初期表示遅延を持たないサンプルエントリーになる。
    fn build_av01_box(initial_presentation_delay_minus_one: Option<u8>) -> Av01Box {
        Av01Box {
            visual: crate::boxes::build_visual_sample_entry_fields(3840, 2160),
            av1c_box: Av1cBox {
                seq_profile: Uint::new(0),
                seq_level_idx_0: Uint::new(13),
                seq_tier_0: Uint::new(0),
                high_bitdepth: Uint::new(0),
                twelve_bit: Uint::new(0),
                monochrome: Uint::new(0),
                chroma_subsampling_x: Uint::new(1),
                chroma_subsampling_y: Uint::new(1),
                chroma_sample_position: Uint::new(0),
                initial_presentation_delay_minus_one: initial_presentation_delay_minus_one
                    .map(Uint::new),
                config_obus: vec![0x0a, 0x0b, 0x00, 0x00],
            },
            unknown_boxes: Vec::new(),
        }
    }

    #[test]
    fn test_av01_to_json() {
        let sample_entry = build_av01_box(None);

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""kind":"av01""#),
            "kind が av01 であること: {json}"
        );
        assert!(
            json.contains(r#""width":3840"#),
            "width が 3840 であること: {json}"
        );
        assert!(
            json.contains(r#""height":2160"#),
            "height が 2160 であること: {json}"
        );
        assert!(
            json.contains(r#""seqProfile":0"#),
            "seqProfile が 0 であること: {json}"
        );
        assert!(
            json.contains(r#""seqLevelIdx0":13"#),
            "seqLevelIdx0 が 13 であること: {json}"
        );
        assert!(
            json.contains(r#""seqTier0":0"#),
            "seqTier0 が 0 であること: {json}"
        );
        assert!(
            json.contains(r#""configObus":[10,11,0,0]"#),
            "configObus が数値配列で出力されること: {json}"
        );
        assert!(
            !json.contains("initialPresentationDelayMinusOne"),
            "初期表示遅延を持たない場合はキー自体を出力しないこと: {json}"
        );
    }

    #[test]
    fn test_av01_to_json_with_initial_presentation_delay() {
        let sample_entry = build_av01_box(Some(3));

        let json = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            json.contains(r#""initialPresentationDelayMinusOne":3"#),
            "初期表示遅延を持つ場合は値付きでキーを出力すること: {json}"
        );
    }

    #[test]
    fn test_json_to_av01() {
        let json_str = r#"{"kind": "av01", "width": 3840, "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": [10, 11, 0, 0]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value()).expect("av01 の JSON をパースできること");

        assert_eq!(sample_entry.visual.width, 3840, "width が 3840 であること");
        assert_eq!(
            sample_entry.visual.height, 2160,
            "height が 2160 であること"
        );
        assert_eq!(
            sample_entry.visual.data_reference_index,
            VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            "data_reference_index が既定値であること"
        );
        assert_eq!(
            sample_entry.av1c_box.seq_profile.get(),
            0,
            "seqProfile が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.seq_level_idx_0.get(),
            13,
            "seqLevelIdx0 が 13 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.seq_tier_0.get(),
            0,
            "seqTier0 が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.high_bitdepth.get(),
            0,
            "highBitdepth が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.twelve_bit.get(),
            0,
            "twelveBit が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.monochrome.get(),
            0,
            "monochrome が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.chroma_subsampling_x.get(),
            1,
            "chromaSubsamplingX が 1 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.chroma_subsampling_y.get(),
            1,
            "chromaSubsamplingY が 1 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.chroma_sample_position.get(),
            0,
            "chromaSamplePosition が 0 であること"
        );
        assert_eq!(
            sample_entry.av1c_box.config_obus,
            vec![10, 11, 0, 0],
            "configObus が入力どおりの数値配列になること"
        );
        assert!(
            sample_entry
                .av1c_box
                .initial_presentation_delay_minus_one
                .is_none(),
            "初期表示遅延のキーがない場合は None になること"
        );
        assert!(
            sample_entry.unknown_boxes.is_empty(),
            "unknown_boxes が空であること"
        );
    }

    #[test]
    fn test_json_to_av01_with_initial_presentation_delay() {
        let json_str = r#"{"kind": "av01", "width": 3840, "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "initialPresentationDelayMinusOne": 3, "configObus": [10, 11, 0, 0]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry =
            parse_json(json.value()).expect("初期表示遅延を持つ av01 の JSON をパースできること");

        assert_eq!(
            sample_entry
                .av1c_box
                .initial_presentation_delay_minus_one
                .map(Uint::get),
            Some(3),
            "初期表示遅延が 3 としてパースされること"
        );
    }

    /// 空の configObus を parse して JSON に再出力する往復テスト
    ///
    /// 空のバイト列が `[]` としてそのまま再出力されることを検証する。
    #[test]
    fn test_json_to_av01_empty_config_obus_roundtrip() {
        let json_str = r#"{"kind": "av01", "width": 3840, "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": []}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let sample_entry = parse_json(json.value())
            .expect("空の configObus を持つ av01 の JSON をパースできること");

        assert!(
            sample_entry.av1c_box.config_obus.is_empty(),
            "空の configObus が空のバイト列としてパースされること"
        );

        let out = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""configObus":[]"#),
            "空の configObus が [] として再出力されること: {out}"
        );
    }

    #[test]
    fn test_json_to_av01_rejects_missing_width() {
        // configObus は揃っているが必須フィールドの width が欠落している場合はパースに失敗する
        let json_str = r#"{"kind": "av01", "height": 2160, "seqProfile": 0, "seqLevelIdx0": 13, "seqTier0": 0, "highBitdepth": 0, "twelveBit": 0, "monochrome": 0, "chromaSubsamplingX": 1, "chromaSubsamplingY": 1, "chromaSamplePosition": 0, "configObus": [10, 11, 0, 0]}"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let result = parse_json(json.value());
        assert!(result.is_err(), "width 欠落時はパースに失敗すること");
    }

    /// ビット幅を超える値は、隣接フィールドを壊す前に拒否すること
    #[test]
    fn test_json_to_av01_rejects_out_of_range_bits() {
        const BASE: &str = r#"{"kind":"av01","width":640,"height":480,"seqProfile":0,"seqLevelIdx0":13,"seqTier0":0,"highBitdepth":0,"twelveBit":0,"monochrome":0,"chromaSubsamplingX":1,"chromaSubsamplingY":1,"chromaSamplePosition":0,"configObus":[1]}"#;
        let cases = [
            (r#""seqProfile":0"#, r#""seqProfile":8"#),
            (r#""seqProfile":0"#, r#""seqProfile":3"#),
            (r#""seqLevelIdx0":13"#, r#""seqLevelIdx0":32"#),
            (r#""seqTier0":0"#, r#""seqTier0":2"#),
            (r#""highBitdepth":0"#, r#""highBitdepth":2"#),
            (r#""chromaSamplePosition":0"#, r#""chromaSamplePosition":4"#),
            (r#""chromaSamplePosition":0"#, r#""chromaSamplePosition":3"#),
        ];
        for (from, to) in cases {
            let json_str = BASE.replace(from, to);
            let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
            assert!(parse_json(json.value()).is_err(), "{to} は拒否されること");
        }

        // 省略可能な initialPresentationDelayMinusOne も 4 ビット
        let json_str = BASE.replace(
            r#""configObus":[1]"#,
            r#""configObus":[1],"initialPresentationDelayMinusOne":16"#,
        );
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        assert!(parse_json(json.value()).is_err());
    }
}
