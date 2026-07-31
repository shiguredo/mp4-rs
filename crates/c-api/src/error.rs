//! shiguredo_mp4 のエラーをまとめて定義するためのモジュール
//!
//! C API で細かくエラー型が分かれていると煩雑なので、ひとつに集約している
use shiguredo_mp4::{
    Error, ErrorKind, aux::SampleTableAccessorError, demux::DemuxError, mux::MuxError,
};

/// 発生する可能性のあるエラーの種類を表現する列挙型
#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
#[expect(non_camel_case_types)]
pub enum Mp4Error {
    /// エラーが発生しなかったことを示す
    MP4_ERROR_OK = 0,

    /// 入力引数ないしパラメーターが無効である
    MP4_ERROR_INVALID_INPUT,

    /// 入力データが破損しているか無効な形式である
    MP4_ERROR_INVALID_DATA,

    /// 操作に対する内部状態が無効である
    MP4_ERROR_INVALID_STATE,

    /// 入力データの読み込みが必要である
    MP4_ERROR_INPUT_REQUIRED,

    /// 出力データの書き込みが必要である
    MP4_ERROR_OUTPUT_REQUIRED,

    /// NULL ポインタが渡された
    MP4_ERROR_NULL_POINTER,

    /// これ以上読み込むサンプルが存在しない
    MP4_ERROR_NO_MORE_SAMPLES,

    /// 操作またはデータ形式がサポートされていない
    MP4_ERROR_UNSUPPORTED,

    /// 上記以外のエラーが発生した
    MP4_ERROR_OTHER,
}

impl From<Error> for Mp4Error {
    fn from(e: Error) -> Self {
        match e.kind {
            ErrorKind::InvalidInput => Self::MP4_ERROR_INVALID_INPUT,
            ErrorKind::InvalidData => Self::MP4_ERROR_INVALID_DATA,
            ErrorKind::Unsupported => Self::MP4_ERROR_UNSUPPORTED,
            // Mp4Error にバッファ不足用コードが無く、意味変更しないため OTHER にマップする
            ErrorKind::InsufficientBuffer => Self::MP4_ERROR_OTHER,
        }
    }
}

impl From<SampleTableAccessorError> for Mp4Error {
    fn from(_e: SampleTableAccessorError) -> Self {
        Self::MP4_ERROR_INVALID_DATA
    }
}

impl From<DemuxError> for Mp4Error {
    fn from(e: DemuxError) -> Self {
        match e {
            DemuxError::DecodeError(e) => e.into(),
            DemuxError::SampleTableError(e) => e.into(),
            DemuxError::InvalidState(_) => Self::MP4_ERROR_INVALID_STATE,
            DemuxError::InputRequired(_) => Self::MP4_ERROR_INPUT_REQUIRED,
        }
    }
}

impl From<MuxError> for Mp4Error {
    fn from(e: MuxError) -> Self {
        match e {
            MuxError::EncodeError(e) => e.into(),
            MuxError::AlreadyFinalized => Self::MP4_ERROR_INVALID_STATE,
            MuxError::Overflow => Self::MP4_ERROR_OTHER,
            MuxError::EmptyTracks
            | MuxError::EmptySamples
            | MuxError::PositionMismatch { .. }
            | MuxError::MissingSampleEntry { .. }
            | MuxError::TimescaleMismatch { .. }
            | MuxError::MixedSampleEntries { .. }
            | MuxError::NoSyncSamples { .. } => Self::MP4_ERROR_INVALID_INPUT,
        }
    }
}

/// `RequiredInput.size` (`Option<usize>`) を C API の `out_required_input_size` (`i32`) に変換する
///
/// - `None` → `Ok(-1)`（ファイル末尾まで必要）
/// - `Some(n)` かつ `n <= i32::MAX` → `Ok(n as i32)`
/// - `Some(n)` かつ `n > i32::MAX` → `Err`（要求サイズが `i32` に収まらない）
///
/// C API は `int32_t` でサイズを返す設計のため、それを超える要求はサポート外としてエラーにする。
/// `as i32` による切り捨てだと `-1`（EOF）と衝突するため、`try_from` で明示的に失敗させる。
pub(crate) fn required_input_size_to_i32(size: Option<usize>) -> Result<i32, String> {
    match size {
        None => Ok(-1),
        Some(n) => i32::try_from(n)
            .map_err(|_| format!("required input size ({n}) exceeds i32::MAX ({})", i32::MAX)),
    }
}

#[cfg(test)]
mod tests {
    use super::required_input_size_to_i32;

    /// None は API 上の -1（EOF まで必要）に対応する
    #[test]
    fn converts_none_to_minus_one() {
        assert_eq!(required_input_size_to_i32(None), Ok(-1));
    }

    /// 0 は「追加入力不要」と衝突しない正の境界（サイズ 0 バイト要求）として通す
    #[test]
    fn converts_zero() {
        assert_eq!(required_input_size_to_i32(Some(0)), Ok(0));
    }

    /// 通常の正値をそのまま返す
    #[test]
    fn converts_one() {
        assert_eq!(required_input_size_to_i32(Some(1)), Ok(1));
    }

    /// i32::MAX ちょうどは表現可能な上限として成功する
    #[test]
    fn converts_i32_max() {
        assert_eq!(
            required_input_size_to_i32(Some(i32::MAX as usize)),
            Ok(i32::MAX)
        );
    }

    /// i32::MAX + 1 は as i32 だと i32::MIN になるため、エラーにする
    #[test]
    fn rejects_i32_max_plus_one() {
        let err = required_input_size_to_i32(Some(i32::MAX as usize + 1)).unwrap_err();
        assert!(
            err.contains("exceeds i32::MAX"),
            "エラーメッセージに超過である旨が含まれること: {err}"
        );
    }

    /// usize::MAX は as i32 だと -1（EOF）と衝突するため、エラーにする
    #[test]
    fn rejects_usize_max() {
        let err = required_input_size_to_i32(Some(usize::MAX)).unwrap_err();
        assert!(
            err.contains("exceeds i32::MAX"),
            "エラーメッセージに超過である旨が含まれること: {err}"
        );
    }
}
