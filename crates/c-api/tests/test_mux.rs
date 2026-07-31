//! C API の `mp4_estimate_maximum_moov_box_size` に関する統合テスト
//!
//! 主な検証内容:
//! - 配列 + 長さの新シグネチャが任意トラック数を受け付け、Rust 側の見積もり式と一致すること
//! - `sample_counts` が NULL のときは `0` を返すこと（誤用扱い）
//! - 空配列（NULL でなく長さ 0）のときは基本オーバーヘッド相当を返すこと
//! - 映像・音声・字幕の 3 トラック構成で、見積もり値を予約サイズに使うと faststart が有効になること

use std::num::{NonZeroU16, NonZeroU32};
use std::ptr::null;

use mp4::mux::mp4_estimate_maximum_moov_box_size;
use shiguredo_mp4::{
    FixedPointNumber, TrackKind, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry, StppBox,
        VisualSampleEntryFields,
    },
    mux::{Mp4FileMuxer, Mp4FileMuxerOptions, Sample},
};

/// 見積もり式の基本オーバーヘッド（`estimate_maximum_moov_box_size` と揃える）
const BASE_MOOV_OVERHEAD: u32 = 512;
/// トラックあたりのオーバーヘッド
const PER_TRACK_OVERHEAD: u32 = 1024;
/// サンプルあたりの概算バイト数
const BYTES_PER_SAMPLE: u32 = 16;

/// 指定解像度の `SampleEntry::Avc1` を組み立てる
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
            avc_profile_indication: 66,
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

/// 最小限の Opus SampleEntry を組み立てる
fn create_opus_sample_entry() -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: NonZeroU16::new(1).expect("data_reference_index は非ゼロ"),
            channelcount: 2,
            samplesize: 16,
            samplerate: FixedPointNumber::new(48000, 0),
        },
        dops_box: DopsBox {
            output_channel_count: 2,
            pre_skip: 312,
            input_sample_rate: 48000,
            output_gain: 0,
        },
        unknown_boxes: vec![],
    })
}

/// 最小限の stpp SampleEntry を組み立てる
fn create_stpp_sample_entry() -> SampleEntry {
    SampleEntry::Stpp(StppBox {
        data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
        namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
        schema_location: Utf8String::EMPTY,
        auxiliary_mime_types: Utf8String::EMPTY,
        unknown_boxes: vec![],
    })
}

/// 見積もり式と同じ計算結果を返す（テスト期待値用）
fn expected_estimate(sample_counts: &[u32]) -> u32 {
    BASE_MOOV_OVERHEAD
        + (sample_counts.len() as u32) * PER_TRACK_OVERHEAD
        + sample_counts.iter().sum::<u32>() * BYTES_PER_SAMPLE
}

/// `sample_counts` が NULL のときは長さによらず `0` を返す
#[test]
fn estimate_returns_zero_for_null_pointer() {
    // NULL 判定を長さ判定より先に行う契約のため、len > 0 でも 0 を返すこと
    let result = unsafe { mp4_estimate_maximum_moov_box_size(null(), 3) };
    assert_eq!(result, 0, "NULL 引数では 0 を返すこと");

    let result = unsafe { mp4_estimate_maximum_moov_box_size(null(), 0) };
    assert_eq!(result, 0, "NULL かつ長さ 0 でも 0 を返すこと");
}

/// NULL でなく長さ 0 のときは基本オーバーヘッド相当を返す
#[test]
fn estimate_returns_base_overhead_for_empty_slice() {
    let empty: [u32; 0] = [];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(empty.as_ptr(), 0) };
    assert_eq!(
        result, BASE_MOOV_OVERHEAD,
        "空スライスでは BASE_MOOV_OVERHEAD を返すこと"
    );
}

/// 1 トラック・2 トラック・3 トラックの見積もりが式どおりであること
#[test]
fn estimate_matches_formula_for_various_track_counts() {
    // 映像のみ（旧 2 引数版の video 相当）
    let one_track = [5u32];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(one_track.as_ptr(), 1) };
    assert_eq!(result, expected_estimate(&one_track));

    // 音声 + 映像（旧 2 引数版相当）
    let two_tracks = [100u32, 3000];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(two_tracks.as_ptr(), 2) };
    assert_eq!(result, expected_estimate(&two_tracks));

    // 音声 + 映像 + 字幕
    let three_tracks = [1000u32, 3000, 100];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(three_tracks.as_ptr(), 3) };
    assert_eq!(result, expected_estimate(&three_tracks));
}

/// 実測ケースの 3 トラック見積もり値が期待どおりであること
#[test]
fn estimate_matches_measured_three_track_cases() {
    // v=10 a=10 s=100 → 5504
    let counts = [10u32, 10, 100];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(counts.as_ptr(), 3) };
    assert_eq!(result, 5504);

    // v=50 a=50 s=300 → 9984
    let counts = [50u32, 50, 300];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(counts.as_ptr(), 3) };
    assert_eq!(result, 9984);

    // v=1 a=1 s=1000 → 19616
    let counts = [1u32, 1, 1000];
    let result = unsafe { mp4_estimate_maximum_moov_box_size(counts.as_ptr(), 3) };
    assert_eq!(result, 19616);
}

/// 映像・音声・字幕を交互に追加した構成で、3 トラック見積もりにより faststart が有効になること
///
/// 実測で 2 トラック見積もりでは faststart が無効になったケースを対象にする。
#[test]
fn estimate_enables_faststart_for_interleaved_three_tracks() {
    let cases: &[(u32, u32, u32)] = &[(10, 10, 100), (50, 50, 300), (1, 1, 1000)];

    for &(video_count, audio_count, subtitle_count) in cases {
        let counts = [video_count, audio_count, subtitle_count];
        let estimated = unsafe { mp4_estimate_maximum_moov_box_size(counts.as_ptr(), 3) };
        assert!(
            estimated > 0,
            "見積もりは正の値であること (v={video_count} a={audio_count} s={subtitle_count})"
        );

        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: estimated as usize,
            ..Default::default()
        };
        let mut muxer = Mp4FileMuxer::with_options(options).expect("ミューサの作成に失敗した");
        let mut offset = muxer.initial_boxes_bytes().len() as u64;

        // `Mp4FileMuxer::append_sample` は最初のサンプルにだけ `sample_entry` を要求し、
        // 以降は `None` を渡す慣用に従うため、`Option::take` で 2 回目以降は自動的に `None` になるようにする
        let mut video_entry = Some(create_avc1_sample_entry(1920, 1080));
        let mut audio_entry = Some(create_opus_sample_entry());
        let mut subtitle_entry = Some(create_stpp_sample_entry());
        let mut remaining_video = video_count;
        let mut remaining_audio = audio_count;
        let mut remaining_subtitle = subtitle_count;

        // トラックを 1 本ずつ交互に追加し、チャンクが細かく分かれる構成を再現する
        while remaining_video > 0 || remaining_audio > 0 || remaining_subtitle > 0 {
            if remaining_video > 0 {
                let data_size = 100usize;
                muxer
                    .append_sample(&Sample {
                        track_kind: TrackKind::Video,
                        sample_entry: video_entry.take(),
                        keyframe: true,
                        timescale: NonZeroU32::new(30).expect("タイムスケールは非ゼロ"),
                        duration: 1,
                        composition_time_offset: None,
                        data_offset: offset,
                        data_size,
                    })
                    .expect("映像サンプルの追加に失敗した");
                offset += data_size as u64;
                remaining_video -= 1;
            }
            if remaining_audio > 0 {
                let data_size = 50usize;
                muxer
                    .append_sample(&Sample {
                        track_kind: TrackKind::Audio,
                        sample_entry: audio_entry.take(),
                        keyframe: false,
                        timescale: NonZeroU32::new(48000).expect("タイムスケールは非ゼロ"),
                        duration: 960,
                        composition_time_offset: None,
                        data_offset: offset,
                        data_size,
                    })
                    .expect("音声サンプルの追加に失敗した");
                offset += data_size as u64;
                remaining_audio -= 1;
            }
            if remaining_subtitle > 0 {
                let data_size = 32usize;
                muxer
                    .append_sample(&Sample {
                        track_kind: TrackKind::Subtitle,
                        sample_entry: subtitle_entry.take(),
                        keyframe: true,
                        timescale: NonZeroU32::new(1000).expect("タイムスケールは非ゼロ"),
                        duration: 500,
                        composition_time_offset: None,
                        data_offset: offset,
                        data_size,
                    })
                    .expect("字幕サンプルの追加に失敗した");
                offset += data_size as u64;
                remaining_subtitle -= 1;
            }
        }

        let finalized = muxer.finalize().expect("finalize に失敗した");
        assert!(
            finalized.is_faststart_enabled(),
            "3 トラック見積もりで faststart が有効になること \
             (v={video_count} a={audio_count} s={subtitle_count}, \
             estimated={estimated}, actual_moov={})",
            finalized.moov_box_size()
        );
        assert!(
            estimated as usize >= finalized.moov_box_size(),
            "見積もりは実 moov サイズ以上であること \
             (estimated={estimated}, actual={})",
            finalized.moov_box_size()
        );
    }
}
