//! マルチプレクサへ渡すサンプル JSON の解釈
//!
//! `Mp4FileMuxer` と `Fmp4SegmentMuxer` は同じサンプル JSON を受け取る
//! （`Fmp4SegmentMuxer` は `data_offset` を使わない）ため、解釈をここに集約する。
//! スキーマは `crates/wasm/README.md` に記載している。
use std::num::NonZeroU32;

use shiguredo_mp4::{TrackKind, boxes::SampleEntry};

/// サンプル JSON のうち、マルチプレクサが共通して必要とする情報
#[derive(Debug)]
pub(crate) struct MuxSampleMetadata {
    /// トラック種別
    pub track_kind: TrackKind,
    /// サンプルのタイムスケール
    pub timescale: NonZeroU32,
    /// サンプルエントリー（同じトラックの直前のサンプルから変化した場合だけ指定する）
    pub sample_entry: Option<SampleEntry>,
    /// サンプルの長さ（タイムスケール単位）
    pub duration: u32,
    /// キーフレームかどうか
    pub keyframe: bool,
    /// コンポジション時間オフセット
    pub composition_time_offset: Option<i64>,
}

/// サンプル JSON から共通フィールドを読み取る
pub(crate) fn parse_metadata(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<MuxSampleMetadata, nojson::JsonParseError> {
    let track_kind = parse_track_kind(value)?;
    let timescale = parse_timescale(value)?;
    let duration: u32 = value.to_member("duration")?.required()?.try_into()?;
    let keyframe: bool = value.to_member("keyframe")?.required()?.try_into()?;
    let composition_time_offset = optional_i64_member(value, "composition_time_offset")?;
    let sample_entry = optional_sample_entry(value)?;

    Ok(MuxSampleMetadata {
        track_kind,
        timescale,
        sample_entry,
        duration,
        keyframe,
        composition_time_offset,
    })
}

/// `track_kind` を読み取る
fn parse_track_kind(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<TrackKind, nojson::JsonParseError> {
    let track_kind_value = value.to_member("track_kind")?.required()?;
    match track_kind_value.to_unquoted_string_str()?.as_ref() {
        "audio" => Ok(TrackKind::Audio),
        "video" => Ok(TrackKind::Video),
        "subtitle" => Ok(TrackKind::Subtitle),
        _ => Err(track_kind_value.invalid("must be \"audio\", \"video\", or \"subtitle\"")),
    }
}

/// `timescale` を読み取る
///
/// 0 はタイムスケールとして無効なので拒否する。
fn parse_timescale(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<NonZeroU32, nojson::JsonParseError> {
    let timescale_member = value.to_member("timescale")?.required()?;
    let timescale_value: u32 = timescale_member.try_into()?;
    NonZeroU32::new(timescale_value)
        .ok_or_else(|| timescale_member.invalid("must be greater than 0"))
}

/// 省略可能な整数メンバーを読み取る
///
/// キーが存在しない場合と値が `null` の場合のどちらも `None` として扱う。
/// デマルチプレックス側は `composition_time_offset` に `null` を出力するため、
/// そのままマルチプレックス側へ渡せるようにするにはこの解釈が必要になる。
fn optional_i64_member(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<Option<i64>, nojson::JsonParseError> {
    match value.to_member(name)?.optional() {
        None => Ok(None),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Ok(None),
        Some(member) => Ok(Some(member.try_into()?)),
    }
}

/// 省略可能な `sample_entry` を読み取る
///
/// キーが存在しない場合と値が `null` の場合のどちらも「変化なし」として扱う。
fn optional_sample_entry(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Option<SampleEntry>, nojson::JsonParseError> {
    match value.to_member("sample_entry")?.optional() {
        None => Ok(None),
        Some(member) if member.kind() == nojson::JsonValueKind::Null => Ok(None),
        Some(member) => Ok(Some(crate::boxes::parse_json(member)?)),
    }
}

/// 必須の非負整数メンバーを `usize` として読み取る
///
/// wasm32 では `usize` が 32 bit なので、`u64` で受けてから変換する。
pub(crate) fn required_usize_member(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<usize, nojson::JsonParseError> {
    let member = value.to_member(name)?.required()?;
    let number: u64 = member.try_into()?;
    usize::try_from(number).map_err(|_| member.invalid("must fit in usize"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 共通フィールドだけを持つ最小のサンプル JSON
    const MINIMAL_JSON: &str = r#"{
        "track_kind": "video",
        "timescale": 90000,
        "duration": 3000,
        "keyframe": true,
        "data_size": 1024
    }"#;

    fn parse(text: &str) -> Result<MuxSampleMetadata, nojson::JsonParseError> {
        let raw_json = nojson::RawJson::parse(text)?;
        parse_metadata(raw_json.value())
    }

    #[test]
    fn test_parse_minimal_sample() {
        let metadata = parse(MINIMAL_JSON).expect("最小のサンプルをパースできること");
        assert!(matches!(metadata.track_kind, TrackKind::Video));
        assert_eq!(metadata.timescale.get(), 90000);
        assert_eq!(metadata.duration, 3000);
        assert!(metadata.keyframe);
        assert!(metadata.sample_entry.is_none());
        assert!(metadata.composition_time_offset.is_none());
    }

    #[test]
    fn test_parse_accepts_null_composition_time_offset() {
        let json = r#"{
            "track_kind": "audio",
            "timescale": 48000,
            "duration": 960,
            "keyframe": true,
            "composition_time_offset": null
        }"#;
        let metadata = parse(json).expect("null を受理すること");
        assert!(metadata.composition_time_offset.is_none());
    }

    #[test]
    fn test_parse_keeps_composition_time_offset() {
        let json = r#"{
            "track_kind": "video",
            "timescale": 90000,
            "duration": 3000,
            "keyframe": true,
            "composition_time_offset": -1500
        }"#;
        let metadata = parse(json).expect("負の値を保持できること");
        assert_eq!(metadata.composition_time_offset, Some(-1500));
    }

    #[test]
    fn test_parse_accepts_null_sample_entry() {
        let json = r#"{
            "track_kind": "video",
            "timescale": 90000,
            "duration": 3000,
            "keyframe": true,
            "sample_entry": null
        }"#;
        let metadata = parse(json).expect("null を受理すること");
        assert!(metadata.sample_entry.is_none());
    }

    #[test]
    fn test_parse_rejects_unknown_track_kind() {
        let json = MINIMAL_JSON.replace("video", "text");
        let error = parse(&json).expect_err("未知の種別は拒否されること");
        assert!(error.to_string().contains("must be"), "{error}");
    }

    #[test]
    fn test_parse_rejects_zero_timescale() {
        let json = MINIMAL_JSON.replace("90000", "0");
        let error = parse(&json).expect_err("0 は拒否されること");
        assert!(error.to_string().contains("greater than 0"), "{error}");
    }

    #[test]
    fn test_required_usize_member() {
        let json = r#"{"data_size": 1024}"#;
        let raw_json = nojson::RawJson::parse(json).expect("有効な JSON");
        assert_eq!(
            required_usize_member(raw_json.value(), "data_size").expect("1024 を受理すること"),
            1024
        );
    }

    #[test]
    fn test_required_usize_member_rejects_missing() {
        let json = r#"{}"#;
        let raw_json = nojson::RawJson::parse(json).expect("有効な JSON");
        assert!(required_usize_member(raw_json.value(), "data_size").is_err());
    }
}
