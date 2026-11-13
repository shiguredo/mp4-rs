//! サンプルエントリーの JSON 変換を束ねるモジュール
//!
//! コーデックごとの変換は `boxes_<codec>` モジュールが担当する。
//! このモジュールは `SampleEntry` のバリアントから対応するモジュールを選ぶだけの役割を持つ。
use shiguredo_mp4::boxes::{SampleEntry, VisualSampleEntryFields};

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
