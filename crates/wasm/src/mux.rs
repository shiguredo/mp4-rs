//! C API の mux.rs に対応するモジュール
use c_api::basic_types::Mp4TrackKind;
use c_api::boxes::Mp4SampleEntry;
use c_api::mux::Mp4MuxSample;

/// JSON データを `Mp4MuxSample` 構造体に変換する
///
/// この関数は JSON 文字列を生のバイト列として受け取り、`Mp4MuxSample` オブジェクトに変換する。
/// 返されたポインタは `mp4_mux_sample_free()` を使用して解放する必要があります。
///
/// # 引数
///
/// - `json_bytes`: JSON データバイト列へのポインタ
/// - `json_bytes_len`: JSON データのバイト長
///
/// # 戻り値
///
/// 成功時は割り当てられた `Mp4MuxSample` へのポインタ、エラー時は null ポインタを返す。
/// 呼び出し側は `mp4_mux_sample_free()` でこのメモリを解放する必要がある。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_mux_sample_from_json(
    json_bytes: *const u8,
    json_bytes_len: u32,
) -> *mut Mp4MuxSample {
    if json_bytes.is_null() {
        return std::ptr::null_mut();
    }

    let Ok(json_text) = std::str::from_utf8(unsafe {
        std::slice::from_raw_parts(json_bytes, json_bytes_len as usize)
    }) else {
        return std::ptr::null_mut();
    };

    let Ok(raw_json) = nojson::RawJson::parse(json_text) else {
        return std::ptr::null_mut();
    };

    let value = raw_json.value();
    let Ok(sample) = parse_json_mp4_mux_sample(value) else {
        return std::ptr::null_mut();
    };

    Box::into_raw(Box::new(sample))
}

/// `Mp4MuxSample` オブジェクトとその関連リソースを解放する。
///
/// この関数は `mp4_mux_sample_from_json()` で以前に作成された `Mp4MuxSample` のメモリを解放する。
///
/// # 引数
///
/// - `sample`: 解放する `Mp4MuxSample` へのポインタ（null の場合には何もしない）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_mux_sample_free(sample: *mut Mp4MuxSample) {
    if sample.is_null() {
        return;
    }

    let sample_mut = unsafe { &mut *sample };
    if !sample_mut.sample_entry.is_null() {
        unsafe {
            crate::boxes::mp4_sample_entry_free(sample_mut.sample_entry as *mut Mp4SampleEntry)
        };
    }
    let _ = unsafe { Box::from_raw(sample) };
}

fn parse_json_mp4_mux_sample(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Mp4MuxSample, nojson::JsonParseError> {
    let track_kind_value = value.to_member("track_kind")?.required()?;
    let track_kind = match track_kind_value.to_unquoted_string_str()?.as_ref() {
        "audio" => Mp4TrackKind::MP4_TRACK_KIND_AUDIO,
        "video" => Mp4TrackKind::MP4_TRACK_KIND_VIDEO,
        _ => return Err(track_kind_value.invalid("must be \"audio\" or \"video\"")),
    };

    let keyframe: bool = value.to_member("keyframe")?.required()?.try_into()?;
    let timescale: u32 = value.to_member("timescale")?.required()?.try_into()?;
    let duration: u32 = value.to_member("duration")?.required()?.try_into()?;
    let composition_time_offset: Option<i32> =
        if let Some(v) = value.to_member("composition_time_offset")?.get() {
            Some(v.try_into()?)
        } else {
            None
        };
    let data_offset: u64 = value.to_member("data_offset")?.required()?.try_into()?;
    let data_size: u32 = value.to_member("data_size")?.required()?.try_into()?;

    let sample_entry: *const Mp4SampleEntry =
        if let Some(sample_entry_value) = value.to_member("sample_entry")?.get() {
            let sample_entry = crate::boxes::parse_json_mp4_sample_entry(sample_entry_value)?;
            Box::into_raw(Box::new(sample_entry)) as *const Mp4SampleEntry
        } else {
            std::ptr::null()
        };

    Ok(Mp4MuxSample {
        track_kind,
        sample_entry,
        keyframe,
        timescale,
        duration,
        has_composition_time_offset: composition_time_offset.is_some(),
        composition_time_offset: composition_time_offset.unwrap_or(0),
        data_offset,
        data_size,
    })
}
