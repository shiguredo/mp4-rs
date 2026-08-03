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
//!     composition_time_offset: None,
//!     data_offset: 0,
//!     data_size: 0,
//! };
//! let _ = (&mut muxer, sample);
//! ```
//!
//! トラックの言語（`mdhd.language`）とトラック名（`hdlr.name`）を指定する例:
//!
//! ```no_run
//! use shiguredo_mp4::mux::{Mp4FileMuxer, Mp4FileMuxerOptions, TrackMetadata};
//! use shiguredo_mp4::{LanguageCode, Utf8String};
//!
//! let options = Mp4FileMuxerOptions {
//!     subtitle_track: TrackMetadata {
//!         language: LanguageCode::from_ascii("eng").expect("valid language code"),
//!         name: Utf8String::new("English").expect("no null byte"),
//!     },
//!     ..Default::default()
//! };
//! let _muxer = Mp4FileMuxer::with_options(options).expect("muxer creation failed");
//! ```
pub use crate::mux_fmp4_segment::{Fmp4SegmentMuxer, SegmentMuxerOptions};
pub use crate::mux_mp4_file::{
    FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, MuxError, Sample, TrackMetadata,
    estimate_maximum_moov_box_size,
};
