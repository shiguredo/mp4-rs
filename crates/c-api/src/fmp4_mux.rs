//! fMP4 マルチプレックス処理の C API を定義するモジュール
use std::ffi::{CString, c_char};
use std::num::NonZeroU32;

use shiguredo_mp4::mux_fmp4::{Fmp4Muxer, Fmp4Sample, Fmp4TrackConfig};

use crate::{basic_types::Mp4TrackKind, boxes::Mp4SampleEntry, error::Mp4Error};

/// fMP4 マルチプレックスのトラック設定を表す C 構造体
#[repr(C)]
pub struct Mp4Fmp4TrackConfig {
    /// トラックの種別
    pub track_kind: Mp4TrackKind,

    /// タイムスケール（0 は無効）
    pub timescale: u32,

    /// サンプルエントリー（コーデック情報）へのポインタ
    ///
    /// NULL を渡すことはできない
    pub sample_entry: *const Mp4SampleEntry,
}

/// fMP4 メディアセグメントに追加するサンプルを表す C 構造体
#[repr(C)]
pub struct Mp4Fmp4Sample {
    /// `mp4_fmp4_muxer_new()` に渡したトラック配列のインデックス
    pub track_index: u32,

    /// サンプルの尺（トラックのタイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// コンポジション時間オフセットが有効かどうか
    pub has_composition_time_offset: bool,

    /// コンポジション時間オフセット（`has_composition_time_offset` が true の場合のみ有効）
    pub composition_time_offset: i32,

    /// サンプルデータへのポインタ
    pub data: *const u8,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: u32,
}

/// fMP4 Muxer の状態を保持する C 構造体
///
/// # 関連関数
///
/// - `mp4_fmp4_muxer_new()`: インスタンスを生成する
/// - `mp4_fmp4_muxer_free()`: リソースを解放する
/// - `mp4_fmp4_muxer_get_last_error()`: 最後のエラーメッセージを取得する
/// - `mp4_fmp4_muxer_write_init_segment()`: 初期化セグメントを生成する
/// - `mp4_fmp4_muxer_write_media_segment()`: メディアセグメントを生成する
/// - `mp4_fmp4_muxer_write_media_segment_with_sidx()`: sidx 付きメディアセグメントを生成する
pub struct Mp4Fmp4Muxer {
    inner: Fmp4Muxer,
    last_error_string: Option<CString>,
}

impl Mp4Fmp4Muxer {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }
}

/// 新しい `Mp4Fmp4Muxer` インスタンスを生成する
///
/// # 引数
///
/// - `tracks`: トラック設定の配列へのポインタ
/// - `track_count`: トラック数
///
/// # 戻り値
///
/// 成功時はインスタンスへのポインタ、失敗時は NULL
///
/// 返されたポインタは `mp4_fmp4_muxer_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_new(
    tracks: *const Mp4Fmp4TrackConfig,
    track_count: u32,
) -> *mut Mp4Fmp4Muxer {
    if tracks.is_null() {
        return std::ptr::null_mut();
    }

    let track_slice = unsafe { std::slice::from_raw_parts(tracks, track_count as usize) };
    let mut track_configs: Vec<Fmp4TrackConfig> = Vec::new();

    for t in track_slice {
        let Some(timescale) = NonZeroU32::new(t.timescale) else {
            return std::ptr::null_mut();
        };
        if t.sample_entry.is_null() {
            return std::ptr::null_mut();
        }
        let sample_entry = unsafe {
            match (&*t.sample_entry).to_sample_entry() {
                Ok(entry) => entry,
                Err(_) => return std::ptr::null_mut(),
            }
        };
        track_configs.push(Fmp4TrackConfig {
            track_kind: t.track_kind.into(),
            timescale,
            sample_entry,
        });
    }

    match Fmp4Muxer::new(track_configs) {
        Ok(inner) => Box::into_raw(Box::new(Mp4Fmp4Muxer {
            inner,
            last_error_string: None,
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `Mp4Fmp4Muxer` インスタンスを破棄してリソースを解放する
///
/// # 引数
///
/// - `muxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_free(muxer: *mut Mp4Fmp4Muxer) {
    if !muxer.is_null() {
        let _ = unsafe { Box::from_raw(muxer) };
    }
}

/// 最後に発生したエラーのメッセージを取得する
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
///
/// # 戻り値
///
/// NULL 終端のエラーメッセージへのポインタ（エラーがない場合は空文字列）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_get_last_error(
    muxer: *const Mp4Fmp4Muxer,
) -> *const c_char {
    if muxer.is_null() {
        return c"".as_ptr();
    }
    let muxer = unsafe { &*muxer };
    let Some(e) = &muxer.last_error_string else {
        return c"".as_ptr();
    };
    e.as_ptr()
}

/// 初期化セグメント（`ftyp` + `moov`）のバイト列を生成する
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
///   - 返されたポインタは `mp4_fmp4_bytes_free()` で解放する必要がある
/// - `out_size`: バイト列のサイズを受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に生成された
/// - その他のエラー: 生成に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_write_init_segment(
    muxer: *mut Mp4Fmp4Muxer,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    if muxer.is_null() || out_data.is_null() || out_size.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let muxer = unsafe { &mut *muxer };

    match muxer.inner.init_segment_bytes() {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            let len = u32::try_from(boxed.len()).expect("init segment size exceeds u32::MAX");
            let ptr = boxed.as_mut_ptr();
            std::mem::forget(boxed);
            unsafe {
                *out_data = ptr;
                *out_size = len;
            }
            Mp4Error::MP4_ERROR_OK
        }
        Err(e) => {
            unsafe {
                *out_data = std::ptr::null_mut();
                *out_size = 0;
            }
            muxer.set_last_error(&format!("[mp4_fmp4_muxer_write_init_segment] {e}"));
            e.into()
        }
    }
}

/// メディアセグメント（`moof` + `mdat`）のバイト列を生成する
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `samples`: サンプル配列へのポインタ
/// - `sample_count`: サンプル数
/// - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
///   - 返されたポインタは `mp4_fmp4_bytes_free()` で解放する必要がある
/// - `out_size`: バイト列のサイズを受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に生成された
/// - その他のエラー: 生成に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_write_media_segment(
    muxer: *mut Mp4Fmp4Muxer,
    samples: *const Mp4Fmp4Sample,
    sample_count: u32,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    if muxer.is_null() || out_data.is_null() || out_size.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if samples.is_null() && sample_count > 0 {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let muxer = unsafe { &mut *muxer };

    let samples_slice = if sample_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(samples, sample_count as usize) }
    };

    let fmp4_samples = unsafe { convert_samples(samples_slice) };

    match muxer.inner.create_media_segment(&fmp4_samples) {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            let len = u32::try_from(boxed.len()).expect("media segment size exceeds u32::MAX");
            let ptr = boxed.as_mut_ptr();
            std::mem::forget(boxed);
            unsafe {
                *out_data = ptr;
                *out_size = len;
            }
            Mp4Error::MP4_ERROR_OK
        }
        Err(e) => {
            unsafe {
                *out_data = std::ptr::null_mut();
                *out_size = 0;
            }
            muxer.set_last_error(&format!("[mp4_fmp4_muxer_write_media_segment] {e}"));
            e.into()
        }
    }
}

/// `sidx` ボックス付きのメディアセグメントを生成する
///
/// `mp4_fmp4_muxer_write_media_segment()` と同じだが、先頭に `sidx` ボックスが付加される。
///
/// # 引数
///
/// `mp4_fmp4_muxer_write_media_segment()` と同じ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_muxer_write_media_segment_with_sidx(
    muxer: *mut Mp4Fmp4Muxer,
    samples: *const Mp4Fmp4Sample,
    sample_count: u32,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    if muxer.is_null() || out_data.is_null() || out_size.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if samples.is_null() && sample_count > 0 {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let muxer = unsafe { &mut *muxer };

    let samples_slice = if sample_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(samples, sample_count as usize) }
    };

    let fmp4_samples = unsafe { convert_samples(samples_slice) };

    match muxer.inner.create_media_segment_with_sidx(&fmp4_samples) {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            let len =
                u32::try_from(boxed.len()).expect("media segment with sidx size exceeds u32::MAX");
            let ptr = boxed.as_mut_ptr();
            std::mem::forget(boxed);
            unsafe {
                *out_data = ptr;
                *out_size = len;
            }
            Mp4Error::MP4_ERROR_OK
        }
        Err(e) => {
            unsafe {
                *out_data = std::ptr::null_mut();
                *out_size = 0;
            }
            muxer.set_last_error(&format!(
                "[mp4_fmp4_muxer_write_media_segment_with_sidx] {e}"
            ));
            e.into()
        }
    }
}

/// `Mp4Fmp4Sample` のスライスを `Fmp4Sample` の `Vec` に変換するヘルパー
///
/// # Safety
///
/// `samples` の各要素の `data` ポインタは、返された `Vec` が使われている間は有効でなければならない。
unsafe fn convert_samples<'a>(samples: &'a [Mp4Fmp4Sample]) -> Vec<Fmp4Sample<'a>> {
    samples
        .iter()
        .map(|s| Fmp4Sample {
            track_index: s.track_index as usize, // u32 -> usize: 常に安全
            duration: s.duration,
            keyframe: s.keyframe,
            composition_time_offset: if s.has_composition_time_offset {
                Some(s.composition_time_offset)
            } else {
                None
            },
            data: if s.data.is_null() {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(s.data, s.data_size as usize) }
            },
        })
        .collect()
}

/// `mp4_fmp4_muxer_write_init_segment()` や `mp4_fmp4_muxer_write_media_segment()` で
/// 割り当てられたバイト列を解放する
///
/// # 引数
///
/// - `data`: 解放するバイト列へのポインタ（NULL の場合は何もしない）
/// - `size`: バイト列のサイズ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_fmp4_bytes_free(data: *mut u8, size: u32) {
    if data.is_null() {
        return;
    }
    // into_boxed_slice して forget したのと同じ方法で解放する
    let _ = unsafe { Vec::from_raw_parts(data, size as usize, size as usize) };
}
