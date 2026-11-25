//! `src/mux_mp4_file.rs` の境界値・エラーパス単体テスト
//!
//! 正常系のラウンドトリップは `pbt/tests/prop_mux_demux.rs` が担う。

use std::num::NonZeroU32;

use shiguredo_mp4::{
    ErrorKind, FixedPointNumber, FullBox, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, ElstBox, ElstEntry, OpusBox,
        SampleEncryptionInfo, SampleEntry, VisualSampleEntryFields,
    },
    demux::{Input, Mp4FileDemuxer},
    mux::{
        EditList, FinalizedBoxes, Mp4FileMuxer, Mp4FileMuxerOptions, MuxError, Sample,
        TrackMetadata,
    },
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
            encryption_info: None,
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
        assert!(
            sample.encryption_info.is_none(),
            "Mp4FileDemuxer は常に encryption_info: None を返す"
        );
    }

    /// `encryption_info: Some` を渡すと MuxError::EncodeError(InvalidInput) になること
    ///
    /// 非 fragmented MP4 にはサンプル単位の暗号化情報を格納できないため、
    /// 黙って無視せずに拒否する。
    #[test]
    fn append_sample_with_encryption_info_is_rejected() {
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
            encryption_info: Some(SampleEncryptionInfo {
                iv: vec![0x11; 16],
                subsamples: Vec::new(),
                kid: None,
            }),
        };
        let err = muxer
            .append_sample(&sample)
            .expect_err("暗号化サンプルを追加できてしまった");
        match err {
            MuxError::EncodeError(error) => {
                assert_eq!(
                    error.kind,
                    ErrorKind::InvalidInput,
                    "エラー種別が InvalidInput ではない (reason={})",
                    error.reason,
                );
                assert!(
                    error.reason.contains("does not support encrypted samples"),
                    "暗号化サンプルの検査とは別のエラーである (reason={})",
                    error.reason,
                );
            }
            other => panic!("EncodeError を期待したが {other:?} だった"),
        }
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
            encryption_info: None,
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
            encryption_info: None,
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
            encryption_info: None,
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
            encryption_info: None,
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
            encryption_info: None,
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
            encryption_info: None,
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

// ===== 編集リスト（edts / elst）のテスト =====

mod edit_list_tests {
    use super::*;

    /// `media_rate` が 1.0 のエントリーを生成する
    fn one_rate() -> FixedPointNumber<i16, i16> {
        FixedPointNumber::new(1i16, 0i16)
    }

    /// 編集リストを生成する
    fn edit_list(movie_timescale: u32, media_timescale: u32, entries: Vec<ElstEntry>) -> EditList {
        EditList {
            movie_timescale: NonZeroU32::new(movie_timescale).expect("movie_timescale は非ゼロ"),
            media_timescale: NonZeroU32::new(media_timescale).expect("media_timescale は非ゼロ"),
            elst_box: ElstBox { entries },
        }
    }

    /// 音声 1 サンプルを追加する
    fn append_audio_sample(muxer: &mut Mp4FileMuxer, data_offset: u64) {
        let sample = Sample {
            encryption_info: None,
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry(2)),
            keyframe: true,
            timescale: NonZeroU32::new(44100).expect("44100 は非ゼロ"),
            duration: 45124,
            composition_time_offset: None,
            data_offset,
            data_size: 64,
        };
        muxer
            .append_sample(&sample)
            .expect("audio sample の追加に失敗した");
    }

    /// 音声 1 トラックを編集リスト付きで mux する
    fn mux_audio_with_edit_list(edit_list: EditList) -> Result<FinalizedBoxes, MuxError> {
        let options = Mp4FileMuxerOptions {
            audio_track: TrackMetadata {
                edit_list: Some(edit_list),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;
        append_audio_sample(&mut muxer, data_offset);
        muxer.finalize().cloned()
    }

    /// 出力された `elst` を返す
    fn elst_of(finalized: &FinalizedBoxes) -> &ElstBox {
        finalized.moov_box().trak_boxes[0]
            .edts_box
            .as_ref()
            .expect("edts が出力されていない")
            .elst_box
            .as_ref()
            .expect("elst が出力されていない")
    }

    /// `edit_duration` が出力 movie timescale に換算されること
    #[test]
    fn edit_list_is_written_with_output_movie_timescale() {
        // 入力 movie timescale は 1000、出力は音声トラックの 44100 になる
        let finalized = mux_audio_with_edit_list(edit_list(
            1000,
            44100,
            vec![ElstEntry {
                edit_duration: 1000,
                media_time: 1024,
                media_rate: one_rate(),
            }],
        ))
        .expect("編集リスト付きの mux に失敗した");

        let moov_box = finalized.moov_box();
        assert_eq!(moov_box.mvhd_box.timescale.get(), 44100);
        assert_eq!(moov_box.mvhd_box.duration, 44100);
        assert_eq!(moov_box.trak_boxes[0].tkhd_box.duration, 44100);

        let elst_box = elst_of(&finalized);
        assert_eq!(elst_box.entries.len(), 1);
        assert_eq!(elst_box.entries[0].edit_duration, 44100);
        assert_eq!(elst_box.entries[0].media_time, 1024);
    }

    /// `media_timescale` が出力トラックの timescale と一致しない場合は拒否されること
    #[test]
    fn edit_list_media_timescale_mismatch_is_rejected() {
        let error = mux_audio_with_edit_list(edit_list(
            1000,
            48000,
            vec![ElstEntry {
                edit_duration: 1000,
                media_time: 1024,
                media_rate: one_rate(),
            }],
        ))
        .expect_err("timescale の不一致は拒否される");
        assert!(
            matches!(error, MuxError::TimescaleMismatch { .. }),
            "unexpected error: {error:?}"
        );
    }

    /// empty edit と 1.0 以外の `media_rate` が保持されること
    #[test]
    fn edit_list_empty_edit_and_media_rate_are_preserved() {
        let finalized = mux_audio_with_edit_list(edit_list(
            1000,
            44100,
            vec![
                ElstEntry {
                    edit_duration: 500,
                    media_time: -1,
                    media_rate: one_rate(),
                },
                ElstEntry {
                    edit_duration: 1000,
                    media_time: 0,
                    media_rate: FixedPointNumber::new(2i16, 0i16),
                },
            ],
        ))
        .expect("編集リスト付きの mux に失敗した");

        let elst_box = elst_of(&finalized);
        assert_eq!(elst_box.entries.len(), 2);
        assert_eq!(elst_box.entries[0].media_time, -1);
        assert_eq!(elst_box.entries[1].media_time, 0);
        assert_eq!(
            elst_box.entries[1].media_rate,
            FixedPointNumber::new(2i16, 0i16)
        );
        assert_eq!(elst_box.entries[1].edit_duration, 44100);
    }

    /// `elst` の version が値域から再計算されること
    #[test]
    fn edit_list_version_is_recomputed_from_values() {
        // 換算後の値が u32 に収まる場合は version 0
        let v0 = mux_audio_with_edit_list(edit_list(
            1,
            44100,
            vec![ElstEntry {
                edit_duration: 1000,
                media_time: 0,
                media_rate: one_rate(),
            }],
        ))
        .expect("編集リスト付きの mux に失敗した");
        assert_eq!(elst_of(&v0).entries[0].edit_duration, 44_100_000);
        assert_eq!(elst_of(&v0).full_box_version(), 0);

        // 換算後の値が u32::MAX を超える場合は version 1
        let v1 = mux_audio_with_edit_list(edit_list(
            1,
            44100,
            vec![ElstEntry {
                edit_duration: 100_000,
                media_time: 0,
                media_rate: one_rate(),
            }],
        ))
        .expect("編集リスト付きの mux に失敗した");
        assert_eq!(elst_of(&v1).entries[0].edit_duration, 4_410_000_000);
        assert_eq!(elst_of(&v1).full_box_version(), 1);
    }

    /// 複数エントリーの合計が `u64` を超える場合は拒否されること
    #[test]
    fn edit_list_total_duration_overflow_is_rejected() {
        let error = mux_audio_with_edit_list(edit_list(
            44100,
            44100,
            vec![
                ElstEntry {
                    edit_duration: u64::MAX,
                    media_time: 0,
                    media_rate: one_rate(),
                },
                ElstEntry {
                    edit_duration: u64::MAX,
                    media_time: 0,
                    media_rate: one_rate(),
                },
            ],
        ))
        .expect_err("合計の桁溢れは拒否される");
        assert!(
            matches!(error, MuxError::EncodeError(_)),
            "unexpected error: {error:?}"
        );
        assert!(
            error.to_string().contains("edit list total duration"),
            "unexpected error message: {error}"
        );
    }

    /// 編集リスト付き音声と編集リストなし映像を mux する
    fn mux_audio_and_video(video_duration: u32) -> FinalizedBoxes {
        let options = Mp4FileMuxerOptions {
            audio_track: TrackMetadata {
                edit_list: Some(edit_list(
                    1000,
                    44100,
                    vec![ElstEntry {
                        edit_duration: 1000,
                        media_time: 1024,
                        media_rate: one_rate(),
                    }],
                )),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_sample = Sample {
            encryption_info: None,
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(320, 240)),
            keyframe: true,
            timescale: NonZeroU32::new(12800).expect("12800 は非ゼロ"),
            duration: video_duration,
            composition_time_offset: None,
            data_offset,
            data_size: 128,
        };
        muxer
            .append_sample(&video_sample)
            .expect("video sample の追加に失敗した");
        append_audio_sample(&mut muxer, data_offset + 128);
        muxer.finalize().cloned().expect("finalize に失敗した")
    }

    /// 編集リスト付きトラックが最長の場合、その timescale が `mvhd` に採用されること
    #[test]
    fn edit_list_track_can_be_the_longest_track() {
        // 音声 1.0 秒（編集リスト）、映像 0.5 秒（6400 / 12800）
        let finalized = mux_audio_and_video(6400);
        let moov_box = finalized.moov_box();
        assert_eq!(moov_box.mvhd_box.timescale.get(), 44100);
        assert_eq!(moov_box.mvhd_box.duration, 44100);
        assert_eq!(moov_box.trak_boxes[0].tkhd_box.duration, 22050);
        assert_eq!(moov_box.trak_boxes[1].tkhd_box.duration, 44100);
    }

    /// エントリーごとの丸めで最長が反転しても `mvhd.duration` が最大 `tkhd` と一致すること
    #[test]
    fn mvhd_duration_is_max_of_tkhd_durations() {
        // 生の合計では音声 (200222 / 10^7 秒) が映像 (200136 / 10^7 秒) より長いが、
        // 出力 movie timescale への換算で音声 882 tick、映像 883 tick になる
        let audio_edit_list = edit_list(
            10_000_000,
            44100,
            vec![
                ElstEntry {
                    edit_duration: 100_111,
                    media_time: 0,
                    media_rate: one_rate(),
                },
                ElstEntry {
                    edit_duration: 100_111,
                    media_time: 0,
                    media_rate: one_rate(),
                },
            ],
        );
        let video_edit_list = edit_list(
            10_000_000,
            44100,
            vec![ElstEntry {
                edit_duration: 200_136,
                media_time: 0,
                media_rate: one_rate(),
            }],
        );
        let options = Mp4FileMuxerOptions {
            audio_track: TrackMetadata {
                edit_list: Some(audio_edit_list),
                ..Default::default()
            },
            video_track: TrackMetadata {
                edit_list: Some(video_edit_list),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("muxer の作成に失敗した");
        let data_offset = muxer.initial_boxes_bytes().len() as u64;
        let video_sample = Sample {
            encryption_info: None,
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(320, 240)),
            keyframe: true,
            timescale: NonZeroU32::new(44100).expect("44100 は非ゼロ"),
            duration: 45124,
            composition_time_offset: None,
            data_offset,
            data_size: 128,
        };
        muxer
            .append_sample(&video_sample)
            .expect("video sample の追加に失敗した");
        append_audio_sample(&mut muxer, data_offset + 128);
        let finalized = muxer.finalize().cloned().expect("finalize に失敗した");

        let moov_box = finalized.moov_box();
        assert_eq!(moov_box.mvhd_box.timescale.get(), 44100);
        assert_eq!(moov_box.trak_boxes[0].tkhd_box.duration, 883, "映像の tkhd");
        assert_eq!(moov_box.trak_boxes[1].tkhd_box.duration, 882, "音声の tkhd");
        assert_eq!(
            moov_box.mvhd_box.duration,
            moov_box
                .trak_boxes
                .iter()
                .map(|trak_box| trak_box.tkhd_box.duration)
                .max()
                .unwrap_or(0),
            "mvhd.duration が最大 tkhd.duration と一致しない"
        );
        assert_eq!(moov_box.mvhd_box.duration, 883);
    }

    /// 編集リストなしトラックが最長の場合も `mvhd` と各 `tkhd` が正しいこと
    #[test]
    fn edit_list_track_can_be_shorter_than_another_track() {
        // 音声 1.0 秒（編集リスト）、映像 2.0 秒（25600 / 12800）
        let finalized = mux_audio_and_video(25600);
        let moov_box = finalized.moov_box();
        assert_eq!(moov_box.mvhd_box.timescale.get(), 12800);
        assert_eq!(moov_box.mvhd_box.duration, 25600);
        assert_eq!(moov_box.trak_boxes[0].tkhd_box.duration, 25600);
        assert_eq!(moov_box.trak_boxes[1].tkhd_box.duration, 12800);
        let elst_box = moov_box.trak_boxes[1]
            .edts_box
            .as_ref()
            .expect("edts が出力されていない")
            .elst_box
            .as_ref()
            .expect("elst が出力されていない");
        assert_eq!(elst_box.entries[0].edit_duration, 12800);
    }
}
