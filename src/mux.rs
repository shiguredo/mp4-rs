//! MP4 / fMP4 のマルチプレックス機能を公開するモジュール
//!
//! このモジュールは file ベースの MP4 mux と、
//! fMP4 segment ベースの mux をまとめて公開する。
//!
//! # Examples
//!
//! ```no_run
//! use shiguredo_mp4::mux::{Mp4FileMuxer, Sample};
//! use shiguredo_mp4::TrackKind;
//!
//! let mut muxer = Mp4FileMuxer::new().expect("muxer creation failed");
//! let sample = Sample {
//!     track_kind: TrackKind::Video,
//!     sample_entry: None,
//!     keyframe: true,
//!     timescale: core::num::NonZeroU32::MIN,
//!     duration: 1,
//!     data_offset: 0,
//!     data_size: 0,
//! };
//! let _ = (&mut muxer, sample);
//! ```
pub use crate::mux_fmp4_segment::{
    Fmp4SegmentMuxer, SegmentMuxError, SegmentSample, SegmentTrackConfig,
};
pub use crate::mux_mp4_file::{
    FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, MuxError, Sample,
    estimate_maximum_moov_box_size,
};
