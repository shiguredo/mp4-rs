//! ../../../src/basic_types.rs の C API を定義するためのモジュール
use shiguredo_mp4::TrackKind;

/// MP4 ファイル内のトラックの種類を表す列挙型
#[derive(Clone, Copy)]
#[repr(C)]
#[expect(non_camel_case_types)]
pub enum Mp4TrackKind {
    /// 音声トラック
    MP4_TRACK_KIND_AUDIO = 0,

    /// 映像トラック
    MP4_TRACK_KIND_VIDEO = 1,

    /// 字幕トラック
    MP4_TRACK_KIND_SUBTITLE = 2,
}

impl From<TrackKind> for Mp4TrackKind {
    fn from(kind: TrackKind) -> Self {
        match kind {
            TrackKind::Audio => Self::MP4_TRACK_KIND_AUDIO,
            TrackKind::Video => Self::MP4_TRACK_KIND_VIDEO,
            TrackKind::Subtitle => Self::MP4_TRACK_KIND_SUBTITLE,
        }
    }
}

impl From<Mp4TrackKind> for TrackKind {
    fn from(kind: Mp4TrackKind) -> Self {
        match kind {
            Mp4TrackKind::MP4_TRACK_KIND_AUDIO => Self::Audio,
            Mp4TrackKind::MP4_TRACK_KIND_VIDEO => Self::Video,
            Mp4TrackKind::MP4_TRACK_KIND_SUBTITLE => Self::Subtitle,
        }
    }
}
