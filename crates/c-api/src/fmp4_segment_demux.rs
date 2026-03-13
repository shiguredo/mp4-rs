//! fMP4 デマルチプレックス処理の C API を定義するモジュール
use std::ffi::{CString, c_char};

use shiguredo_mp4::BaseBox;
use shiguredo_mp4::demux::Fmp4SegmentDemuxer as RustFmp4SegmentDemuxer;

use crate::{
    boxes::{Mp4SampleEntry, Mp4SampleEntryOwned},
    demux::{Mp4DemuxSample, Mp4DemuxTrackInfo},
    error::Mp4Error,
};

/// fMP4 Demuxer の状態を保持する C 構造体
///
/// # 関連関数
///
/// - `fmp4_segment_demuxer_new()`: インスタンスを生成する
/// - `fmp4_segment_demuxer_free()`: リソースを解放する
/// - `fmp4_segment_demuxer_get_last_error()`: 最後のエラーメッセージを取得する
/// - `fmp4_segment_demuxer_handle_init_segment()`: 初期化セグメントを処理する
/// - `fmp4_segment_demuxer_get_tracks()`: トラック情報を取得する
/// - `fmp4_segment_demuxer_handle_media_segment()`: メディアセグメントを処理する
/// - `fmp4_segment_demuxer_free_samples()`: サンプル配列を解放する
pub struct Fmp4SegmentDemuxer {
    inner: RustFmp4SegmentDemuxer,
    /// キャッシュ済みのトラック情報。`None` は未初期化または未取得を表す。
    tracks_cache: Option<Vec<Mp4DemuxTrackInfo>>,
    sample_entries: Vec<(
        shiguredo_mp4::boxes::SampleEntry,
        Mp4SampleEntryOwned,
        Box<Mp4SampleEntry>,
    )>,
    last_error_string: Option<CString>,
}

impl Fmp4SegmentDemuxer {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }

    fn ensure_tracks_cache(&mut self) -> Result<&[Mp4DemuxTrackInfo], Mp4Error> {
        if self.tracks_cache.is_none() {
            match self.inner.tracks() {
                Ok(tracks) => {
                    self.tracks_cache = Some(tracks.iter().cloned().map(Into::into).collect());
                }
                Err(e) => {
                    self.set_last_error(&format!("[fmp4_segment_demuxer_get_tracks] {e}"));
                    return Err(e.into());
                }
            }
        }

        Ok(self
            .tracks_cache
            .as_deref()
            .expect("tracks_cache should be initialized above"))
    }
}

/// 新しい `Fmp4SegmentDemuxer` インスタンスを生成する
///
/// # 戻り値
///
/// インスタンスへのポインタ（返されたポインタは `fmp4_segment_demuxer_free()` で解放する）
#[unsafe(no_mangle)]
pub extern "C" fn fmp4_segment_demuxer_new() -> *mut Fmp4SegmentDemuxer {
    Box::into_raw(Box::new(Fmp4SegmentDemuxer {
        inner: RustFmp4SegmentDemuxer::new(),
        tracks_cache: None,
        sample_entries: Vec::new(),
        last_error_string: None,
    }))
}

/// `Fmp4SegmentDemuxer` インスタンスを破棄してリソースを解放する
///
/// # 引数
///
/// - `demuxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_demuxer_free(demuxer: *mut Fmp4SegmentDemuxer) {
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
pub unsafe extern "C" fn fmp4_segment_demuxer_get_last_error(
    demuxer: *const Fmp4SegmentDemuxer,
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
pub unsafe extern "C" fn fmp4_segment_demuxer_handle_init_segment(
    demuxer: *mut Fmp4SegmentDemuxer,
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
            demuxer.set_last_error(&format!("[fmp4_segment_demuxer_handle_init_segment] {e}"));
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
pub unsafe extern "C" fn fmp4_segment_demuxer_get_tracks(
    demuxer: *mut Fmp4SegmentDemuxer,
    out_tracks: *mut *const Mp4DemuxTrackInfo,
    out_count: *mut u32,
) -> Mp4Error {
    if demuxer.is_null() || out_tracks.is_null() || out_count.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };

    let cached = match demuxer.ensure_tracks_cache() {
        Ok(cached) => cached,
        Err(error) => {
            unsafe {
                *out_tracks = std::ptr::null();
                *out_count = 0;
            }
            return error;
        }
    };
    let count = match u32::try_from(cached.len()) {
        Ok(v) => v,
        Err(_) => {
            unsafe {
                *out_tracks = std::ptr::null();
                *out_count = 0;
            }
            demuxer
                .set_last_error("[fmp4_segment_demuxer_get_tracks] track count exceeds u32::MAX");
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
///   - 返された配列は `fmp4_segment_demuxer_free_samples()` で解放する必要がある
/// - `out_count`: サンプル数を受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に処理された
/// - `MP4_ERROR_INVALID_STATE`: 未初期化
/// - その他のエラー: 処理に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_demuxer_handle_media_segment(
    demuxer: *mut Fmp4SegmentDemuxer,
    data: *const u8,
    size: u32,
    out_samples: *mut *mut Mp4DemuxSample,
    out_count: *mut u32,
) -> Mp4Error {
    if demuxer.is_null() || data.is_null() || out_samples.is_null() || out_count.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let demuxer = unsafe { &mut *demuxer };
    let data = unsafe { std::slice::from_raw_parts(data, size as usize) };

    if let Err(error) = demuxer.ensure_tracks_cache() {
        unsafe {
            *out_samples = std::ptr::null_mut();
            *out_count = 0;
        }
        return error;
    }

    match demuxer.inner.handle_media_segment(data) {
        Ok(samples) => {
            let mut c_samples: Vec<Mp4DemuxSample> = Vec::new();
            for s in samples {
                let sample_entry = if let Some(sample_entry) = s.sample_entry {
                    let sample_entry_box_type = sample_entry.box_type();
                    if let Some(entry) = demuxer
                        .sample_entries
                        .iter()
                        .find_map(|entry| (entry.0 == *sample_entry).then_some(&entry.2))
                    {
                        Some(&**entry)
                    } else {
                        let Some(entry_owned) = Mp4SampleEntryOwned::new(sample_entry.clone())
                        else {
                            unsafe {
                                *out_samples = std::ptr::null_mut();
                                *out_count = 0;
                            }
                            demuxer.set_last_error(&format!(
                                "[fmp4_segment_demuxer_handle_media_segment] Unsupported sample entry box type: {sample_entry_box_type}",
                            ));
                            return Mp4Error::MP4_ERROR_UNSUPPORTED;
                        };
                        let entry = Box::new(entry_owned.to_mp4_sample_entry());
                        demuxer
                            .sample_entries
                            .push((sample_entry.clone(), entry_owned, entry));
                        demuxer.sample_entries.last().map(|entry| &*entry.2)
                    }
                } else {
                    None
                };
                let Some(track) = demuxer.tracks_cache.as_ref().and_then(|tracks| {
                    tracks
                        .iter()
                        .find(|track| track.track_id == s.track.track_id)
                }) else {
                    unsafe {
                        *out_samples = std::ptr::null_mut();
                        *out_count = 0;
                    }
                    demuxer.set_last_error(
                        "[fmp4_segment_demuxer_handle_media_segment] track info not found for sample",
                    );
                    return Mp4Error::MP4_ERROR_OTHER;
                };
                c_samples.push(Mp4DemuxSample::new(s, track, sample_entry));
            }

            let count = match u32::try_from(c_samples.len()) {
                Ok(v) => v,
                Err(_) => {
                    unsafe {
                        *out_samples = std::ptr::null_mut();
                        *out_count = 0;
                    }
                    demuxer.set_last_error(
                        "[fmp4_segment_demuxer_handle_media_segment] sample count exceeds u32::MAX",
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
            demuxer.set_last_error(&format!("[fmp4_segment_demuxer_handle_media_segment] {e}"));
            e.into()
        }
    }
}

/// `fmp4_segment_demuxer_handle_media_segment()` で割り当てられたサンプル配列を解放する
///
/// # 引数
///
/// - `samples`: 解放するサンプル配列へのポインタ（NULL の場合は何もしない）
/// - `count`: サンプル数
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_demuxer_free_samples(
    samples: *mut Mp4DemuxSample,
    count: u32,
) {
    if samples.is_null() {
        return;
    }
    let samples = unsafe { std::slice::from_raw_parts_mut(samples, count as usize) };
    let _ = unsafe { Box::from_raw(samples) };
}
