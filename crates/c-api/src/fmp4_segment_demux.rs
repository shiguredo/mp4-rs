//! fMP4 デマルチプレックス処理の C API を定義するモジュール
use std::ffi::{CString, c_char};

use shiguredo_mp4::demux::Fmp4SegmentDemuxer;

use crate::{basic_types::Mp4TrackKind, error::Mp4Error};

/// fMP4 のトラック情報を表す C 構造体
#[repr(C)]
pub struct Mp4Fmp4SegmentTrackInfo {
    /// トラック ID
    pub track_id: u32,

    /// トラックの種別
    pub kind: Mp4TrackKind,

    /// タイムスケール
    pub timescale: u32,
}

/// fMP4 メディアセグメントから取り出されたサンプルを表す C 構造体
#[repr(C)]
pub struct Mp4Fmp4SegmentDemuxSample {
    /// サンプルが属するトラックの ID
    pub track_id: u32,

    /// サンプルのタイムスタンプ（タイムスケール単位）
    ///
    /// この値は decode timestamp を表す。
    pub timestamp: u64,

    /// サンプルの尺（タイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// コンポジション時間オフセットが存在するかどうか
    pub has_composition_time_offset: bool,

    /// コンポジション時間オフセット（タイムスケール単位）
    ///
    /// `has_composition_time_offset` が true の場合のみ有効。
    /// PTS = timestamp + composition_time_offset で計算できる。
    pub composition_time_offset: i32,

    /// セグメントデータ内のサンプルデータ開始位置（バイト単位）
    ///
    /// `mp4_fmp4_segment_demuxer_handle_media_segment()` に渡したデータの先頭からのオフセット
    pub data_offset: u64,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: u32,
}

/// fMP4 Demuxer の状態を保持する C 構造体
///
/// # 関連関数
///
/// - `mp4_fmp4_segment_demuxer_new()`: インスタンスを生成する
/// - `mp4_fmp4_segment_demuxer_free()`: リソースを解放する
/// - `mp4_fmp4_segment_demuxer_get_last_error()`: 最後のエラーメッセージを取得する
/// - `mp4_fmp4_segment_demuxer_handle_init_segment()`: 初期化セグメントを処理する
/// - `mp4_fmp4_segment_demuxer_get_tracks()`: トラック情報を取得する
/// - `mp4_fmp4_segment_demuxer_handle_media_segment()`: メディアセグメントを処理する
/// - `mp4_fmp4_segment_demuxer_free_samples()`: サンプル配列を解放する
pub struct Mp4Fmp4SegmentDemuxer {
    inner: Fmp4SegmentDemuxer,
    /// キャッシュ済みのトラック情報。`None` は未初期化または未取得を表す。
    tracks_cache: Option<Vec<Mp4Fmp4SegmentTrackInfo>>,
    last_error_string: Option<CString>,
}

impl Mp4Fmp4SegmentDemuxer {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }
}

/// 新しい `Mp4Fmp4SegmentDemuxer` インスタンスを生成する
///
/// # 戻り値
///
/// インスタンスへのポインタ（返されたポインタは `mp4_fmp4_segment_demuxer_free()` で解放する）
#[unsafe(no_mangle)]
pub extern "C" fn mp4_fmp4_segment_demuxer_new() -> *mut Mp4Fmp4SegmentDemuxer {
    Box::into_raw(Box::new(Mp4Fmp4SegmentDemuxer {
        inner: Fmp4SegmentDemuxer::new(),
        tracks_cache: None,
        last_error_string: None,
    }))
}

/// `Mp4Fmp4SegmentDemuxer` インスタンスを破棄してリソースを解放する
///
/// # 引数
///
/// - `demuxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_free(demuxer: *mut Mp4Fmp4SegmentDemuxer) {
    if !demuxer.is_null() {
        let _ = unsafe { Box::from_raw(demuxer) };
    }
}

/// 最後に発生したエラーのメッセージを取得する
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
///
/// # 戻り値
///
/// NULL 終端のエラーメッセージへのポインタ（エラーがない場合は空文字列）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_get_last_error(
    demuxer: *const Mp4Fmp4SegmentDemuxer,
) -> *const c_char {
    if demuxer.is_null() {
        return c"".as_ptr();
    }
    let demuxer = unsafe { &*demuxer };
    let Some(e) = &demuxer.last_error_string else {
        return c"".as_ptr();
    };
    e.as_ptr()
}

/// 初期化セグメント（`ftyp` + `moov`）を処理してトラック情報を初期化する
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
/// - `data`: 初期化セグメントデータへのポインタ
/// - `size`: データのサイズ（バイト単位）
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に処理された
/// - `MP4_ERROR_INVALID_STATE`: 既に初期化済み
/// - その他のエラー: 処理に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_handle_init_segment(
    demuxer: *mut Mp4Fmp4SegmentDemuxer,
    data: *const u8,
    size: u32,
) -> Mp4Error {
    if demuxer.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if data.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };
    let data = unsafe { std::slice::from_raw_parts(data, size as usize) };

    match demuxer.inner.handle_init_segment(data) {
        Ok(()) => Mp4Error::MP4_ERROR_OK,
        Err(e) => {
            demuxer.set_last_error(&format!(
                "[mp4_fmp4_segment_demuxer_handle_init_segment] {e}"
            ));
            e.into()
        }
    }
}

/// 初期化済みのトラック情報を取得する
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
/// - `out_tracks`: トラック情報配列へのポインタを受け取るポインタ
///   - このポインタの参照先は `demuxer` インスタンスが有効な間のみアクセス可能
/// - `out_count`: トラック数を受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に取得された
/// - `MP4_ERROR_INVALID_STATE`: 未初期化
/// - その他のエラー: 取得に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_get_tracks(
    demuxer: *mut Mp4Fmp4SegmentDemuxer,
    out_tracks: *mut *const Mp4Fmp4SegmentTrackInfo,
    out_count: *mut u32,
) -> Mp4Error {
    if demuxer.is_null() || out_tracks.is_null() || out_count.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    if demuxer.tracks_cache.is_none() {
        match demuxer.inner.tracks() {
            Ok(tracks) => {
                demuxer.tracks_cache = Some(
                    tracks
                        .iter()
                        .map(|t| Mp4Fmp4SegmentTrackInfo {
                            track_id: t.track_id,
                            kind: t.kind.into(),
                            timescale: t.timescale.get(),
                        })
                        .collect(),
                );
            }
            Err(e) => {
                unsafe {
                    *out_tracks = std::ptr::null();
                    *out_count = 0;
                }
                demuxer.set_last_error(&format!("[mp4_fmp4_segment_demuxer_get_tracks] {e}"));
                return e.into();
            }
        }
    }

    let cached = demuxer
        .tracks_cache
        .as_ref()
        .expect("tracks_cache is initialized above");
    let count = match u32::try_from(cached.len()) {
        Ok(v) => v,
        Err(_) => {
            unsafe {
                *out_tracks = std::ptr::null();
                *out_count = 0;
            }
            demuxer.set_last_error(
                "[mp4_fmp4_segment_demuxer_get_tracks] track count exceeds u32::MAX",
            );
            return Mp4Error::MP4_ERROR_OTHER;
        }
    };
    unsafe {
        *out_tracks = cached.as_ptr();
        *out_count = count;
    }
    Mp4Error::MP4_ERROR_OK
}

/// メディアセグメント（`moof` + `mdat` または `sidx` + `moof` + `mdat`）を処理して
/// サンプルの配列を返す
///
/// # 引数
///
/// - `demuxer`: インスタンスへのポインタ
/// - `data`: メディアセグメントデータへのポインタ
/// - `size`: データのサイズ（バイト単位）
/// - `out_samples`: 生成されたサンプル配列へのポインタを受け取るポインタ
///   - 返された配列は `mp4_fmp4_segment_demuxer_free_samples()` で解放する必要がある
/// - `out_count`: サンプル数を受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に処理された
/// - `MP4_ERROR_INVALID_STATE`: 未初期化
/// - その他のエラー: 処理に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_handle_media_segment(
    demuxer: *mut Mp4Fmp4SegmentDemuxer,
    data: *const u8,
    size: u32,
    out_samples: *mut *mut Mp4Fmp4SegmentDemuxSample,
    out_count: *mut u32,
) -> Mp4Error {
    if demuxer.is_null() || data.is_null() || out_samples.is_null() || out_count.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };
    let data = unsafe { std::slice::from_raw_parts(data, size as usize) };

    match demuxer.inner.handle_media_segment(data) {
        Ok(samples) => {
            let mut c_samples: Vec<Mp4Fmp4SegmentDemuxSample> = Vec::new();
            for s in &samples {
                let data_size = match u32::try_from(s.data_size) {
                    Ok(v) => v,
                    Err(_) => {
                        unsafe {
                            *out_samples = std::ptr::null_mut();
                            *out_count = 0;
                        }
                        demuxer.set_last_error(
                            "[mp4_fmp4_segment_demuxer_handle_media_segment] data_size exceeds u32::MAX",
                        );
                        return Mp4Error::MP4_ERROR_OTHER;
                    }
                };
                c_samples.push(Mp4Fmp4SegmentDemuxSample {
                    track_id: s.track_id,
                    timestamp: s.timestamp,
                    duration: s.duration,
                    keyframe: s.keyframe,
                    has_composition_time_offset: s.composition_time_offset.is_some(),
                    composition_time_offset: s.composition_time_offset.unwrap_or(0),
                    data_offset: s.data_offset as u64, // usize -> u64: 常に安全
                    data_size,
                });
            }

            let count = match u32::try_from(c_samples.len()) {
                Ok(v) => v,
                Err(_) => {
                    unsafe {
                        *out_samples = std::ptr::null_mut();
                        *out_count = 0;
                    }
                    demuxer.set_last_error(
                        "[mp4_fmp4_segment_demuxer_handle_media_segment] sample count exceeds u32::MAX",
                    );
                    return Mp4Error::MP4_ERROR_OTHER;
                }
            };
            let mut boxed = c_samples.into_boxed_slice();
            let ptr = boxed.as_mut_ptr();
            std::mem::forget(boxed);

            unsafe {
                *out_samples = ptr;
                *out_count = count;
            }
            Mp4Error::MP4_ERROR_OK
        }
        Err(e) => {
            unsafe {
                *out_samples = std::ptr::null_mut();
                *out_count = 0;
            }
            demuxer.set_last_error(&format!(
                "[mp4_fmp4_segment_demuxer_handle_media_segment] {e}"
            ));
            e.into()
        }
    }
}

/// `mp4_fmp4_segment_demuxer_handle_media_segment()` で割り当てられたサンプル配列を解放する
///
/// # 引数
///
/// - `samples`: 解放するサンプル配列へのポインタ（NULL の場合は何もしない）
/// - `count`: サンプル数
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_segment_demuxer_free_samples(
    samples: *mut Mp4Fmp4SegmentDemuxSample,
    count: u32,
) {
    if samples.is_null() {
        return;
    }
    let _ = unsafe { Vec::from_raw_parts(samples, count as usize, count as usize) };
}
