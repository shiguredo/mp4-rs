//! fMP4 マルチプレックス処理の WASM バインディング
//!
//! # トラック設定 JSON フォーマット (`mp4_fmp4_muxer_from_json`)
//!
//! ```json
//! [
//!   {
//!     "track_kind": "video",
//!     "timescale": 90000,
//!     "sample_entry": { ... }
//!   },
//!   {
//!     "track_kind": "audio",
//!     "timescale": 48000,
//!     "sample_entry": { ... }
//!   }
//! ]
//! ```
//!
//! # サンプルメタデータ JSON フォーマット (`mp4_fmp4_muxer_write_media_segment_json`)
//!
//! ```json
//! [
//!   {
//!     "track_index": 0,
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
use c_api::fmp4_mux::Mp4Fmp4Muxer;

/// JSON 形式のトラック設定から `Mp4Fmp4Muxer` インスタンスを生成する
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
/// 返されたポインタは `mp4_fmp4_muxer_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_from_json(
    json_bytes: *const u8,
    json_bytes_len: u32,
) -> *mut Mp4Fmp4Muxer {
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

    let Ok(entries) = parse_json_track_configs(raw_json.value()) else {
        return std::ptr::null_mut();
    };

    let track_configs_c: Vec<c_api::fmp4_mux::Mp4Fmp4TrackConfig> = entries
        .iter()
        .map(
            |(kind, timescale, entry_ptr)| c_api::fmp4_mux::Mp4Fmp4TrackConfig {
                track_kind: *kind,
                timescale: *timescale,
                sample_entry: *entry_ptr as *const _,
            },
        )
        .collect();

    let result = unsafe {
        c_api::fmp4_mux::mp4_fmp4_muxer_new(track_configs_c.as_ptr(), track_configs_c.len() as u32)
    };

    // sample_entry の Box を解放する（muxer が内部でコピー済みのため）
    for (_, _, entry_ptr) in entries {
        let _ = unsafe { Box::from_raw(entry_ptr) };
    }

    result
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
/// 成功時は `Vec<u8>` へのポインタ、エラー時は NULL
///
/// 返されたポインタは `mp4_vec_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_write_media_segment_json(
    muxer: *mut Mp4Fmp4Muxer,
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
/// `mp4_fmp4_muxer_write_media_segment_json()` と同じ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_write_media_segment_with_sidx_json(
    muxer: *mut Mp4Fmp4Muxer,
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
    muxer: *mut Mp4Fmp4Muxer,
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
    let mut c_samples: Vec<c_api::fmp4_mux::Mp4Fmp4Sample> = Vec::new();
    let mut data_offset = 0usize;
    for meta in &sample_metas {
        let end = data_offset + meta.data_size;
        if end > sample_data_slice.len() {
            return std::ptr::null_mut();
        }
        c_samples.push(c_api::fmp4_mux::Mp4Fmp4Sample {
            track_index: meta.track_index,
            duration: meta.duration,
            keyframe: meta.keyframe,
            has_composition_time_offset: meta.composition_time_offset.is_some(),
            composition_time_offset: meta.composition_time_offset.unwrap_or(0),
            data: sample_data_slice[data_offset..].as_ptr(),
            data_size: u32::try_from(meta.data_size)
                .expect("data_size exceeds u32::MAX; validated by parse_json_sample_metas"),
        });
        data_offset = end;
    }

    let mut out_data: *mut u8 = std::ptr::null_mut();
    let mut out_size: u32 = 0;

    let result = if with_sidx {
        unsafe {
            c_api::fmp4_mux::mp4_fmp4_muxer_write_media_segment_with_sidx(
                muxer,
                c_samples.as_ptr(),
                c_samples.len() as u32,
                &mut out_data,
                &mut out_size,
            )
        }
    } else {
        unsafe {
            c_api::fmp4_mux::mp4_fmp4_muxer_write_media_segment(
                muxer,
                c_samples.as_ptr(),
                c_samples.len() as u32,
                &mut out_data,
                &mut out_size,
            )
        }
    };

    if !matches!(result, c_api::error::Mp4Error::MP4_ERROR_OK) || out_data.is_null() {
        return std::ptr::null_mut();
    }

    let bytes = unsafe { Vec::from_raw_parts(out_data, out_size as usize, out_size as usize) };
    Box::into_raw(Box::new(bytes))
}

struct SampleMeta {
    track_index: u32,
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
            let track_index: u32 = item.to_member("track_index")?.required()?.try_into()?;
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
                track_index,
                duration,
                keyframe,
                composition_time_offset,
                data_size,
            })
        })
        .collect()
}

fn parse_json_track_configs(
    value: nojson::RawJsonValue<'_, '_>,
) -> Result<
    Vec<(
        c_api::basic_types::Mp4TrackKind,
        u32,
        *mut c_api::boxes::Mp4SampleEntry,
    )>,
    nojson::JsonParseError,
> {
    // Box::into_raw を行う前に全エントリの解析を完了させる。
    // エラー時に生 ポインタが漏洩しないよう、成功後のみ raw ポインタに変換する。
    let parsed: Vec<(
        c_api::basic_types::Mp4TrackKind,
        u32,
        c_api::boxes::Mp4SampleEntry,
    )> = value
        .to_array()?
        .map(|item| {
            let kind_str = item
                .to_member("track_kind")?
                .required()?
                .to_unquoted_string_str()?;
            let kind = match kind_str.as_ref() {
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
            let entry_value = item.to_member("sample_entry")?.required()?;
            let entry = crate::boxes::parse_json_mp4_sample_entry(entry_value)?;
            Ok((kind, timescale, entry))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(parsed
        .into_iter()
        .map(|(kind, timescale, entry)| (kind, timescale, Box::into_raw(Box::new(entry))))
        .collect())
}
