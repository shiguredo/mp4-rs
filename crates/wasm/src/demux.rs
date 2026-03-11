//! C API の demux.rs に対応するモジュール
use c_api::basic_types::Mp4TrackKind;
use c_api::demux::{Mp4DemuxSample, Mp4DemuxTrackInfo};

use crate::boxes::fmt_json_mp4_sample_entry;

/// トラック情報を JSON 文字列に変換する
//
/// # 引数
///
/// - `track_info`: 変換対象の Mp4DemuxTrackInfo へのポインタ
///
/// # 戻り値
///
/// JSON 文字列を含む `Vec<u8>` へのポインタ。エラー時は NULL
///
/// 呼び出しもとは不要になったら `mp4_vec_free` 関数を使ってこの `Vec` を解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_demux_track_info_to_json(
    track_info: *const Mp4DemuxTrackInfo,
) -> *mut Vec<u8> {
    if track_info.is_null() {
        return std::ptr::null_mut();
    }

    let track_info = unsafe { &*track_info };
    let json = nojson::json(|f| fmt_json_mp4_demux_track_info(f, track_info)).to_string();
    Box::into_raw(Box::new(json.into_bytes()))
}

/// サンプルを JSON 文字列に変換する
///
/// # 引数
///
/// - `sample`: 変換対象の `Mp4DemuxSample` へのポインタ
///
/// # 戻り値
///
/// JSON 文字列を含む `Vec<u8>` へのポインタ。エラー時は NULL
///
/// 呼び出しもとは不要になったら `mp4_vec_free` 関数を使ってこの `Vec` を解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_demux_sample_to_json(sample: *const Mp4DemuxSample) -> *mut Vec<u8> {
    if sample.is_null() {
        return std::ptr::null_mut();
    }

    let sample = unsafe { &*sample };
    let json = nojson::json(|f| fmt_json_mp4_demux_sample(f, sample)).to_string();
    Box::into_raw(Box::new(json.into_bytes()))
}

fn fmt_json_mp4_demux_sample(
    f: &mut nojson::JsonFormatter<'_, '_>,
    sample: &Mp4DemuxSample,
) -> std::fmt::Result {
    f.object(|f| {
        // トラック情報
        if !sample.track.is_null() {
            let track = unsafe { &*sample.track };

            // トラック情報全体を毎回 JSON に変換するのは無駄なので、
            // wasm 版ではサンプルには ID だけを持たせるようにする
            // （ID と実際の情報とのマッピングを行うのは利用側の責務）
            f.member("track_id", track.track_id)?;
        }

        // サンプルエントリー
        if !sample.sample_entry.is_null() {
            let sample_entry = unsafe { &*sample.sample_entry };
            f.member(
                "sample_entry",
                nojson::json(|f| fmt_json_mp4_sample_entry(f, sample_entry)),
            )?;
        }

        // キーフレームフラグ
        f.member("keyframe", sample.keyframe)?;

        // タイムスタンプと尺
        f.member("timestamp", sample.timestamp)?;
        f.member("duration", sample.duration)?;
        let cto: Option<i64> = if sample.has_composition_time_offset {
            Some(sample.composition_time_offset)
        } else {
            None
        };
        f.member("composition_time_offset", cto)?;

        // データの位置とサイズ
        f.member("data_offset", sample.data_offset)?;
        f.member("data_size", sample.data_size)?;

        Ok(())
    })
}

fn fmt_json_mp4_demux_track_info(
    f: &mut nojson::JsonFormatter<'_, '_>,
    track_info: &Mp4DemuxTrackInfo,
) -> std::fmt::Result {
    let kind = match track_info.kind {
        Mp4TrackKind::MP4_TRACK_KIND_AUDIO => "audio",
        Mp4TrackKind::MP4_TRACK_KIND_VIDEO => "video",
    };

    f.object(|f| {
        f.member("track_id", track_info.track_id)?;
        f.member("kind", kind)?;
        f.member("duration", track_info.duration)?;
        f.member("timescale", track_info.timescale)
    })
}
