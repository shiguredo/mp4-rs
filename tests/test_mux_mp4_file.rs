//! `src/mux_mp4_file.rs` の境界値・エラーパス単体テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_mux_demux.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry,
        VisualSampleEntryFields,
    },
    demux::{Input, Mp4FileDemuxer},
    mux::{FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, Sample},
};

/// テスト用の H.264 SampleEntry を作成
fn create_avc1_sample_entry(width: u16, height: u16) -> SampleEntry {
    SampleEntry::Avc1(Avc1Box {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width,
            height,
            horizresolution: VisualSampleEntryFields::DEFAULT_HORIZRESOLUTION,
            vertresolution: VisualSampleEntryFields::DEFAULT_VERTRESOLUTION,
            frame_count: VisualSampleEntryFields::DEFAULT_FRAME_COUNT,
            compressorname: VisualSampleEntryFields::NULL_COMPRESSORNAME,
            depth: VisualSampleEntryFields::DEFAULT_DEPTH,
        },
        avcc_box: AvccBox {
            avc_profile_indication: 66, // Baseline Profile
            profile_compatibility: 0,
            avc_level_indication: 30,
            length_size_minus_one: Uint::new(3),
            sps_list: vec![],
            pps_list: vec![],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    })
}

/// テスト用の Opus SampleEntry を作成
fn create_opus_sample_entry(channel_count: u8) -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: channel_count as u16,
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(48000u16, 0),
        },
        dops_box: DopsBox {
            output_channel_count: channel_count,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        },
        unknown_boxes: vec![],
    })
}

/// FinalizedBoxes からファイルデータを構築する
///
/// non-faststart の場合: ftyp | mdat_header | mdat_data | moov
fn build_file_data(
    initial_bytes: &[u8],
    finalized: &FinalizedBoxes,
    sample_data_size: usize,
) -> Vec<u8> {
    // 全体のサイズを計算（十分なサイズを確保）
    let total_size = initial_bytes.len() + sample_data_size + finalized.moov_box_size() + 1024;
    let mut file_data = vec![0u8; total_size];

    // initial bytes をコピー
    file_data[..initial_bytes.len()].copy_from_slice(initial_bytes);

    // offset_and_bytes_pairs() で各ボックスを書き込む
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let offset = offset as usize;
        file_data[offset..offset + bytes.len()].copy_from_slice(bytes);
    }

    // 実際のファイルサイズにトリミング
    // moov の終端を見つける
    let mut max_end = initial_bytes.len() + sample_data_size;
    for (offset, bytes) in finalized.offset_and_bytes_pairs() {
        let end = offset as usize + bytes.len();
        if end > max_end {
            max_end = end;
        }
    }
    file_data.truncate(max_end);
    file_data
}

// ===== 境界値テスト =====

mod mux_mp4_file_boundary_tests {
    use super::*;

    /// 最小構成のビデオファイル
    #[test]
    fn minimal_video_file() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer の作成に失敗した");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(640, 480)),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("30 は非ゼロ"),
            duration: 1,
            composition_time_offset: None,
            data_offset,
            data_size: 100,
        };
        muxer
            .append_sample(&sample)
            .expect("sample の追加に失敗した");

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");

        let file_data = build_file_data(&initial_bytes, finalized, 100);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert!(matches!(tracks[0].kind, TrackKind::Video));

        let sample = demuxer
            .next_sample()
            .expect("sample の読み取りに失敗した")
            .expect("sample が無い");
        assert!(sample.keyframe);
        assert_eq!(sample.data_size, 100);
    }

    /// faststart が有効な場合の roundtrip
    #[test]
    fn faststart_enabled_roundtrip() {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: 8192,
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(1920, 1080)),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("30 は非ゼロ"),
            duration: 1,
            composition_time_offset: None,
            data_offset,
            data_size: 1024,
        };
        muxer
            .append_sample(&sample)
            .expect("sample の追加に失敗した");

        let initial_bytes = muxer.initial_boxes_bytes().to_vec();
        let finalized = muxer.finalize().expect("finalize に失敗した");
        assert!(finalized.is_faststart_enabled());

        // faststart 用のファイルデータを構築
        let file_data = build_file_data(&initial_bytes, finalized, 1024);

        let mut demuxer = Mp4FileDemuxer::new();
        demuxer.handle_input(Input {
            position: 0,
            data: &file_data,
        });

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
    }
}

mod estimate_moov_size_unit_tests {
    use shiguredo_mp4::mux::estimate_maximum_moov_box_size;

    /// 空のトラックリストの場合
    #[test]
    fn estimate_empty_tracks() {
        let result = estimate_maximum_moov_box_size(&[]);
        // 基本オーバーヘッドのみ
        assert!(result > 0);
    }

    /// 単一トラック、サンプルなし
    #[test]
    fn estimate_single_track_no_samples() {
        let result = estimate_maximum_moov_box_size(&[0]);
        assert!(result > 0);
    }

    /// 大量のサンプルがある場合
    #[test]
    fn estimate_large_sample_count() {
        let result = estimate_maximum_moov_box_size(&[100000, 100000]);
        // 大量のサンプルでもオーバーフローしない
        assert!(result > 0);
    }
}

// ===== mux.rs のエラーパステスト =====

mod mux_error_tests {
    use std::num::NonZeroU32;

    use shiguredo_mp4::{
        TrackKind,
        mux::{Mp4FileMuxer, MuxError, Sample},
    };

    /// タイムスケール不一致エラー (Video)
    #[test]
    fn timescale_mismatch_video() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer は作成できる");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 最初のサンプル (timescale = 30)
        let sample1 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(super::create_avc1_sample_entry(1920, 1080)),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer.append_sample(&sample1).expect("sample1 は成功する");

        // 2番目のサンプル (timescale = 60) - 不一致
        let sample2 = Sample {
            track_kind: TrackKind::Video,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::new(60).expect("timescale は非ゼロである"), // 不一致
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size + 1024,
            data_size: 512,
        };
        let result = muxer.append_sample(&sample2);
        assert!(matches!(
            result,
            Err(MuxError::TimescaleMismatch {
                track_kind: TrackKind::Video,
                ..
            })
        ));
    }

    /// タイムスケール不一致エラー (Audio)
    #[test]
    fn timescale_mismatch_audio() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer は作成できる");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        // 最初のサンプル (timescale = 48000)
        let sample1 = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(super::create_opus_sample_entry(2)),
            keyframe: false,
            timescale: NonZeroU32::new(48000).expect("timescale は非ゼロである"),
            duration: 960,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 256,
        };
        muxer.append_sample(&sample1).expect("sample1 は成功する");

        // 2番目のサンプル (timescale = 44100) - 不一致
        let sample2 = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: None,
            keyframe: false,
            timescale: NonZeroU32::new(44100).expect("timescale は非ゼロである"), // 不一致
            duration: 1024,
            composition_time_offset: None,
            data_offset: initial_size + 256,
            data_size: 256,
        };
        let result = muxer.append_sample(&sample2);
        assert!(matches!(
            result,
            Err(MuxError::TimescaleMismatch {
                track_kind: TrackKind::Audio,
                ..
            })
        ));
    }

    /// MuxError の Display 実装テスト
    #[test]
    fn mux_error_display() {
        // PositionMismatch
        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        let display_str = format!("{pos_error}");
        assert!(display_str.contains("100"));
        assert!(display_str.contains("200"));

        // MissingSampleEntry
        let missing_error = MuxError::MissingSampleEntry {
            track_kind: TrackKind::Video,
        };
        let display_str = format!("{missing_error}");
        assert!(display_str.contains("Video"));

        // AlreadyFinalized
        let finalized_error = MuxError::AlreadyFinalized;
        let display_str = format!("{finalized_error}");
        assert!(display_str.contains("finalized"));

        // TimescaleMismatch
        let timescale_error = MuxError::TimescaleMismatch {
            track_kind: TrackKind::Audio,
            expected: NonZeroU32::new(48000).expect("timescale は非ゼロである"),
            actual: NonZeroU32::new(44100).expect("timescale は非ゼロである"),
        };
        let display_str = format!("{timescale_error}");
        assert!(display_str.contains("Audio"));
        assert!(display_str.contains("48000"));
        assert!(display_str.contains("44100"));
    }

    /// MuxError の Debug 実装テスト
    /// Debug 実装は Display と同じ出力を返すため、Display の出力を検証する
    #[test]
    fn mux_error_debug() {
        let error = MuxError::AlreadyFinalized;
        let debug_str = format!("{error:?}");
        assert!(debug_str.contains("finalized"));

        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        let debug_str = format!("{pos_error:?}");
        assert!(debug_str.contains("mismatch"));
    }

    /// MuxError::source() のテスト
    #[test]
    fn mux_error_source() {
        use std::error::Error as StdError;

        // 他のエラーでは source は None
        let other_error = MuxError::AlreadyFinalized;
        assert!(other_error.source().is_none());

        let pos_error = MuxError::PositionMismatch {
            expected: 100,
            actual: 200,
        };
        assert!(pos_error.source().is_none());
    }

    /// 二重 finalize エラーのテスト
    #[test]
    fn double_finalize_error() {
        let mut muxer = Mp4FileMuxer::new().expect("muxer は作成できる");
        let initial_size = muxer.initial_boxes_bytes().len() as u64;

        let sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(super::create_avc1_sample_entry(1920, 1080)),
            keyframe: true,
            timescale: NonZeroU32::new(30).expect("timescale は非ゼロである"),
            duration: 1,
            composition_time_offset: None,
            data_offset: initial_size,
            data_size: 1024,
        };
        muxer.append_sample(&sample).expect("sample は成功する");

        // 最初の finalize は成功
        muxer.finalize().expect("最初の finalize は成功する");

        // 2回目の finalize は失敗
        let result = muxer.finalize();
        assert!(matches!(result, Err(MuxError::AlreadyFinalized)));
    }
}
