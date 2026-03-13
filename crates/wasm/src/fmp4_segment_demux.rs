//! fMP4 デマルチプレックス処理の WASM バインディング
//!
//! `fmp4_segment_demuxer_new` / `fmp4_segment_demuxer_handle_init_segment` / `fmp4_segment_demuxer_free`
//! などの基本関数は C API クレートからそのまま公開されるため、
//! このモジュールでは WASM 固有の JSON 変換関数のみを定義する。
use c_api::demux::{Mp4DemuxSample, Mp4DemuxTrackInfo};
use c_api::fmp4_segment_demux::Fmp4SegmentDemuxer;

use crate::boxes::fmt_json_mp4_sample_entry;

/// トラック情報を JSON 形式で返す
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
///
/// # 戻り値
///
/// JSON 文字列を含む `Vec<u8>` へのポインタ（エラー時は NULL）
///
/// 返されたポインタは `mp4_vec_free()` で解放する必要がある
///
/// # JSON フォーマット
///
/// ```json
/// [{ "track_id": 1, "kind": "video", "duration": 0, "timescale": 90000 }]
/// ```
///
/// `duration` は fMP4 では init segment 由来の値であり、0 になることが多い。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_demuxer_get_tracks_json(
    demuxer: *mut Fmp4SegmentDemuxer,
) -> *mut Vec<u8> {
    if demuxer.is_null() {
        return std::ptr::null_mut();
    }

    let mut tracks_ptr: *const Mp4DemuxTrackInfo = std::ptr::null();
    let mut count: u32 = 0;

    let result = unsafe {
        c_api::fmp4_segment_demux::fmp4_segment_demuxer_get_tracks(
            demuxer,
            &mut tracks_ptr,
            &mut count,
        )
    };

    if !matches!(result, c_api::error::Mp4Error::MP4_ERROR_OK) || tracks_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let tracks = unsafe { std::slice::from_raw_parts(tracks_ptr, count as usize) };
    let json = nojson::json(|f| {
        f.array(|f| {
            for t in tracks {
                f.element(nojson::json(|f| fmt_json_track_info(f, t)))?;
            }
            Ok(())
        })
    })
    .to_string();

    Box::into_raw(Box::new(json.into_bytes()))
}

/// メディアセグメントを処理してサンプル情報を JSON 形式で返す
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
/// - `data`: メディアセグメントデータへのポインタ
/// - `size`: データのサイズ（バイト単位）
///
/// # 戻り値
///
/// JSON 文字列を含む `Vec<u8>` へのポインタ（エラー時は NULL）
///
/// 返されたポインタは `mp4_vec_free()` で解放する必要がある
///
/// # JSON フォーマット
///
/// ```json
/// [
///   {
///     "track_id": 1,
///     "timestamp": 0,
///     "duration": 3000,
///     "keyframe": true,
///     "data_offset": 1234,
///     "data_size": 1024
///   }
/// ]
/// ```
///
/// `data_offset` は `data` 引数の先頭からのバイトオフセット
/// `composition_time_offset` は共通 API として `i64` で出力される。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_demuxer_handle_media_segment_json(
    demuxer: *mut Fmp4SegmentDemuxer,
    data: *const u8,
    size: u32,
) -> *mut Vec<u8> {
    if demuxer.is_null() || data.is_null() {
        return std::ptr::null_mut();
    }

    let mut out_samples: *mut Mp4DemuxSample = std::ptr::null_mut();
    let mut out_count: u32 = 0;

    let result = unsafe {
        c_api::fmp4_segment_demux::fmp4_segment_demuxer_handle_media_segment(
            demuxer,
            data,
            size,
            &mut out_samples,
            &mut out_count,
        )
    };

    if !matches!(result, c_api::error::Mp4Error::MP4_ERROR_OK) {
        return std::ptr::null_mut();
    }

    let samples = if out_samples.is_null() || out_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(out_samples, out_count as usize) }
    };

    let json = nojson::json(|f| {
        f.array(|f| {
            for s in samples {
                f.element(nojson::json(|f| fmt_json_demux_sample(f, s)))?;
            }
            Ok(())
        })
    })
    .to_string();

    if !out_samples.is_null() {
        unsafe {
            c_api::fmp4_segment_demux::fmp4_segment_demuxer_free_samples(out_samples, out_count)
        };
    }

    Box::into_raw(Box::new(json.into_bytes()))
}

fn fmt_json_track_info(
    f: &mut nojson::JsonFormatter<'_, '_>,
    track: &Mp4DemuxTrackInfo,
) -> std::fmt::Result {
    let kind_str = match track.kind {
        c_api::basic_types::Mp4TrackKind::MP4_TRACK_KIND_AUDIO => "audio",
        c_api::basic_types::Mp4TrackKind::MP4_TRACK_KIND_VIDEO => "video",
    };
    f.object(|f| {
        f.member("track_id", track.track_id)?;
        f.member("kind", nojson::json(|f| f.string(kind_str)))?;
        f.member("duration", track.duration)?;
        f.member("timescale", track.timescale)
    })
}

fn fmt_json_demux_sample(
    f: &mut nojson::JsonFormatter<'_, '_>,
    sample: &Mp4DemuxSample,
) -> std::fmt::Result {
    f.object(|f| {
        if !sample.sample_entry.is_null() {
            let sample_entry = unsafe { &*sample.sample_entry };
            f.member(
                "sample_entry",
                nojson::json(|f| fmt_json_mp4_sample_entry(f, sample_entry)),
            )?;
        }
        let track = unsafe { &*sample.track };
        f.member("track_id", track.track_id)?;
        f.member("timestamp", sample.timestamp)?;
        f.member("duration", sample.duration)?;
        f.member("keyframe", sample.keyframe)?;
        let cto: Option<i64> = if sample.has_composition_time_offset {
            Some(sample.composition_time_offset)
        } else {
            None
        };
        f.member("composition_time_offset", cto)?;
        f.member("data_offset", sample.data_offset)?;
        f.member("data_size", sample.data_size)
    })
}
