//! c_api::boxes の JSON シリアライズ機能を提供する wasm 専用モジュール（tx3g 用）
use c_api::boxes::Mp4SampleEntryTx3g;

/// tx3g サンプルエントリーを JSON フォーマットする
///
/// `default_style` は入れ子オブジェクト、`ftab` は `font_id` + `font_name` (バイト配列) の
/// オブジェクト配列として書き出す。`font_name` は 3GPP TS 26.245 が文字エンコーディングを
/// 明示していないため UTF-8 を保証しない生バイト列であり、JSON 文字列ではなく数値配列で表現する
pub fn fmt_json_mp4_sample_entry_tx3g(
    f: &mut nojson::JsonFormatter<'_, '_>,
    data: &Mp4SampleEntryTx3g,
) -> std::fmt::Result {
    f.object(|f| {
        f.member("kind", "tx3g")?;
        f.member("display_flags", data.display_flags)?;
        f.member("horizontal_justification", data.horizontal_justification)?;
        f.member("vertical_justification", data.vertical_justification)?;
        f.member("background_color_rgba", data.background_color_rgba)?;
        f.member("default_text_box", data.default_text_box)?;
        f.member(
            "default_style",
            nojson::json(|f| {
                f.object(|f| {
                    f.member("start_char", data.default_style_start_char)?;
                    f.member("end_char", data.default_style_end_char)?;
                    f.member("font_id", data.default_style_font_id)?;
                    f.member("face_style_flags", data.default_style_face_style_flags)?;
                    f.member("font_size", data.default_style_font_size)?;
                    f.member("text_color_rgba", data.default_style_text_color_rgba)
                })
            }),
        )?;
        f.member(
            "ftab",
            FtabList {
                font_ids: data.ftab_font_ids,
                font_name_ptrs: data.ftab_font_name_ptrs,
                font_name_sizes: data.ftab_font_name_sizes,
                count: data.ftab_count,
            },
        )
    })
}

/// JSON から Mp4SampleEntryTx3g に変換する
///
/// 返り値の Mp4SampleEntryTx3g は内部でメモリを確保する。
/// 不要になったら [`mp4_sample_entry_tx3g_free()`] で解放すること
pub fn parse_json_mp4_sample_entry_tx3g(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4SampleEntryTx3g, nojson::JsonParseError> {
    // パースとメモリ確保を交互に行うと、途中でパースが失敗したときに
    // 確保済みバッファがリークする。まず全フィールドを Rust 型に落としてから
    // 一括でメモリを確保して、パース失敗時には確保処理に到達しないようにする

    // フェーズ 1: すべての JSON フィールドを Rust 型に落とす（メモリ確保前）
    let default_style = value.to_member("default_style")?.required()?;

    // ftab は `{ "font_id": u16, "font_name": [u8; N] }` オブジェクトの配列。
    // 要素ごとに 2 値あるため一度 Vec に組んでから unzip する
    let ftab_value = value.to_member("ftab")?.required()?;
    let ftab_pairs: Vec<(u16, Vec<u8>)> = ftab_value
        .to_array()?
        .map(|entry| {
            let font_id: u16 = entry.to_member("font_id")?.required()?.try_into()?;
            let font_name: Vec<u8> = entry.to_member("font_name")?.required()?.try_into()?;
            Ok((font_id, font_name))
        })
        .collect::<Result<_, nojson::JsonParseError>>()?;
    let (font_ids_vec, font_names_vec): (Vec<u16>, Vec<Vec<u8>>) = ftab_pairs.into_iter().unzip();

    let display_flags: u32 = value.to_member("display_flags")?.required()?.try_into()?;
    let horizontal_justification: i8 = value
        .to_member("horizontal_justification")?
        .required()?
        .try_into()?;
    let vertical_justification: i8 = value
        .to_member("vertical_justification")?
        .required()?
        .try_into()?;
    let background_color_rgba: [u8; 4] = value
        .to_member("background_color_rgba")?
        .required()?
        .try_into()?;
    let default_text_box: [i16; 4] = value
        .to_member("default_text_box")?
        .required()?
        .try_into()?;
    let default_style_start_char: u16 = default_style
        .to_member("start_char")?
        .required()?
        .try_into()?;
    let default_style_end_char: u16 = default_style
        .to_member("end_char")?
        .required()?
        .try_into()?;
    let default_style_font_id: u16 = default_style.to_member("font_id")?.required()?.try_into()?;
    let default_style_face_style_flags: u8 = default_style
        .to_member("face_style_flags")?
        .required()?
        .try_into()?;
    let default_style_font_size: u8 = default_style
        .to_member("font_size")?
        .required()?
        .try_into()?;
    let default_style_text_color_rgba: [u8; 4] = default_style
        .to_member("text_color_rgba")?
        .required()?
        .try_into()?;

    // フェーズ 2: すべてのパースが成功したときだけメモリを確保する
    let (ftab_font_ids, ftab_count) = crate::boxes::allocate_and_copy_u16_array(&font_ids_vec);
    let (ftab_font_name_ptrs, ftab_font_name_sizes, _) =
        crate::boxes::allocate_and_copy_array_list(&font_names_vec);

    Ok(Mp4SampleEntryTx3g {
        display_flags,
        horizontal_justification,
        vertical_justification,
        background_color_rgba,
        default_text_box,
        default_style_start_char,
        default_style_end_char,
        default_style_font_id,
        default_style_face_style_flags,
        default_style_font_size,
        default_style_text_color_rgba,
        ftab_font_ids,
        ftab_font_name_ptrs,
        ftab_font_name_sizes,
        ftab_count,
    })
}

/// tx3g サンプルエントリーのメモリを解放する
///
/// [`parse_json_mp4_sample_entry_tx3g()`] で割り当てられた ftab 系バッファを解放する。
/// 固定サイズフィールドは解放不要
pub fn mp4_sample_entry_tx3g_free(entry: &mut Mp4SampleEntryTx3g) {
    unsafe {
        crate::boxes::free_u16_array(entry.ftab_font_ids as *mut u16, entry.ftab_count);
        crate::boxes::free_array_list(
            entry.ftab_font_name_ptrs as *mut *mut u8,
            entry.ftab_font_name_sizes as *mut u32,
            entry.ftab_count,
        );
    }
    entry.ftab_font_ids = std::ptr::null();
    entry.ftab_font_name_ptrs = std::ptr::null();
    entry.ftab_font_name_sizes = std::ptr::null();
    entry.ftab_count = 0;
}

/// tx3g `ftab` の JSON シリアライズ用構造体
struct FtabList {
    font_ids: *const u16,
    font_name_ptrs: *const *const u8,
    font_name_sizes: *const u32,
    count: u32,
}

impl nojson::DisplayJson for FtabList {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.array(|f| {
            for i in 0..self.count as usize {
                let font_id = unsafe { *self.font_ids.add(i) };
                let name_ptr = unsafe { *self.font_name_ptrs.add(i) };
                let name_size = unsafe { *self.font_name_sizes.add(i) } as usize;
                // パース時に格納されたポインタ／サイズを読む（ここでは確保しない）。
                // 空要素は (null, 0)。`from_raw_parts` は size 0 でも非 null ポインタを
                // 要求するため、size == 0 の枝を先に落として空配列として出力する
                let font_name = if name_size == 0 {
                    &[][..]
                } else {
                    unsafe { std::slice::from_raw_parts(name_ptr, name_size) }
                };
                f.element(nojson::json(|f| {
                    f.object(|f| {
                        f.member("font_id", font_id)?;
                        f.member("font_name", font_name)
                    })
                }))?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx3g_to_json() {
        // ローカル変数のアドレスを Mp4SampleEntryTx3g に格納するため、
        // sample_entry の生存期間中は font_ids / font_name_ptrs / font_name_sizes が
        // ムーブされないようにテスト関数内でそのまま束縛する
        static FONT_NAME: &[u8] = b"Serif";
        let font_ids: [u16; 1] = [1];
        let font_name_ptrs: [*const u8; 1] = [FONT_NAME.as_ptr()];
        let font_name_sizes: [u32; 1] = [FONT_NAME.len() as u32];
        let sample_entry = Mp4SampleEntryTx3g {
            display_flags: 0,
            horizontal_justification: 0,
            vertical_justification: 0,
            background_color_rgba: [0, 0, 0, 255],
            default_text_box: [0, 0, 240, 320],
            default_style_start_char: 0,
            default_style_end_char: 0,
            default_style_font_id: 1,
            default_style_face_style_flags: 0,
            default_style_font_size: 12,
            default_style_text_color_rgba: [255, 255, 255, 255],
            ftab_font_ids: font_ids.as_ptr(),
            ftab_font_name_ptrs: font_name_ptrs.as_ptr(),
            ftab_font_name_sizes: font_name_sizes.as_ptr(),
            ftab_count: 1,
        };

        let json = nojson::json(|f| fmt_json_mp4_sample_entry_tx3g(f, &sample_entry)).to_string();
        assert!(json.contains(r#""kind":"tx3g""#));
        assert!(json.contains(r#""display_flags":0"#));
        assert!(json.contains(r#""horizontal_justification":0"#));
        assert!(json.contains(r#""vertical_justification":0"#));
        assert!(json.contains(r#""background_color_rgba":[0,0,0,255]"#));
        assert!(json.contains(r#""default_text_box":[0,0,240,320]"#));
        assert!(json.contains(r#""default_style":{"start_char":0"#));
        assert!(json.contains(r#""font_id":1"#));
        assert!(json.contains(r#""font_name":[83,101,114,105,102]"#));
    }

    #[test]
    fn test_json_to_tx3g_and_free() {
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 255],
            "default_text_box": [0, 0, 240, 320],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 12,
                "text_color_rgba": [255, 255, 255, 255]
            },
            "ftab": [
                { "font_id": 1, "font_name": [83, 101, 114, 105, 102] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_tx3g(json.value()).expect("有効な tx3g JSON");

        assert_eq!(sample_entry.display_flags, 0);
        assert_eq!(sample_entry.horizontal_justification, 0);
        assert_eq!(sample_entry.background_color_rgba, [0, 0, 0, 255]);
        assert_eq!(sample_entry.default_text_box, [0, 0, 240, 320]);
        assert_eq!(sample_entry.default_style_font_id, 1);
        assert_eq!(sample_entry.default_style_font_size, 12);
        assert_eq!(sample_entry.ftab_count, 1);

        // ftab の中身を検証する
        let font_id = unsafe { *sample_entry.ftab_font_ids };
        assert_eq!(font_id, 1);
        let name_ptr = unsafe { *sample_entry.ftab_font_name_ptrs };
        let name_size = unsafe { *sample_entry.ftab_font_name_sizes } as usize;
        let font_name = unsafe { std::slice::from_raw_parts(name_ptr, name_size) };
        assert_eq!(font_name, b"Serif");

        // メモリ解放
        mp4_sample_entry_tx3g_free(&mut sample_entry);
        assert_eq!(sample_entry.ftab_count, 0);
        assert!(sample_entry.ftab_font_ids.is_null());
        assert!(sample_entry.ftab_font_name_ptrs.is_null());
        assert!(sample_entry.ftab_font_name_sizes.is_null());
    }

    #[test]
    fn test_tx3g_json_roundtrip_with_empty_ftab() {
        // ftab が空のケースでもラウンドトリップが成立する
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 0,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": []
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_tx3g(json.value()).expect("有効な tx3g JSON");
        assert_eq!(sample_entry.ftab_count, 0);

        // 空 ftab のラウンドトリップも問題なく成立する
        let out_json =
            nojson::json(|f| fmt_json_mp4_sample_entry_tx3g(f, &sample_entry)).to_string();
        assert!(out_json.contains(r#""ftab":[]"#));

        mp4_sample_entry_tx3g_free(&mut sample_entry);
    }

    /// 空の font_name 要素を含む ftab を parse → JSON 再出力する往復テスト
    ///
    /// `allocate_and_copy_array_list` は空要素を `(null, 0)` にする。
    /// `FtabList::fmt` が `from_raw_parts(null, 0)` を呼ぶと UB になるため、
    /// ガード後も空配列として再出力できることを検証する
    #[test]
    fn test_tx3g_json_roundtrip_with_empty_font_name() {
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": [
                { "font_id": 1, "font_name": [] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_tx3g(json.value()).expect("空 font_name の tx3g JSON");

        assert_eq!(sample_entry.ftab_count, 1);
        assert_eq!(unsafe { *sample_entry.ftab_font_name_sizes }, 0);
        assert!(unsafe { (*sample_entry.ftab_font_name_ptrs).is_null() });

        let out = nojson::json(|f| fmt_json_mp4_sample_entry_tx3g(f, &sample_entry)).to_string();
        assert!(
            out.contains(r#""font_name":[]"#),
            "空 font_name が [] として再出力されること: {out}"
        );

        mp4_sample_entry_tx3g_free(&mut sample_entry);
        assert_eq!(sample_entry.ftab_count, 0);
        assert!(sample_entry.ftab_font_name_ptrs.is_null());
        assert!(sample_entry.ftab_font_name_sizes.is_null());
    }

    #[test]
    fn test_tx3g_json_roundtrip_with_multiple_fonts() {
        // ftab に複数エントリを持たせて、順序と値が保持されることを検証する
        let json_str = r#"{
            "kind": "tx3g",
            "display_flags": 0,
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 0],
            "default_text_box": [0, 0, 0, 0],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 0,
                "face_style_flags": 0,
                "font_size": 0,
                "text_color_rgba": [0, 0, 0, 0]
            },
            "ftab": [
                { "font_id": 1, "font_name": [65, 66] },
                { "font_id": 2, "font_name": [67, 68, 69] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let mut sample_entry =
            parse_json_mp4_sample_entry_tx3g(json.value()).expect("有効な tx3g JSON");
        assert_eq!(sample_entry.ftab_count, 2);

        // 順序と値の検証
        let id0 = unsafe { *sample_entry.ftab_font_ids };
        let id1 = unsafe { *sample_entry.ftab_font_ids.add(1) };
        assert_eq!(id0, 1);
        assert_eq!(id1, 2);
        let name0_ptr = unsafe { *sample_entry.ftab_font_name_ptrs };
        let name0_size = unsafe { *sample_entry.ftab_font_name_sizes } as usize;
        let name0 = unsafe { std::slice::from_raw_parts(name0_ptr, name0_size) };
        assert_eq!(name0, b"AB");
        let name1_ptr = unsafe { *sample_entry.ftab_font_name_ptrs.add(1) };
        let name1_size = unsafe { *sample_entry.ftab_font_name_sizes.add(1) } as usize;
        let name1 = unsafe { std::slice::from_raw_parts(name1_ptr, name1_size) };
        assert_eq!(name1, b"CDE");

        mp4_sample_entry_tx3g_free(&mut sample_entry);
    }

    #[test]
    fn test_json_to_tx3g_rejects_missing_display_flags_after_ftab() {
        // ftab / default_style は揃っているが後段の必須フィールド display_flags が欠落している。
        // 全フィールドを Rust 型に落としてからメモリ確保する順序なので、
        // この失敗経路では確保処理に到達せず Err だけが返る
        let json_str = r#"{
            "kind": "tx3g",
            "horizontal_justification": 0,
            "vertical_justification": 0,
            "background_color_rgba": [0, 0, 0, 255],
            "default_text_box": [0, 0, 240, 320],
            "default_style": {
                "start_char": 0,
                "end_char": 0,
                "font_id": 1,
                "face_style_flags": 0,
                "font_size": 12,
                "text_color_rgba": [255, 255, 255, 255]
            },
            "ftab": [
                { "font_id": 1, "font_name": [83, 101, 114, 105, 102] }
            ]
        }"#;

        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let result = parse_json_mp4_sample_entry_tx3g(json.value());
        assert!(result.is_err(), "display_flags 欠落時はパース失敗すること");
    }
}
