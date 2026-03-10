//! shiguredo_mp4 のエラーをまとめて定義するためのモジュール
//!
//! C API で細かくエラー方が分かれていると煩雑なので、ひとつに集約している
use shiguredo_mp4::{
    Error, ErrorKind, aux::SampleTableAccessorError, demux::DemuxError, demux::SegmentDemuxError,
    mux::MuxError, mux_fmp4_segment::Fmp4SegmentMuxError,
};

/// 発生する可能性のあるエラーの種類を表現する列挙型
#[repr(C)]
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
            _ => Self::MP4_ERROR_OTHER,
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
            DemuxError::InputRequired(_) => Self::MP4_ERROR_INPUT_REQUIRED,
            _ => Self::MP4_ERROR_OTHER,
        }
    }
}

impl From<MuxError> for Mp4Error {
    fn from(e: MuxError) -> Self {
        match e {
            MuxError::EncodeError(e) => e.into(),
            MuxError::AlreadyFinalized => Self::MP4_ERROR_INVALID_STATE,
            MuxError::PositionMismatch { .. }
            | MuxError::MissingSampleEntry { .. }
            | MuxError::TimescaleMismatch { .. } => Self::MP4_ERROR_INVALID_INPUT,
            _ => Self::MP4_ERROR_OTHER,
        }
    }
}

impl From<Fmp4SegmentMuxError> for Mp4Error {
    fn from(e: Fmp4SegmentMuxError) -> Self {
        match e {
            Fmp4SegmentMuxError::EncodeError(e) => e.into(),
            Fmp4SegmentMuxError::EmptyTracks
            | Fmp4SegmentMuxError::EmptySamples
            | Fmp4SegmentMuxError::InvalidTrackIndex { .. } => Self::MP4_ERROR_INVALID_INPUT,
            _ => Self::MP4_ERROR_OTHER,
        }
    }
}

impl From<SegmentDemuxError> for Mp4Error {
    fn from(e: SegmentDemuxError) -> Self {
        match e {
            SegmentDemuxError::DecodeError(e) => e.into(),
            SegmentDemuxError::NotInitialized | SegmentDemuxError::AlreadyInitialized => {
                Self::MP4_ERROR_INVALID_STATE
            }
            SegmentDemuxError::UnknownTrackId(_) => Self::MP4_ERROR_INVALID_DATA,
            _ => Self::MP4_ERROR_OTHER,
        }
    }
}
