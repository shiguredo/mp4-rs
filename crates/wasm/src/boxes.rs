//! サンプルエントリーの JSON 変換を束ねるモジュール
//!
//! コーデックごとの変換は `boxes_<codec>` モジュールが担当する。
//! このモジュールは次の 2 つの役割を持つ。
//!
//! - `SampleEntry` のバリアントから対応するモジュールを選ぶ
//! - 複数のコーデックで共通する JSON 変換のヘルパを提供する
use shiguredo_mp4::boxes::{
    AudioSampleEntryFields, SampleEntry, UnknownBox, VisualSampleEntryFields,
};
use shiguredo_mp4::{BoxSize, BoxType, FixedPointNumber, Uint};

/// テスト用に映像系サンプルエントリーの共通フィールドを既定値で構築する
#[cfg(test)]
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

/// 映像系サンプルエントリーの共通フィールドを JSON に出力する
pub(crate) fn fmt_visual_sample_entry_fields(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    visual: &VisualSampleEntryFields,
) -> std::fmt::Result {
    f.member("dataReferenceIndex", visual.data_reference_index.get())?;
    f.member("width", visual.width)?;
    f.member("height", visual.height)?;
    f.member(
        "horizresolution",
        fixed_point_to_f64(visual.horizresolution),
    )?;
    f.member("vertresolution", fixed_point_to_f64(visual.vertresolution))?;
    f.member("frameCount", visual.frame_count)?;
    match compressorname_to_str(&visual.compressorname) {
        Some(name) => f.member("compressorname", name)?,
        None => f.member("compressorname", visual.compressorname.as_slice())?,
    }
    f.member("depth", visual.depth)
}

/// 映像系サンプルエントリーの共通フィールドを JSON から読み取る
pub(crate) fn parse_visual_sample_entry_fields(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<VisualSampleEntryFields, nojson::JsonParseError> {
    let width: u16 = value.to_member("width")?.required()?.try_into()?;
    let height: u16 = value.to_member("height")?.required()?.try_into()?;
    let data_reference_index = parse_non_zero_u16_member(
        value,
        "dataReferenceIndex",
        VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
    )?;
    let horizresolution = parse_fixed_point(value, "horizresolution")?
        .unwrap_or(VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION);
    let vertresolution = parse_fixed_point(value, "vertresolution")?
        .unwrap_or(VisualSampleEntryFields::DEFAULT_VERTRESOLUTION);
    let frame_count: u16 = match value.to_member("frameCount")?.optional() {
        None => VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
        Some(member) if member.kind() == nojson::JsonValueKind::Null => {
            VisualSampleEntryFields::DEFAULT_FRAME_COUNT
        }
        Some(member) => member.try_into()?,
    };
    let depth: u16 = match value.to_member("depth")?.optional() {
        None => VisualSampleEntryFields::DEFAULT_DEPTH,
        Some(member) if member.kind() == nojson::JsonValueKind::Null => {
            VisualSampleEntryFields::DEFAULT_DEPTH
        }
        Some(member) => member.try_into()?,
    };
    let compressorname = parse_compressorname(value)?;
    Ok(VisualSampleEntryFields {
        data_reference_index,
        width,
        height,
        horizresolution,
        vertresolution,
        frame_count,
        compressorname,
        depth,
    })
}

/// 省略可能な非ゼロの u16 を JSON から読み取る
pub(crate) fn parse_non_zero_u16_member(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
    default: std::num::NonZeroU16,
) -> Result<std::num::NonZeroU16, nojson::JsonParseError> {
    match value.to_member(name)?.optional() {
        None => Ok(default),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Ok(default),
        Some(member) => {
            let index: u16 = member.try_into()?;
            std::num::NonZeroU16::new(index)
                .ok_or_else(|| member.invalid(format!("{name} must be greater than or equal to 1")))
        }
    }
}

/// 音声系サンプルエントリーのデータ参照インデックスを JSON から読み取る
pub(crate) fn parse_audio_data_reference_index(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<std::num::NonZeroU16, nojson::JsonParseError> {
    parse_non_zero_u16_member(
        value,
        "dataReferenceIndex",
        AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
    )
}

/// 固定小数点数を JSON の数値に変換する
fn fixed_point_to_f64(value: FixedPointNumber<u16, u16>) -> f64 {
    f64::from(value.integer) + f64::from(value.fraction) / 65536.0
}

/// JSON の数値を固定小数点数として読み取る
///
/// キーが無い場合は `None` を返す。
fn parse_fixed_point(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Option<FixedPointNumber<u16, u16>>, nojson::JsonParseError> {
    let Some(member) = value.to_member(name)?.optional() else {
        return Ok(None);
    };
    if member.kind() == nojson::JsonValueKind::Null {
        return Ok(None);
    }
    let number: f64 = member.try_into()?;
    if !(0.0..=65535.0 + 65535.0 / 65536.0).contains(&number) {
        let message = format!("{name} is out of range");
        return Err(member.invalid(message.as_str()));
    }
    let fixed_point = (number * 65536.0).round();
    if fixed_point > f64::from(u32::MAX) {
        let message = format!("{name} is out of range");
        return Err(member.invalid(message.as_str()));
    }
    let fixed_point = fixed_point as u32;
    Ok(Some(FixedPointNumber::new(
        (fixed_point >> 16) as u16,
        (fixed_point & 0xFFFF) as u16,
    )))
}

/// コンプレッサー名を文字列として返す
///
/// 先頭 1 バイトが長さで、続くバイト列が名前という Pascal 文字列形式である。
/// 名前が UTF-8 として解釈できない場合は `None` を返し、生の 32 バイトを出力する。
fn compressorname_to_str(raw: &[u8; 32]) -> Option<String> {
    let length = usize::from(raw[0]).min(31);
    core::str::from_utf8(&raw[1..1 + length])
        .ok()
        .map(str::to_string)
}

/// コンプレッサー名を JSON から読み取る
fn parse_compressorname(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<[u8; 32], nojson::JsonParseError> {
    let Some(member) = value.to_member("compressorname")?.optional() else {
        return Ok(VisualSampleEntryFields::NULL_COMPRESSORNAME);
    };
    if member.kind() == nojson::JsonValueKind::Null {
        return Ok(VisualSampleEntryFields::NULL_COMPRESSORNAME);
    }
    if member.kind() == nojson::JsonValueKind::String {
        let name: String = member.try_into()?;
        let bytes = name.as_bytes();
        if bytes.len() > 31 {
            return Err(member.invalid("compressorname must be at most 31 bytes"));
        }
        let mut raw = [0u8; 32];
        raw[0] = bytes.len() as u8;
        raw[1..1 + bytes.len()].copy_from_slice(bytes);
        return Ok(raw);
    }
    let raw: [u8; 32] = member.try_into()?;
    // 長さバイトは 31 以下でなければ名前として解釈できない
    if raw[0] > 31 {
        return Err(member.invalid("compressorname length must be at most 31"));
    }
    Ok(raw)
}

/// 型付き実装を持たない子ボックスを JSON に出力する
pub(crate) fn fmt_unknown_boxes(
    f: &mut nojson::JsonObjectFormatter<'_, '_, '_>,
    unknown_boxes: &[UnknownBox],
) -> std::fmt::Result {
    f.member(
        "unknownBoxes",
        nojson::array(|f| {
            for unknown_box in unknown_boxes {
                f.element(nojson::object(|f| {
                    match unknown_box.box_type {
                        BoxType::Normal(ty) => match core::str::from_utf8(&ty) {
                            Ok(name) => f.member("type", name)?,
                            Err(_) => f.member("type", ty.as_slice())?,
                        },
                        BoxType::Uuid(_) => f.member("type", unknown_box.box_type.as_bytes())?,
                    }
                    f.member("data", unknown_box.payload.as_slice())
                }))?;
            }
            Ok(())
        }),
    )
}

/// 型付き実装を持たない子ボックスを JSON から読み取る
pub(crate) fn parse_unknown_boxes(
    value: nojson::RawJsonValue<'_, '_>,
    known_types: &[&[u8; 4]],
) -> Result<Vec<UnknownBox>, nojson::JsonParseError> {
    let Some(member) = value.to_member("unknownBoxes")?.optional() else {
        return Ok(Vec::new());
    };
    if member.kind() == nojson::JsonValueKind::Null {
        return Ok(Vec::new());
    }

    let mut unknown_boxes = Vec::new();
    for entry in member.to_array()? {
        let type_value = entry.to_member("type")?.required()?;
        let box_type = if type_value.kind() == nojson::JsonValueKind::String {
            let name: String = type_value.try_into()?;
            let bytes = name.as_bytes();
            if bytes.len() != 4 {
                return Err(type_value.invalid("type must be 4 bytes"));
            }
            let mut ty = [0u8; 4];
            ty.copy_from_slice(bytes);
            if &ty == b"uuid" {
                return Err(type_value.invalid("type \"uuid\" requires a 16-byte user type array"));
            }
            BoxType::Normal(ty)
        } else {
            let bytes: Vec<u8> = type_value.try_into()?;
            match bytes.len() {
                4 => {
                    let mut ty = [0u8; 4];
                    ty.copy_from_slice(&bytes);
                    if &ty == b"uuid" {
                        return Err(
                            type_value.invalid("type \"uuid\" requires a 16-byte user type array")
                        );
                    }
                    BoxType::Normal(ty)
                }
                16 => {
                    let mut ty = [0u8; 16];
                    ty.copy_from_slice(&bytes);
                    BoxType::Uuid(ty)
                }
                _ => return Err(type_value.invalid("type must be 4 or 16 bytes")),
            }
        };
        if let BoxType::Normal(ty) = box_type
            && known_types.contains(&&ty)
        {
            return Err(type_value
                .invalid("typed boxes must be specified with their own key, not unknownBoxes"));
        }
        let payload: Vec<u8> = entry.to_member("data")?.required()?.try_into()?;
        let box_size = BoxSize::with_payload_size(box_type, payload.len() as u64);
        unknown_boxes.push(UnknownBox {
            box_type,
            box_size,
            payload,
        });
    }
    Ok(unknown_boxes)
}

/// 音声系サンプルエントリーのサンプリングレートを JSON の数値に変換する
///
/// [`FixedPointNumber`] は 16.16 固定小数点数なので、整数部と小数部を合わせて
/// 1 つの実数として返す。
pub(crate) fn sample_rate_to_json(sample_rate: FixedPointNumber<u16, u16>) -> f64 {
    fixed_point_to_f64(sample_rate)
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

    /// 未知の子ボックスの type と data を読み取れること
    #[test]
    fn test_parse_unknown_boxes() {
        // 4 文字の文字列
        let json_str = r#"{"unknownBoxes":[{"type":"pasp","data":[0,0,0,1]}]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let boxes = parse_unknown_boxes(json.value(), &[]).expect("未知の子ボックス");
        assert_eq!(boxes.len(), 1);
        assert_eq!(boxes[0].box_type, BoxType::Normal(*b"pasp"));
        assert_eq!(boxes[0].payload, vec![0, 0, 0, 1]);
        assert_eq!(
            boxes[0].box_size,
            BoxSize::U32(8 + 4),
            "ヘッダ 8 バイトと payload 4 バイト"
        );

        // 4 バイトの数値配列
        let json_str = r#"{"unknownBoxes":[{"type":[112,97,115,112],"data":[]}]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        let boxes = parse_unknown_boxes(json.value(), &[]).expect("4 バイトの種別");
        assert_eq!(boxes[0].box_type, BoxType::Normal(*b"pasp"));

        // 16 バイトの UUID
        let uuid = (0..16).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        let json_str = format!(r#"{{"unknownBoxes":[{{"type":[{uuid}],"data":[1]}}]}}"#);
        let json = nojson::RawJson::parse(&json_str).expect("有効な JSON");
        let boxes = parse_unknown_boxes(json.value(), &[]).expect("UUID の種別");
        assert!(matches!(boxes[0].box_type, BoxType::Uuid(_)));
        assert_eq!(
            boxes[0].box_size,
            BoxSize::U32(24 + 1),
            "UUID のヘッダは 24 バイト"
        );

        // "uuid" を 4 バイト種別として指定すると、読み手は 16 バイトのユーザータイプを
        // 期待するため壊れたボックスになる。ここで拒否する
        let json_str = r#"{"unknownBoxes":[{"type":"uuid","data":[0]}]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        assert!(parse_unknown_boxes(json.value(), &[]).is_err());

        // 長さが 4 / 16 以外の配列は拒否する
        let json_str = r#"{"unknownBoxes":[{"type":[1,2,3],"data":[]}]}"#;
        let json = nojson::RawJson::parse(json_str).expect("有効な JSON");
        assert!(parse_unknown_boxes(json.value(), &[]).is_err());

        // キーが無ければ空
        let json = nojson::RawJson::parse(r#"{}"#).expect("有効な JSON");
        assert!(
            parse_unknown_boxes(json.value(), &[])
                .expect("空")
                .is_empty()
        );
    }
}
