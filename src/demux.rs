//! MP4 / fMP4 のデマルチプレックス機能を公開するモジュール
//!
//! このモジュールは file ベースの MP4 / fMP4 デマルチプレクサと、
//! fMP4 segment ベースのデマルチプレクサをまとめて公開する。
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::demux::{Input, Mp4FileDemuxer};
//!
//! // MP4 ファイル全体をメモリに読み込む
//! let file_data = std::fs::read("sample.mp4").expect("ファイル読み込み失敗");
//!
//! // デマルチプレックス処理を初期化し、ファイルデータ全体を提供する
//! let mut demuxer = Mp4FileDemuxer::new();
//! let input = Input {
//!     position: 0,
//!     data: &file_data,
//! };
//! demuxer.handle_input(input);
//!
//! // トラック情報を取得する
//! let tracks = demuxer.tracks().expect("トラック取得失敗");
//! println!("{}個のトラックが見つかりました", tracks.len());
//! for track in tracks {
//!     println!("トラックID: {}, 種類: {:?}, 尺: {}, タイムスケール: {}",
//!              track.track_id, track.kind, track.duration, track.timescale);
//! }
//! ```
pub use crate::demux_fmp4_file::Fmp4FileDemuxer;
pub use crate::demux_fmp4_segment::Fmp4SegmentDemuxer;
pub use crate::demux_mp4_file::{
    DemuxError, Input, Mp4FileDemuxer, RequiredInput, Sample, TrackInfo,
};
pub use crate::demux_mp4_file_kind_detector::{Mp4FileKind, Mp4FileKindDetector};
