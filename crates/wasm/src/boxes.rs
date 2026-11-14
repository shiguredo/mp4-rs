//! サンプルエントリーの JSON 変換を束ねるモジュール
//!
//! コーデックごとの変換は `boxes_<codec>` モジュールが担当する。
//! このモジュールは次の 2 つの役割を持つ。
//!
//! - `SampleEntry` のバリアントから対応するモジュールを選ぶ
//! - 複数のコーデックで共通する JSON 変換のヘルパを提供する
use shiguredo_mp4::{
    FixedPointNumber, Uint,
    boxes::{SampleEntry, VisualSampleEntryFields},
};

/// 映像系サンプルエントリーの共通フィールドを既定値で構築する
///
/// JSON には幅と高さだけが含まれるため、それ以外のフィールドは規格上の既定値で埋める。
pub(crate) fn build_visual_sample_entry_fields(width: u16, height: u16) -> VisualSampleEntryFields {
    VisualSampleEntryFields {
        data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
        width,
        height,
        horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
        vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
        frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
        depth: VisualSampleEntryFields::DEFAULT_DEPTH,
    }
}

/// 音声系サンプルエントリーのサンプリングレートを JSON の数値に変換する
///
/// [`FixedPointNumber`] は 16.16 固定小数点数なので、整数部と小数部を合わせて
/// 1 つの実数として返す。
pub(crate) fn sample_rate_to_json(sample_rate: FixedPointNumber<u16, u16>) -> f64 {
    f64::from(sample_rate.integer) + f64::from(sample_rate.fraction) / 65536.0
}

/// JSON の `sampleRate` を音声系サンプルエントリーのサンプリングレートに変換する
///
/// JSON では 16.16 固定小数点数を 1 つの実数として表現しているため、
/// 実数を固定小数点のビット列に直してから整数部と小数部に分解する。
pub(crate) fn sample_rate_from_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<FixedPointNumber<u16, u16>, nojson::JsonParseError> {
    let sample_rate_value = value.to_member("sampleRate")?.required()?;
    let sample_rate: f64 = sample_rate_value.try_into()?;
    if !(0.0..65536.0).contains(&sample_rate) {
        return Err(sample_rate_value.invalid("sampleRate must be in the range [0, 65536)"));
    }

    let fixed_point = (sample_rate * 65536.0).round() as u32;
    Ok(FixedPointNumber::new(
        (fixed_point >> 16) as u16,
        (fixed_point & 0xFFFF) as u16,
    ))
}

/// JSON の真偽値を読み取る
///
/// JSON の boolean に加えて、旧実装が出力していた `0` / `1` の数値も受理する。
/// デマルチプレックス側の出力とマルチプレックス側の入力を往復させるために必要である。
pub(crate) fn bool_or_int_member(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<bool, nojson::JsonParseError> {
    let member = value.to_member(name)?.required()?;
    match member.kind() {
        nojson::JsonValueKind::Boolean => member.try_into(),
        nojson::JsonValueKind::Integer => {
            let number: u8 = member.try_into()?;
            match number {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(member.invalid("must be a boolean or 0/1")),
            }
        }
        _ => Err(member.invalid("must be a boolean or 0/1")),
    }
}

/// JSON の数値を `BITS` ビットで表現できるか検証する
///
/// `Uint::new()` は範囲を検証しないため、はみ出した値はエンコード時に
/// 隣接フィールドのビットを侵食する。ここで入口の時点で拒否する。
fn check_uint_bits(
    number: u64,
    bits: u32,
    name: &str,
    member: nojson::RawJsonValue<'_, '_>,
) -> Result<(), nojson::JsonParseError> {
    if bits < 64 {
        let limit = 1u64 << bits;
        if number >= limit {
            let message = format!("{name} must be less than {limit}");
            return Err(member.invalid(message.as_str()));
        }
    }
    Ok(())
}

/// メンバーを取り直してエラーを作る
///
/// 値の検証をした後に、そのフィールド名を含むエラーを返すために使う。
pub(crate) fn invalid_member(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
    message: &str,
) -> nojson::JsonParseError {
    match value.to_member(name).and_then(|member| member.required()) {
        Ok(member) => member.invalid(message),
        Err(error) => error,
    }
}

/// JSON の数値を、指定したビット幅の `Uint<u8, BITS, OFFSET>` として読み取る
pub(crate) fn uint_u8_member<const BITS: u32, const OFFSET: u32>(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Uint<u8, BITS, OFFSET>, nojson::JsonParseError> {
    let member = value.to_member(name)?.required()?;
    let number: u8 = member.try_into()?;
    check_uint_bits(u64::from(number), BITS, name, member)?;
    Ok(Uint::new(number))
}

/// JSON の数値を、指定したビット幅の `Uint<u16, BITS, OFFSET>` として読み取る
pub(crate) fn uint_u16_member<const BITS: u32, const OFFSET: u32>(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Uint<u16, BITS, OFFSET>, nojson::JsonParseError> {
    let member = value.to_member(name)?.required()?;
    let number: u16 = member.try_into()?;
    check_uint_bits(u64::from(number), BITS, name, member)?;
    Ok(Uint::new(number))
}

/// JSON の数値を、指定したビット幅の `Uint<u64, BITS, OFFSET>` として読み取る
pub(crate) fn uint_u64_member<const BITS: u32, const OFFSET: u32>(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Uint<u64, BITS, OFFSET>, nojson::JsonParseError> {
    let member = value.to_member(name)?.required()?;
    let number: u64 = member.try_into()?;
    check_uint_bits(number, BITS, name, member)?;
    Ok(Uint::new(number))
}

/// 省略可能な数値を、指定したビット幅の `Uint<u8, BITS, OFFSET>` として読み取る
///
/// キーが存在しない場合と値が `null` の場合のどちらも `None` として扱う。
pub(crate) fn optional_uint_u8_member<const BITS: u32, const OFFSET: u32>(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Option<Uint<u8, BITS, OFFSET>>, nojson::JsonParseError> {
    match value.to_member(name)?.optional() {
        None => Ok(None),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Ok(None),
        Some(member) => {
            let number: u8 = member.try_into()?;
            check_uint_bits(u64::from(number), BITS, name, member)?;
            Ok(Some(Uint::new(number)))
        }
    }
}

/// サンプルエントリーを JSON に変換する
pub(crate) fn fmt_json(
    f: &mut nojson::JsonFormatter<'_, '_>,
    sample_entry: &SampleEntry,
) -> std::fmt::Result {
    match sample_entry {
        SampleEntry::Avc1(b) => crate::boxes_avc1::fmt_json(f, b),
        SampleEntry::Hev1(b) => crate::boxes_hev1::fmt_json(f, b),
        SampleEntry::Hvc1(b) => crate::boxes_hvc1::fmt_json(f, b),
        SampleEntry::Vp08(b) => crate::boxes_vp08::fmt_json(f, b),
        SampleEntry::Vp09(b) => crate::boxes_vp09::fmt_json(f, b),
        SampleEntry::Av01(b) => crate::boxes_av01::fmt_json(f, b),
        SampleEntry::Opus(b) => crate::boxes_opus::fmt_json(f, b),
        SampleEntry::Mp4a(b) => crate::boxes_mp4a::fmt_json(f, b),
        SampleEntry::Flac(b) => crate::boxes_flac::fmt_json(f, b),
        SampleEntry::Stpp(b) => crate::boxes_stpp::fmt_json(f, b),
        SampleEntry::Wvtt(b) => crate::boxes_wvtt::fmt_json(f, b),
        SampleEntry::Tx3g(b) => crate::boxes_tx3g::fmt_json(f, b),
        // 未知のサンプルエントリーは種別だけを返す。
        // ここでエラーにすると、未対応コーデックを含むファイルの他のトラックまで
        // 読めなくなってしまうため、利用側が判断できる形で情報を残す
        SampleEntry::Unknown(b) => f.object(|f| {
            f.member("kind", "unknown")?;
            f.member("boxType", b.box_type.to_string())
        }),
    }
}

/// JSON からサンプルエントリーに変換する
///
/// `kind` の文字列でコーデックを判別する。
/// 未知のサンプルエントリーの出力 (`"kind": "unknown"`) は受理しない。
pub(crate) fn parse_json(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<SampleEntry, nojson::JsonParseError> {
    let kind_value = value.to_member("kind")?.required()?;
    let kind_str = kind_value.to_unquoted_string_str()?;

    match kind_str.as_ref() {
        "avc1" => Ok(SampleEntry::Avc1(crate::boxes_avc1::parse_json(value)?)),
        "hev1" => Ok(SampleEntry::Hev1(crate::boxes_hev1::parse_json(value)?)),
        "hvc1" => Ok(SampleEntry::Hvc1(crate::boxes_hvc1::parse_json(value)?)),
        "vp08" => Ok(SampleEntry::Vp08(crate::boxes_vp08::parse_json(value)?)),
        "vp09" => Ok(SampleEntry::Vp09(crate::boxes_vp09::parse_json(value)?)),
        "av01" => Ok(SampleEntry::Av01(crate::boxes_av01::parse_json(value)?)),
        "opus" => Ok(SampleEntry::Opus(crate::boxes_opus::parse_json(value)?)),
        "mp4a" => Ok(SampleEntry::Mp4a(crate::boxes_mp4a::parse_json(value)?)),
        "flac" => Ok(SampleEntry::Flac(crate::boxes_flac::parse_json(value)?)),
        "stpp" => Ok(SampleEntry::Stpp(crate::boxes_stpp::parse_json(value)?)),
        "wvtt" => Ok(SampleEntry::Wvtt(crate::boxes_wvtt::parse_json(value)?)),
        "tx3g" => Ok(SampleEntry::Tx3g(crate::boxes_tx3g::parse_json(value)?)),
        _ => Err(kind_value.invalid("unknown sample entry kind")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全コーデックのサンプルエントリーを表現する JSON の一覧
    ///
    /// ディスパッチの配線漏れを検出するため、`kind` ごとに 1 件ずつ用意する。
    fn sample_entry_json_list() -> Vec<&'static str> {
        vec![
            r#"{"kind":"avc1","width":640,"height":480,"avcProfileIndication":66,"profileCompatibility":0,"avcLevelIndication":30,"lengthSizeMinusOne":3,"sps":[[103,66,0,30]],"pps":[[104,206,60]]}"#,
            r#"{"kind":"hev1","width":640,"height":480,"generalProfileSpace":0,"generalTierFlag":0,"generalProfileIdc":2,"generalProfileCompatibilityFlags":1610612736,"generalConstraintIndicatorFlags":193514046488576,"generalLevelIdc":120,"chromaFormatIdc":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"minSpatialSegmentationIdc":0,"parallelismType":0,"avgFrameRate":0,"constantFrameRate":0,"numTemporalLayers":1,"temporalIdNested":1,"lengthSizeMinusOne":3,"naluArrays":[{"naluType":32,"units":[[64,1,12,1]]}]}"#,
            r#"{"kind":"hvc1","width":640,"height":480,"generalProfileSpace":0,"generalTierFlag":0,"generalProfileIdc":2,"generalProfileCompatibilityFlags":1610612736,"generalConstraintIndicatorFlags":193514046488576,"generalLevelIdc":120,"chromaFormatIdc":1,"bitDepthLumaMinus8":0,"bitDepthChromaMinus8":0,"minSpatialSegmentationIdc":0,"parallelismType":0,"avgFrameRate":0,"constantFrameRate":0,"numTemporalLayers":1,"temporalIdNested":1,"lengthSizeMinusOne":3,"naluArrays":[{"naluType":32,"units":[[64,1,12,1]]}]}"#,
            r#"{"kind":"vp08","width":640,"height":480,"bitDepth":8,"chromaSubsampling":1,"videoFullRangeFlag":false,"colourPrimaries":1,"transferCharacteristics":1,"matrixCoefficients":1}"#,
            r#"{"kind":"vp09","width":640,"height":480,"profile":0,"level":31,"bitDepth":8,"chromaSubsampling":1,"videoFullRangeFlag":false,"colourPrimaries":1,"transferCharacteristics":1,"matrixCoefficients":1}"#,
            r#"{"kind":"av01","width":640,"height":480,"seqProfile":0,"seqLevelIdx0":13,"seqTier0":0,"highBitdepth":0,"twelveBit":0,"monochrome":0,"chromaSubsamplingX":1,"chromaSubsamplingY":1,"chromaSamplePosition":0,"configObus":[10,11]}"#,
            r#"{"kind":"opus","channelCount":2,"sampleRate":48000,"sampleSize":16,"preSkip":0,"inputSampleRate":48000,"outputGain":0}"#,
            r#"{"kind":"mp4a","channelCount":2,"sampleRate":44100,"sampleSize":16,"bufferSizeDb":0,"maxBitrate":128000,"avgBitrate":128000,"decSpecificInfo":[1,2]}"#,
            r#"{"kind":"flac","channelCount":2,"sampleRate":44100,"sampleSize":16,"streaminfoData":[0,16,0,16]}"#,
            r#"{"kind":"stpp","namespace":"http://www.w3.org/ns/ttml","schemaLocation":"","auxiliaryMimeTypes":""}"#,
            r#"{"kind":"wvtt","config":"WEBVTT\n\n"}"#,
            r#"{"kind":"tx3g","display_flags":0,"horizontal_justification":0,"vertical_justification":0,"background_color_rgba":[0,0,0,255],"default_text_box":[0,0,240,320],"default_style":{"start_char":0,"end_char":0,"font_id":1,"face_style_flags":0,"font_size":12,"text_color_rgba":[255,255,255,255]},"ftab":[{"font_id":1,"font_name":[65]}]}"#,
        ]
    }

    #[test]
    fn test_all_kinds_roundtrip_through_dispatch() {
        // 12 コーデックすべてが fmt_json と parse_json の両方に配線されていること
        for json_str in sample_entry_json_list() {
            let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
            let sample_entry = parse_json(json.value()).expect("パースできること");

            let output = nojson::json(|f| fmt_json(f, &sample_entry)).to_string();
            let output_json = nojson::RawJson::parse(&output).expect("出力が JSON であること");
            let roundtripped = parse_json(output_json.value()).expect("出力を再パースできること");

            let output2 = nojson::json(|f| fmt_json(f, &roundtripped)).to_string();
            assert_eq!(
                output, output2,
                "往復して JSON が変化しないこと: {json_str}"
            );
        }
    }

    #[test]
    fn test_unknown_kind_is_rejected() {
        let json_str = r#"{"kind":"unknown","boxType":"zzzz"}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON であること");
        let error = parse_json(json.value()).expect_err("unknown は受理されないこと");
        assert!(
            error.to_string().contains("unknown sample entry kind"),
            "{error}"
        );
    }
}
