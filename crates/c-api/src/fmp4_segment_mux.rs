//! fMP4 マルチプレックス処理の C API を定義するモジュール
use std::ffi::{CString, c_char};
use std::num::NonZeroU32;
use std::time::Duration;

use shiguredo_mp4::mux::{
    Fmp4SegmentMuxer as RustFmp4SegmentMuxer, MuxError, Sample, SegmentMuxerOptions,
};

use crate::{basic_types::Mp4TrackKind, boxes::Mp4SampleEntry, error::Mp4Error};

/// fMP4 Muxer 生成時のオプションを表す C 構造体
#[repr(C)]
pub struct Fmp4SegmentMuxerOptions {
    /// ファイル作成時刻（UNIX エポックからの秒数）
    pub creation_timestamp_secs: u64,
}

/// fMP4 メディアセグメントに追加するサンプルを表す C 構造体
#[repr(C)]
pub struct Fmp4SegmentSample {
    /// トラックの種別
    pub track_kind: Mp4TrackKind,

    /// タイムスケール（0 は無効）
    pub timescale: u32,

    /// サンプルの詳細情報（コーデック情報）
    ///
    /// 最初のサンプルでは必須。以後、同じトラックで変更がなければ NULL を指定できる。
    pub sample_entry: *const Mp4SampleEntry,

    /// サンプルの尺（トラックのタイムスケール単位）
    pub duration: u32,

    /// キーフレームかどうか
    pub keyframe: bool,

    /// コンポジション時間オフセットが有効かどうか
    pub has_composition_time_offset: bool,

    /// コンポジション時間オフセット（`has_composition_time_offset` が true の場合のみ有効）
    pub composition_time_offset: i32,

    /// セグメント内の `mdat` payload 領域先頭から見たサンプルデータの相対オフセット
    ///
    /// `fmp4_segment_muxer_write_media_segment_metadata()` の返り値には payload 自体は含まれない。
    /// 呼び出し側は返された `moof + mdat header` の直後に、
    /// ここで指定した位置関係になるよう payload を配置する必要がある。
    ///
    /// 同じトラックに属するサンプル群は、`data_offset` の昇順で
    /// 隙間なく連続した 1 区間に配置されている必要がある。
    /// 複数トラックを含む場合は、トラックごとの区間同士も隙間なく並んでいる必要がある。
    pub data_offset: u64,

    /// サンプルデータのサイズ（バイト単位）
    pub data_size: u32,
}

/// fMP4 Muxer の状態を保持する C 構造体
///
/// # 関連関数
///
/// - `fmp4_segment_muxer_new()`: インスタンスを生成する
/// - `fmp4_segment_muxer_free()`: リソースを解放する
/// - `fmp4_segment_muxer_get_last_error()`: 最後のエラーメッセージを取得する
/// - `fmp4_segment_muxer_write_init_segment()`: 初期化セグメントを生成する
/// - `fmp4_segment_muxer_write_media_segment_metadata()`: メディアセグメント先頭メタデータを生成する
/// - `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()`: sidx 付きメディアセグメント先頭メタデータを生成する
/// - `fmp4_segment_muxer_write_mfra()`: `mfra` ボックスを生成する
pub struct Fmp4SegmentMuxer {
    inner: RustFmp4SegmentMuxer,
    last_error_string: Option<CString>,
}

impl Fmp4SegmentMuxer {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }
}

/// 新しい `Fmp4SegmentMuxer` インスタンスを生成する
///
/// デフォルトオプションを使用する。
///
/// # 引数
///
/// # 戻り値
///
/// 成功時はインスタンスへのポインタ、失敗時は NULL
///
/// 返されたポインタは `fmp4_segment_muxer_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_new() -> *mut Fmp4SegmentMuxer {
    unsafe { fmp4_segment_muxer_new_with_options(std::ptr::null()) }
}

/// オプションを指定して新しい `Fmp4SegmentMuxer` インスタンスを生成する
///
/// # 引数
///
/// - `options`: オプションへのポインタ
///   - NULL の場合はデフォルトオプションを使う
///
/// # 戻り値
///
/// 成功時はインスタンスへのポインタ、失敗時は NULL
///
/// 返されたポインタは `fmp4_segment_muxer_free()` で解放する必要がある
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_new_with_options(
    options: *const Fmp4SegmentMuxerOptions,
) -> *mut Fmp4SegmentMuxer {
    let rust_options = if options.is_null() {
        SegmentMuxerOptions::default()
    } else {
        let options = unsafe { &*options };
        SegmentMuxerOptions {
            creation_timestamp: Duration::from_secs(options.creation_timestamp_secs),
        }
    };

    match RustFmp4SegmentMuxer::with_options(rust_options) {
        Ok(inner) => Box::into_raw(Box::new(Fmp4SegmentMuxer {
            inner,
            last_error_string: None,
        })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// `Fmp4SegmentMuxer` インスタンスを破棄してリソースを解放する
///
/// # 引数
///
/// - `muxer`: 破棄するインスタンスへのポインタ（NULL の場合は何もしない）
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_free(muxer: *mut Fmp4SegmentMuxer) {
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
pub unsafe extern "C" fn fmp4_segment_muxer_get_last_error(
    muxer: *const Fmp4SegmentMuxer,
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
/// 返される init segment には、この関数を呼んだ時点までに
/// `fmp4_segment_muxer_write_media_segment_metadata()` ないし
/// `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()` で観測したトラック情報と
/// sample entry が反映される。
///
/// まだどのトラックも観測されていない状態ではエラーになる。
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
///   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
/// - `out_size`: バイト列のサイズを受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に生成された
/// - その他のエラー: 生成に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_init_segment(
    muxer: *mut Fmp4SegmentMuxer,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    if muxer.is_null() || out_data.is_null() || out_size.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let muxer = unsafe { &mut *muxer };
    let result = muxer.inner.init_segment_bytes();
    unsafe {
        write_bytes_result(
            muxer,
            result,
            "fmp4_segment_muxer_write_init_segment",
            out_data,
            out_size,
        )
    }
}

/// メディアセグメント先頭のメタデータ（`moof` + `mdat` ヘッダー）のバイト列を生成する
///
/// 返り値には `mdat` payload 自体は含まれない。
/// 呼び出し側は、この関数が返したバイト列の直後に
/// `Fmp4SegmentSample.data_offset` / `data_size` が示す payload を自前で配置すること。
/// その際、各トラックの payload はトラック単位で連続した 1 区間にまとめ、
/// トラック区間同士も `data_offset` 順に隙間なく並べる必要がある。
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `samples`: サンプル配列へのポインタ
/// - `sample_count`: サンプル数
/// - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
///   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
/// - `out_size`: バイト列のサイズを受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に生成された
/// - その他のエラー: 生成に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_media_segment_metadata(
    muxer: *mut Fmp4SegmentMuxer,
    samples: *const Fmp4SegmentSample,
    sample_count: u32,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    unsafe { write_media_segment_impl(muxer, samples, sample_count, out_data, out_size, false) }
}

/// `sidx` ボックス付きのメディアセグメント先頭メタデータを生成する
///
/// `fmp4_segment_muxer_write_media_segment_metadata()` と同じだが、先頭に `sidx` ボックスが付加される。
/// 返り値は `sidx + moof + mdat` ヘッダーであり、payload は含まれない。
/// payload 配置に関する制約も `fmp4_segment_muxer_write_media_segment_metadata()` と同じである。
///
/// # 引数
///
/// `fmp4_segment_muxer_write_media_segment_metadata()` と同じ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_media_segment_metadata_with_sidx(
    muxer: *mut Fmp4SegmentMuxer,
    samples: *const Fmp4SegmentSample,
    sample_count: u32,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    unsafe { write_media_segment_impl(muxer, samples, sample_count, out_data, out_size, true) }
}

/// ランダムアクセスインデックス（`mfra`）のバイト列を生成する
///
/// `mfra` はファイル末尾に付加することで、fragmented MP4 のランダムアクセスを補助する。
/// `fmp4_segment_muxer_write_init_segment()` と
/// `fmp4_segment_muxer_write_media_segment_metadata()` ないし
/// `fmp4_segment_muxer_write_media_segment_metadata_with_sidx()` を呼び出した後に使うこと。
///
/// `tfra.moof_offset` は、この関数を呼んだ時点での init segment サイズを基準に計算される。
/// したがって、実際に `mfra` を付加するファイルでは、
/// この関数と同じ時点の init segment を先頭に配置する必要がある。
/// 途中で観測済みトラックや sample entry が増えて init segment が変わり得る場合は、
/// 最終的に先頭へ配置する init segment を確定させた後でこの関数を呼ぶこと。
///
/// # 引数
///
/// - `muxer`: インスタンスへのポインタ
/// - `out_data`: 生成されたバイト列へのポインタを受け取るポインタ
///   - 返されたポインタは `fmp4_bytes_free()` で解放する必要がある
/// - `out_size`: バイト列のサイズを受け取るポインタ
///
/// # 戻り値
///
/// - `MP4_ERROR_OK`: 正常に生成された
/// - その他のエラー: 生成に失敗した
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_segment_muxer_write_mfra(
    muxer: *mut Fmp4SegmentMuxer,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    if muxer.is_null() || out_data.is_null() || out_size.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let muxer = unsafe { &mut *muxer };
    let result = muxer.inner.mfra_bytes();
    unsafe {
        write_bytes_result(
            muxer,
            result,
            "fmp4_segment_muxer_write_mfra",
            out_data,
            out_size,
        )
    }
}

/// メディアセグメント生成の共通実装
///
/// # Safety
///
/// 呼び出し元が全ポインタの有効性を保証すること。
unsafe fn write_media_segment_impl(
    muxer: *mut Fmp4SegmentMuxer,
    samples: *const Fmp4SegmentSample,
    sample_count: u32,
    out_data: *mut *mut u8,
    out_size: *mut u32,
    with_sidx: bool,
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

    let fmp4_samples = match unsafe { convert_samples(samples_slice) } {
        Ok(samples) => samples,
        Err(message) => {
            unsafe {
                *out_data = std::ptr::null_mut();
                *out_size = 0;
            }
            muxer.set_last_error(&format!("[write_media_segment_impl] {message}"));
            return Mp4Error::MP4_ERROR_INVALID_INPUT;
        }
    };

    let func_name = if with_sidx {
        "fmp4_segment_muxer_write_media_segment_metadata_with_sidx"
    } else {
        "fmp4_segment_muxer_write_media_segment_metadata"
    };

    let result = if with_sidx {
        muxer
            .inner
            .create_media_segment_metadata_with_sidx(&fmp4_samples)
    } else {
        muxer.inner.create_media_segment_metadata(&fmp4_samples)
    };

    unsafe { write_bytes_result(muxer, result, func_name, out_data, out_size) }
}

/// `Result<Vec<u8>, MuxError>` を C の出力ポインタに書き込む共通ヘルパー
///
/// # Safety
///
/// `out_data` と `out_size` は有効なポインタでなければならない。
unsafe fn write_bytes_result(
    muxer: &mut Fmp4SegmentMuxer,
    result: Result<Vec<u8>, MuxError>,
    func_name: &str,
    out_data: *mut *mut u8,
    out_size: *mut u32,
) -> Mp4Error {
    match result {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            let len = match u32::try_from(boxed.len()) {
                Ok(v) => v,
                Err(_) => {
                    unsafe {
                        *out_data = std::ptr::null_mut();
                        *out_size = 0;
                    }
                    muxer.set_last_error(&format!("[{func_name}] output size exceeds u32::MAX"));
                    return Mp4Error::MP4_ERROR_OTHER;
                }
            };
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
            muxer.set_last_error(&format!("[{func_name}] {e}"));
            e.into()
        }
    }
}

/// `Fmp4SegmentSample` のスライスを [`Sample`] の `Vec` に変換するヘルパー
unsafe fn convert_samples(samples: &[Fmp4SegmentSample]) -> Result<Vec<Sample>, &'static str> {
    samples
        .iter()
        .map(|s| {
            let Some(timescale) = NonZeroU32::new(s.timescale) else {
                return Err("timescale must be non-zero");
            };
            let sample_entry = if s.sample_entry.is_null() {
                None
            } else {
                Some(unsafe {
                    (&*s.sample_entry)
                        .to_sample_entry()
                        .map_err(|_| "sample_entry is invalid")?
                })
            };
            Ok(Sample {
                track_kind: s.track_kind.into(),
                timescale,
                sample_entry,
                duration: s.duration,
                keyframe: s.keyframe,
                composition_time_offset: if s.has_composition_time_offset {
                    Some(s.composition_time_offset)
                } else {
                    None
                },
                data_offset: s.data_offset,
                data_size: s.data_size as usize,
            })
        })
        .collect()
}

/// `fmp4_segment_muxer_write_init_segment()` や `fmp4_segment_muxer_write_media_segment_metadata()` で
/// 割り当てられたバイト列を解放する
///
/// # 引数
///
/// - `data`: 解放するバイト列へのポインタ（NULL の場合は何もしない）
/// - `size`: バイト列のサイズ
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fmp4_bytes_free(data: *mut u8, size: u32) {
    if data.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(data, size as usize) };
    let _ = unsafe { Box::from_raw(slice) };
}
