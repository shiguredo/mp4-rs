use std::ffi::{CString, c_char};

use shiguredo_mp4::demux::{Input, Mp4FileKindDetector as RustMp4FileKindDetector};

use crate::error::Mp4Error;

#[repr(C)]
#[expect(non_camel_case_types)]
pub enum Mp4FileKind {
    MP4_FILE_KIND_MP4 = 0,
    MP4_FILE_KIND_FRAGMENTED_MP4 = 1,
}

impl From<shiguredo_mp4::demux::Mp4FileKind> for Mp4FileKind {
    fn from(kind: shiguredo_mp4::demux::Mp4FileKind) -> Self {
        match kind {
            shiguredo_mp4::demux::Mp4FileKind::Mp4 => Self::MP4_FILE_KIND_MP4,
            shiguredo_mp4::demux::Mp4FileKind::FragmentedMp4 => Self::MP4_FILE_KIND_FRAGMENTED_MP4,
        }
    }
}

pub struct Mp4FileKindDetector {
    inner: RustMp4FileKindDetector,
    last_error_string: Option<CString>,
}

impl Mp4FileKindDetector {
    fn set_last_error(&mut self, message: &str) {
        self.last_error_string = CString::new(message).ok();
    }
}

/// 新しい `Mp4FileKindDetector` インスタンスを生成する
///
/// 返されたポインタは、使用後に `mp4_file_kind_detector_free()` で解放する必要がある。
#[unsafe(no_mangle)]
pub extern "C" fn mp4_file_kind_detector_new() -> *mut Mp4FileKindDetector {
    Box::into_raw(Box::new(Mp4FileKindDetector {
        inner: RustMp4FileKindDetector::new(),
        last_error_string: None,
    }))
}

/// `Mp4FileKindDetector` インスタンスを破棄してリソースを解放する
///
/// NULL が渡された場合は何もしない。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_kind_detector_free(detector: *mut Mp4FileKindDetector) {
    if !detector.is_null() {
        let _ = unsafe { Box::from_raw(detector) };
    }
}

/// 最後に発生したエラーメッセージを取得する
///
/// エラーが発生していない場合は空文字列を返す。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_kind_detector_get_last_error(
    detector: *const Mp4FileKindDetector,
) -> *const c_char {
    if detector.is_null() {
        return c"".as_ptr();
    }
    let detector = unsafe { &*detector };
    let Some(e) = &detector.last_error_string else {
        return c"".as_ptr();
    };
    e.as_ptr()
}

/// 次の判定に必要な入力データの位置とサイズを取得する
///
/// `out_required_input_size` には以下のいずれかが設定される:
/// - 0: 追加の入力が不要
/// - -1: ファイル末尾までの入力が必要
/// - それ以外の正値: そのサイズ以上の入力が必要
///
/// ここで大きなサイズが要求されるのは実質的には `moov` ボックス本体であり、
/// `mdat` のような巨大ペイロードを丸ごと要求することはない想定である。
/// そのため、サイズ表現には `int32_t` を使っている。
///
/// 判定器がエラー状態に遷移している場合は `MP4_ERROR_OK` ではなくエラーを返す。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_kind_detector_get_required_input(
    detector: *mut Mp4FileKindDetector,
    out_required_input_position: *mut u64,
    out_required_input_size: *mut i32,
) -> Mp4Error {
    if detector.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let detector = unsafe { &mut *detector };

    if out_required_input_position.is_null() {
        detector.set_last_error(
            "[mp4_file_kind_detector_get_required_input] out_required_input_position is null",
        );
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    if out_required_input_size.is_null() {
        detector.set_last_error(
            "[mp4_file_kind_detector_get_required_input] out_required_input_size is null",
        );
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    if let Err(e) = detector.inner.file_kind() {
        detector.set_last_error(&format!("[mp4_file_kind_detector_get_required_input] {e}"));
        return e.into();
    }

    unsafe {
        if let Some(required) = detector.inner.required_input() {
            *out_required_input_position = required.position;
            *out_required_input_size = required.size.map(|n| n as i32).unwrap_or(-1);
        } else {
            *out_required_input_position = 0;
            *out_required_input_size = 0;
        }
    }

    Mp4Error::MP4_ERROR_OK
}

/// 入力データを供給して判定処理を進める
///
/// `mp4_file_kind_detector_get_required_input()` が返した要求に従って入力を渡すこと。
///
/// EOF を通知する場合には、要求された `input_position` に対して
/// `input_data = NULL` かつ `input_data_size = 0` を渡す。
///
/// 入力が不正で判定器がエラー状態に遷移した場合は、その場でエラーを返す。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_kind_detector_handle_input(
    detector: *mut Mp4FileKindDetector,
    input_position: u64,
    input_data: *const u8,
    input_data_size: u32,
) -> Mp4Error {
    if detector.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let detector = unsafe { &mut *detector };

    let input = if input_data.is_null() {
        if input_data_size != 0 {
            detector.set_last_error(
                "[mp4_file_kind_detector_handle_input] input_data is null but input_data_size is non-zero",
            );
            return Mp4Error::MP4_ERROR_NULL_POINTER;
        }
        Input {
            position: input_position,
            data: &[],
        }
    } else {
        let data = unsafe { std::slice::from_raw_parts(input_data, input_data_size as usize) };
        Input {
            position: input_position,
            data,
        }
    };

    detector.inner.handle_input(input);
    match detector.inner.file_kind() {
        Ok(_) => Mp4Error::MP4_ERROR_OK,
        Err(e) => {
            detector.set_last_error(&format!("[mp4_file_kind_detector_handle_input] {e}"));
            e.into()
        }
    }
}

/// 判定結果を取得する
///
/// 戻り値が `MP4_ERROR_OK` の場合にのみ `out_kind` が有効になる。
/// まだ追加入力が必要な場合は `MP4_ERROR_INPUT_REQUIRED` が返る。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mp4_file_kind_detector_get_file_kind(
    detector: *mut Mp4FileKindDetector,
    out_kind: *mut Mp4FileKind,
) -> Mp4Error {
    if detector.is_null() {
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }
    let detector = unsafe { &mut *detector };

    if out_kind.is_null() {
        detector.set_last_error("[mp4_file_kind_detector_get_file_kind] out_kind is null");
        return Mp4Error::MP4_ERROR_NULL_POINTER;
    }

    match detector.inner.file_kind() {
        Ok(Some(kind)) => {
            unsafe { *out_kind = kind.into() };
            Mp4Error::MP4_ERROR_OK
        }
        Ok(None) => Mp4Error::MP4_ERROR_INPUT_REQUIRED,
        Err(e) => {
            detector.set_last_error(&format!("[mp4_file_kind_detector_get_file_kind] {e}"));
            e.into()
        }
    }
}
