//! muxer の JSON オプション解析
//!
//! `Mp4FileMuxer.withOptions()` と `Fmp4SegmentMuxer.withOptions()` が共有する。
//! スキーマは `crates/wasm/README.md` に記載している。
use std::num::NonZeroU32;
use std::time::Duration;

use shiguredo_mp4::{
    FixedPointNumber,
    boxes::{ElstBox, ElstEntry},
    mux::{EditList, TrackMetadata},
};
use wasm_bindgen::JsError;

/// MP4 の時刻の起点（1904 年 1 月 1 日）から UNIX エポックまでの秒数
pub(crate) const MP4_EPOCH_OFFSET_SECS: u64 = 2_082_844_800;

/// `reservedMoovBoxSize` の上限（512 MiB）
pub(crate) const MAX_RESERVED_MOOV_BOX_SIZE: usize = 512 * 1024 * 1024;

/// JSON オプションから読み取った muxer 設定
pub(crate) struct ParsedMuxerOptions {
    /// faststart 用に確保する moov のサイズ（未指定時は `None`）
    pub reserved_moov_box_size: Option<usize>,

    /// ファイル作成時刻
    pub creation_timestamp: Duration,

    /// 映像トラックのメタデータ
    pub video_track: TrackMetadata,

    /// 音声トラックのメタデータ
    pub audio_track: TrackMetadata,

    /// 字幕トラックのメタデータ
    pub subtitle_track: TrackMetadata,
}

/// JSON オプションを読み取る
pub(crate) fn parse(options_json: &str) -> Result<ParsedMuxerOptions, JsError> {
    crate::json_limit::check(options_json).map_err(|message| JsError::new(&message))?;
    let raw_json = nojson::RawJson::parse(options_json)
        .map_err(|error| JsError::new(&format!("failed to parse options JSON: {error}")))?;
    let value = raw_json.value();

    let reserved_moov_box_size = match optional_member(value, "reservedMoovBoxSize")? {
        Some(member) => {
            let size = u64::try_from(member).map_err(js_error)?;
            let size = usize::try_from(size)
                .map_err(|_| JsError::new("reservedMoovBoxSize exceeds usize::MAX"))?;
            if size > MAX_RESERVED_MOOV_BOX_SIZE {
                return Err(JsError::new("reservedMoovBoxSize must be at most 512 MiB"));
            }
            Some(size)
        }
        None => None,
    };

    let creation_timestamp_secs = match optional_member(value, "creationTimestampSecs")? {
        Some(member) => {
            let secs = u64::try_from(member).map_err(js_error)?;
            if secs > u64::MAX - MP4_EPOCH_OFFSET_SECS {
                return Err(JsError::new("creationTimestampSecs is too large"));
            }
            secs
        }
        None => 0,
    };

    Ok(ParsedMuxerOptions {
        reserved_moov_box_size,
        creation_timestamp: Duration::from_secs(creation_timestamp_secs),
        video_track: parse_track_metadata(value, "video")?,
        audio_track: parse_track_metadata(value, "audio")?,
        subtitle_track: parse_track_metadata(value, "subtitle")?,
    })
}

/// 省略可能なメンバーを読み取る
///
/// キーが存在しない場合と値が `null` の場合のどちらも `None` として扱う。
fn optional_member<'a, 'b>(
    value: nojson::RawJsonValue<'a, 'b>,
    name: &str,
) -> Result<Option<nojson::RawJsonValue<'a, 'b>>, JsError> {
    let member = value.to_member(name).map_err(js_error)?;
    Ok(member
        .optional()
        .filter(|member| member.kind() != nojson::JsonValueKind::Null))
}

/// トラック種別ごとのメタデータを読み取る
///
/// 現状は編集リストだけを指定できる。省略した場合は既定値になる。
fn parse_track_metadata(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<TrackMetadata, JsError> {
    let Some(member) = optional_member(value, name)? else {
        return Ok(TrackMetadata::default());
    };
    let edit_list = match optional_member(member, "edit_list")? {
        Some(edit_list_value) => Some(parse_edit_list(edit_list_value)?),
        None => None,
    };
    Ok(TrackMetadata {
        edit_list,
        ..Default::default()
    })
}

/// 編集リストを読み取る
fn parse_edit_list(value: nojson::RawJsonValue<'_, '_>) -> Result<EditList, JsError> {
    let movie_timescale = required_non_zero_u32(value, "movie_timescale")?;
    let media_timescale = required_non_zero_u32(value, "media_timescale")?;

    let entries_value = value
        .to_member("entries")
        .and_then(|member| member.required())
        .map_err(js_error)?;
    let mut entries = Vec::new();
    for entry in entries_value.to_array().map_err(js_error)? {
        let segment_duration: u64 = entry
            .to_member("segment_duration")
            .and_then(|member| member.required())
            .map_err(js_error)?
            .try_into()
            .map_err(js_error)?;
        let media_time: i64 = entry
            .to_member("media_time")
            .and_then(|member| member.required())
            .map_err(js_error)?
            .try_into()
            .map_err(js_error)?;
        let media_rate: f64 = entry
            .to_member("media_rate")
            .and_then(|member| member.required())
            .map_err(js_error)?
            .try_into()
            .map_err(js_error)?;
        entries.push(ElstEntry {
            edit_duration: segment_duration,
            media_time,
            media_rate: parse_media_rate(media_rate)?,
        });
    }

    Ok(EditList {
        movie_timescale,
        media_timescale,
        elst_box: ElstBox { entries },
    })
}

/// 必須の非ゼロ `u32` メンバーを読み取る
fn required_non_zero_u32(
    value: nojson::RawJsonValue<'_, '_>,
    name: &str,
) -> Result<NonZeroU32, JsError> {
    let member = value
        .to_member(name)
        .and_then(|member| member.required())
        .map_err(js_error)?;
    let number = u32::try_from(member).map_err(js_error)?;
    NonZeroU32::new(number).ok_or_else(|| JsError::new(&format!("{name} must be greater than 0")))
}

/// `media_rate` を i16.16 の固定小数点数として読み取る
fn parse_media_rate(value: f64) -> Result<FixedPointNumber<i16, i16>, JsError> {
    let scaled = value * 65536.0;
    if !scaled.is_finite()
        || scaled < f64::from(i16::MIN) * 65536.0
        || scaled > f64::from(i16::MAX) * 65536.0 + 65535.0
    {
        return Err(JsError::new("media_rate is out of range"));
    }
    let scaled = scaled.round() as i64;
    let integer = (scaled >> 16) as i16;
    let fraction = (scaled & 0xFFFF) as i16;
    Ok(FixedPointNumber::new(integer, fraction))
}

/// `nojson` のエラーを JavaScript の例外に変換する
fn js_error(error: nojson::JsonParseError) -> JsError {
    JsError::new(&format!("failed to parse options JSON: {error}"))
}
