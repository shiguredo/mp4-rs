//! fMP4 マルチプレックス処理の WASM バインディング
//!
//! # オプション JSON フォーマット (`fmp4_segment_muxer_from_json`)
//!
//! ```json
//! {
//!   "creation_timestamp_secs": 0
//! }
//! ```
//!
//! # サンプルメタデータ JSON フォーマット (`fmp4_segment_muxer_write_media_segment_json`)
//!
//! ```json
//! [
//!   {
//!     "track_kind": "video",
//!     "timescale": 90000,
//!     "sample_entry": { ... },
//!     "duration": 3000,
//!     "keyframe": true,
//!     "composition_time_offset": null,
//!     "data_size": 1024
//!   }
//! ]
//! ```
//!
//! `data_size` の合計が `sample_data` バイト列の長さと一致する必要がある。
//! サンプルデータはサンプルの出現順に連結されていること。
use c_api::fmp4_segment_mux::Fmp4SegmentMuxer;

/// JSON 形式のトラック設定から `Fmp4SegmentMuxer` インスタンスを生成する
///
/// # 引数
///
/// - `json_bytes`: JSON データバイト列へのポインタ
/// - `json_bytes_len`: JSON データのバイト長
///
/// # 戻り値
///
/// 成功時はインスタンスへのポインタ、エラー時は NULL
///
/// 返されたポインタは `fmp4_segment_muxer_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_from_json(
    json_bytes: *const u8,
    json_bytes_len: u32,
) -> *mut Fmp4SegmentMuxer {
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

    let Ok(options) = parse_json_muxer_options(raw_json.value()) else {
        return std::ptr::null_mut();
    };

    unsafe { c_api::fmp4_segment_mux::fmp4_segment_muxer_new_with_options(&options) }
}

/// JSON メタデータとサンプルバイナリデータからメディアセグメントを生成して `Vec<u8>` として返す
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `meta_json_bytes`: サンプルメタデータ JSON バイト列へのポインタ
/// - `meta_json_len`: JSON のバイト長
/// - `sample_data`: サンプルのバイナリデータへのポインタ（サンプル順に連結）
/// - `sample_data_len`: バイナリデータの合計サイズ
///
/// # 戻り値
///
/// 成功時は `moof + mdat header + payload` 全体を格納した `Vec<u8>` へのポインタ、エラー時は NULL
///
/// 返されたポインタは `mp4_vec_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_media_segment_json(
    muxer: *mut Fmp4SegmentMuxer,
    meta_json_bytes: *const u8,
    meta_json_len: u32,
    sample_data: *const u8,
    sample_data_len: u32,
) -> *mut Vec<u8> {
    write_segment_impl(
        muxer,
        meta_json_bytes,
        meta_json_len,
        sample_data,
        sample_data_len,
        false,
    )
}

/// JSON メタデータとサンプルバイナリデータから `sidx` 付きメディアセグメントを生成して `Vec<u8>` として返す
///
/// # 引数
///
/// `fmp4_segment_muxer_write_media_segment_json()` と同じ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_media_segment_with_sidx_json(
    muxer: *mut Fmp4SegmentMuxer,
    meta_json_bytes: *const u8,
    meta_json_len: u32,
    sample_data: *const u8,
    sample_data_len: u32,
) -> *mut Vec<u8> {
    write_segment_impl(
        muxer,
        meta_json_bytes,
        meta_json_len,
        sample_data,
        sample_data_len,
        true,
    )
}

fn write_segment_impl(
    muxer: *mut Fmp4SegmentMuxer,
    meta_json_bytes: *const u8,
    meta_json_len: u32,
    sample_data: *const u8,
    sample_data_len: u32,
    with_sidx: bool,
) -> *mut Vec<u8> {
    if muxer.is_null() || meta_json_bytes.is_null() {
        return std::ptr::null_mut();
    }

    let Ok(json_text) = std::str::from_utf8(unsafe {
        std::slice::from_raw_parts(meta_json_bytes, meta_json_len as usize)
    }) else {
        return std::ptr::null_mut();
    };

    let Ok(raw_json) = nojson::RawJson::parse(json_text) else {
        return std::ptr::null_mut();
    };

    let sample_data_slice: &[u8] = if sample_data.is_null() || sample_data_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(sample_data, sample_data_len as usize) }
    };

    let Ok(sample_metas) = parse_json_sample_metas(raw_json.value()) else {
        return std::ptr::null_mut();
    };

    // 各サンプルのデータ範囲を計算する
    let mut sample_entry_boxes: Vec<Option<Box<c_api::boxes::Mp4SampleEntry>>> = Vec::new();
    let mut c_samples: Vec<c_api::fmp4_segment_mux::Fmp4SegmentSample> = Vec::new();
    let mut data_offset = 0usize;
    for meta in sample_metas {
        let Some(end) = data_offset.checked_add(meta.data_size) else {
            return std::ptr::null_mut();
        };
        if end > sample_data_slice.len() {
            return std::ptr::null_mut();
        }
        sample_entry_boxes.push(meta.sample_entry.map(Box::new));
        let sample_entry_ptr = sample_entry_boxes
            .last()
            .and_then(|entry| entry.as_ref())
            .map_or(std::ptr::null(), |entry| (&**entry) as *const _);
        c_samples.push(c_api::fmp4_segment_mux::Fmp4SegmentSample {
            track_kind: meta.track_kind,
            timescale: meta.timescale,
            sample_entry: sample_entry_ptr,
            duration: meta.duration,
            keyframe: meta.keyframe,
            has_composition_time_offset: meta.composition_time_offset.is_some(),
            composition_time_offset: meta.composition_time_offset.unwrap_or(0),
            data_offset: u64::try_from(data_offset).expect("data_offset exceeds u64::MAX"),
            data_size: u32::try_from(meta.data_size)
                .expect("data_size exceeds u32::MAX; validated by parse_json_sample_metas"),
        });
        data_offset = end;
    }

    let mut out_data: *mut u8 = std::ptr::null_mut();
    let mut out_size: u32 = 0;

    let result = if with_sidx {
        unsafe {
            c_api::fmp4_segment_mux::fmp4_segment_muxer_write_media_segment_with_sidx(
                muxer,
                c_samples.as_ptr(),
                u32::try_from(c_samples.len()).expect("sample count exceeds u32::MAX"),
                &mut out_data,
                &mut out_size,
            )
        }
    } else {
        unsafe {
            c_api::fmp4_segment_mux::fmp4_segment_muxer_write_media_segment(
                muxer,
                c_samples.as_ptr(),
                u32::try_from(c_samples.len()).expect("sample count exceeds u32::MAX"),
                &mut out_data,
                &mut out_size,
            )
        }
    };

    if !matches!(result, c_api::error::Mp4Error::MP4_ERROR_OK) || out_data.is_null() {
        return std::ptr::null_mut();
    }

    let mut bytes = unsafe { Vec::from_raw_parts(out_data, out_size as usize, out_size as usize) };
    bytes.extend_from_slice(sample_data_slice);
    Box::into_raw(Box::new(bytes))
}

struct SampleMeta {
    track_kind: c_api::basic_types::Mp4TrackKind,
    timescale: u32,
    sample_entry: Option<c_api::boxes::Mp4SampleEntry>,
    duration: u32,
    keyframe: bool,
    composition_time_offset: Option<i32>,
    data_size: usize,
}

fn parse_json_sample_metas(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<Vec<SampleMeta>, nojson::JsonParseError> {
    value
        .to_array()?
        .map(|item| {
            let track_kind_str = item
                .to_member("track_kind")?
                .required()?
                .to_unquoted_string_str()?;
            let track_kind = match track_kind_str.as_ref() {
                "audio" => c_api::basic_types::Mp4TrackKind::MP4_TRACK_KIND_AUDIO,
                "video" => c_api::basic_types::Mp4TrackKind::MP4_TRACK_KIND_VIDEO,
                _ => {
                    return Err(item
                        .to_member("track_kind")?
                        .required()?
                        .invalid("must be \"audio\" or \"video\""));
                }
            };
            let timescale: u32 = item.to_member("timescale")?.required()?.try_into()?;
            let sample_entry = if let Some(value) = item.to_member("sample_entry")?.get() {
                Some(crate::boxes::parse_json_mp4_sample_entry(value)?)
            } else {
                None
            };
            let duration: u32 = item.to_member("duration")?.required()?.try_into()?;
            let keyframe: bool = item.to_member("keyframe")?.required()?.try_into()?;
            let composition_time_offset: Option<i32> =
                if let Some(v) = item.to_member("composition_time_offset")?.get() {
                    Some(v.try_into()?)
                } else {
                    None
                };
            let data_size: u64 = item.to_member("data_size")?.required()?.try_into()?;
            let data_size = usize::try_from(data_size).map_err(|_| {
                item.to_member("data_size")
                    .and_then(|m| m.required())
                    .map(|v| v.invalid("data_size exceeds usize::MAX"))
                    .unwrap_or_else(|e| e)
            })?;
            Ok(SampleMeta {
                track_kind,
                timescale,
                sample_entry,
                duration,
                keyframe,
                composition_time_offset,
                data_size,
            })
        })
        .collect()
}

fn parse_json_muxer_options(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<c_api::fmp4_segment_mux::Fmp4SegmentMuxerOptions, nojson::JsonParseError> {
    let creation_timestamp_secs =
        if let Some(value) = value.to_member("creation_timestamp_secs")?.get() {
            let creation_timestamp_secs: u64 = value.try_into()?;
            creation_timestamp_secs
        } else {
            0
        };
    Ok(c_api::fmp4_segment_mux::Fmp4SegmentMuxerOptions {
        creation_timestamp_secs,
    })
}
