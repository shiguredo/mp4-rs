//! C API の `fmp4_segment_demuxer_handle_media_segment` に関する統合テスト
//!
//! 主な検証内容:
//! - C API が変換できないサンプルエントリーを含むメディアセグメントは `MP4_ERROR_UNSUPPORTED` になること
//! - そのエラーを返した呼び出しの前後で、内部の `Fmp4SegmentDemuxer` の状態が変わらないこと。
//!   エラーの後に、C API が対応しているトラックの `traf` だけを含むメディアセグメントを渡すと、
//!   そのトラックの最初のサンプルに `sample_entry` が付くこと

use std::num::NonZeroU32;
use std::ptr::null_mut;

use mp4::demux::Mp4DemuxSample;
use mp4::error::Mp4Error;
use mp4::fmp4_segment_demux::{
    fmp4_segment_demuxer_free, fmp4_segment_demuxer_free_samples,
    fmp4_segment_demuxer_handle_init_segment, fmp4_segment_demuxer_handle_media_segment,
    fmp4_segment_demuxer_new,
};
use shiguredo_mp4::{
    BoxSize, BoxType, TrackKind, Uint,
    boxes::{Avc1Box, AvccBox, SampleEntry, UnknownBox, VisualSampleEntryFields},
    mux::{Fmp4SegmentMuxer, Sample},
};

/// 映像トラックの timescale
const VIDEO_TIMESCALE: u32 = 90_000;

/// 音声トラックの timescale
const AUDIO_TIMESCALE: u32 = 48_000;

/// テスト用の `avc1` サンプルエントリーを組み立てる
fn create_avc1_sample_entry() -> SampleEntry {
    SampleEntry::Avc1(Avc1Box {
        visual: VisualSampleEntryFields {
            data_reference_index: VisualSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            width: 320,
            height: 240,
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

/// C API が変換できないサンプルエントリー（`ac-3`）を組み立てる
///
/// `ac-3` は `Mp4SampleEntryOwned` が対応しておらず、`MP4_ERROR_UNSUPPORTED` の経路に入る。
/// payload は空なので、ボックスのサイズはヘッダーの 8 バイトだけになる
fn create_unsupported_sample_entry() -> SampleEntry {
    SampleEntry::Unknown(UnknownBox {
        box_type: BoxType::Normal(*b"ac-3"),
        box_size: BoxSize::U32(8),
        payload: Vec::new(),
    })
}

/// サンプル 1 個分を組み立てる
fn create_sample(
    track_kind: TrackKind,
    sample_entry: &SampleEntry,
    timescale: u32,
    duration: u32,
) -> Sample {
    Sample {
        track_kind,
        timescale: NonZeroU32::new(timescale).expect("timescale は非ゼロである"),
        sample_entry: Some(sample_entry.clone()),
        duration,
        keyframe: true,
        composition_time_offset: None,
        data_offset: 0,
        data_size: 4,
    }
}

/// トラックごとにまとめた順にサンプルを並べ、`data_offset` と `data_size` を設定して
/// メディアセグメントを組み立てる
fn build_segment(muxer: &mut Fmp4SegmentMuxer, samples: &[Sample]) -> Vec<u8> {
    let mut ordered_kinds = Vec::new();
    for sample in samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut arranged_samples = samples.to_vec();
    let mut payload_bytes = Vec::new();
    for track_kind in ordered_kinds {
        for sample in arranged_samples.iter_mut() {
            if sample.track_kind != track_kind {
                continue;
            }
            sample.data_offset = payload_bytes.len() as u64;
            payload_bytes.extend_from_slice(&vec![0u8; sample.data_size]);
        }
    }

    let mut segment = muxer
        .create_media_segment_metadata(&arranged_samples)
        .expect("メディアセグメントの作成に失敗した");
    segment.extend_from_slice(&payload_bytes);
    segment
}

/// init セグメントと、メディアセグメントを 2 つ組み立てる
///
/// 1 つ目のメディアセグメントは映像と音声の 2 トラックを含み、
/// 2 つ目のメディアセグメントは映像トラックだけを含む
fn build_segments() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let video_sample_entry = create_avc1_sample_entry();
    let unsupported_sample_entry = create_unsupported_sample_entry();
    let video_sample = create_sample(
        TrackKind::Video,
        &video_sample_entry,
        VIDEO_TIMESCALE,
        3_000,
    );
    let unsupported_sample = create_sample(
        TrackKind::Audio,
        &unsupported_sample_entry,
        AUDIO_TIMESCALE,
        1_024,
    );

    let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

    // 1 つ目のメディアセグメント（映像 + 音声）
    let first_segment = build_segment(&mut muxer, &[video_sample.clone(), unsupported_sample]);

    // 2 つ目のメディアセグメント（映像のみ）
    let second_segment = build_segment(&mut muxer, &[video_sample]);

    let init_segment = muxer
        .init_segment_bytes()
        .expect("init セグメントの作成に失敗した");

    (init_segment, first_segment, second_segment)
}

/// C API が変換できないサンプルエントリーを含むメディアセグメントは
/// `MP4_ERROR_UNSUPPORTED` になり、その呼び出しの前後で内部の状態が変わらないこと
#[test]
fn handle_media_segment_with_unsupported_sample_entry_keeps_state() {
    let (init_segment, first_segment, second_segment) = build_segments();

    let demuxer = fmp4_segment_demuxer_new();
    assert!(!demuxer.is_null(), "インスタンスの生成に失敗した");

    let error = unsafe {
        fmp4_segment_demuxer_handle_init_segment(
            demuxer,
            init_segment.as_ptr(),
            init_segment.len() as u32,
        )
    };
    assert_eq!(
        error,
        Mp4Error::MP4_ERROR_OK,
        "init セグメントの処理に失敗した"
    );

    // 1 つ目のメディアセグメントは、変換できないサンプルエントリーがあるためエラーになる
    let mut samples: *mut Mp4DemuxSample = null_mut();
    let mut count = 0u32;
    let error = unsafe {
        fmp4_segment_demuxer_handle_media_segment(
            demuxer,
            first_segment.as_ptr(),
            first_segment.len() as u32,
            &mut samples,
            &mut count,
        )
    };
    assert_eq!(
        error,
        Mp4Error::MP4_ERROR_UNSUPPORTED,
        "変換できないサンプルエントリーを含むメディアセグメントは MP4_ERROR_UNSUPPORTED になる"
    );
    assert!(
        samples.is_null(),
        "エラーを返すときは samples が NULL である"
    );
    assert_eq!(count, 0, "エラーを返すときは count が 0 である");

    // 2 つ目のメディアセグメントは、対応しているトラックだけを含むので成功する。
    // 内部の状態が戻っていれば、このトラックの最初のサンプルに sample_entry が付く
    let error = unsafe {
        fmp4_segment_demuxer_handle_media_segment(
            demuxer,
            second_segment.as_ptr(),
            second_segment.len() as u32,
            &mut samples,
            &mut count,
        )
    };
    assert_eq!(
        error,
        Mp4Error::MP4_ERROR_OK,
        "対応しているトラックだけのメディアセグメントは成功する"
    );
    let samples = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    // 2 つ目のメディアセグメントは映像トラックだけを含むので、返るのは映像のサンプル 1 つである。
    // 内部の状態が戻っていれば、このトラックの最初のサンプルなので `sample_entry` が付く
    assert_eq!(samples.len(), 1, "映像トラックのサンプルが 1 つ返る");
    assert!(
        !samples[0].sample_entry.is_null(),
        "エラーを返した呼び出しの後に、対応しているトラックの最初のサンプルに sample_entry が付く"
    );

    unsafe {
        fmp4_segment_demuxer_free_samples(samples.as_ptr() as *mut Mp4DemuxSample, count);
        fmp4_segment_demuxer_free(demuxer);
    }
}
