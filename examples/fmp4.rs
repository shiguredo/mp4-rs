//! fMP4 (Fragmented MP4) のマルチプレックスとデマルチプレックスのサンプル
//!
//! 映像トラック（AVC/H.264）と音声トラック（Opus）を含む fMP4 を生成し、
//! 複数のメディアセグメントに分けて出力したあと、それを読み戻すことで
//! データが正しく保持されていることを確認する。
//!
//! # 実行方法
//!
//! ```bash
//! cargo run --example fmp4
//! ```
use std::num::NonZeroU32;

use shiguredo_mp4::{
    FixedPointNumber, TrackKind, Uint,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, OpusBox, SampleEntry,
        VisualSampleEntryFields,
    },
    demux::Fmp4SegmentDemuxer,
    mux::{Fmp4SegmentMuxer, Sample},
};

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
            sps_list: vec![vec![0x67, 0x42, 0xc0, 0x1e]],
            pps_list: vec![vec![0x68, 0xce, 0x38, 0x80]],
            chroma_format: None,
            bit_depth_luma_minus8: None,
            bit_depth_chroma_minus8: None,
            sps_ext_list: vec![],
        },
        unknown_boxes: vec![],
    })
}

fn create_opus_sample_entry() -> SampleEntry {
    SampleEntry::Opus(OpusBox {
        audio: AudioSampleEntryFields {
            data_reference_index: AudioSampleEntryFields::DEFAULT_DATA_REFERENCE_INDEX,
            channelcount: 2,
            samplesize: AudioSampleEntryFields::DEFAULT_SAMPLESIZE,
            samplerate: FixedPointNumber::new(48000u16, 0),
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

/// ダミーの映像フレームデータを生成する（実際には H.264 ビットストリームが入る）
fn dummy_video_frame(keyframe: bool, size: usize) -> Vec<u8> {
    // NAL ユニット長プレフィックス (4 バイト) + ダミーデータ
    let nal_size = size.saturating_sub(4);
    let mut data = vec![0u8; size];
    // NAL unit length
    let len = u32::try_from(nal_size).expect("NAL unit size exceeds u32::MAX");
    data[0] = (len >> 24) as u8;
    data[1] = (len >> 16) as u8;
    data[2] = (len >> 8) as u8;
    data[3] = len as u8;
    // NAL ユニット種別バイト: IDR=0x65, non-IDR=0x41
    if size > 4 {
        data[4] = if keyframe { 0x65 } else { 0x41 };
    }
    data
}

/// ダミーの音声フレームデータを生成する（実際には Opus パケットが入る）
fn dummy_audio_frame(size: usize) -> Vec<u8> {
    vec![0u8; size]
}

fn build_segment(samples: &[Sample], segment_metadata: &[u8], payloads: &[&[u8]]) -> Vec<u8> {
    let mut segment = segment_metadata.to_vec();
    let payload_size: usize = samples.iter().map(|sample| sample.data_size).sum();
    segment.reserve(payload_size);
    for payload in payloads {
        segment.extend_from_slice(payload);
    }
    segment
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width: u16 = 1280;
    let height: u16 = 720;
    let video_timescale = NonZeroU32::new(90000).expect("non-zero");
    let audio_timescale = NonZeroU32::new(48000).expect("non-zero");

    let video_sample_entry = create_avc1_sample_entry(width, height);
    let audio_sample_entry = create_opus_sample_entry();

    // Muxer を生成し、最初のメディアセグメント作成を通じてトラック情報を学習させる
    let mut muxer = Fmp4SegmentMuxer::new()?;

    // 3 つのメディアセグメントを生成する
    let video_frame_duration = 3000u32; // 90000 / 30fps = 3000
    let audio_frame_duration = 960u32; // 48000 Hz で 20ms = 960

    let segments: Vec<Vec<u8>> = (0..3)
        .map(|seg_idx| {
            let video_data = dummy_video_frame(seg_idx == 0, 2048);
            let audio_data = dummy_audio_frame(256);

            let samples = vec![
                Sample {
                    track_kind: TrackKind::Video,
                    timescale: video_timescale,
                    sample_entry: Some(video_sample_entry.clone()),
                    duration: video_frame_duration,
                    keyframe: seg_idx == 0,
                    composition_time_offset: None,
                    data_offset: 0,
                    data_size: video_data.len(),
                },
                Sample {
                    track_kind: TrackKind::Audio,
                    timescale: audio_timescale,
                    sample_entry: Some(audio_sample_entry.clone()),
                    duration: audio_frame_duration,
                    keyframe: true,
                    composition_time_offset: None,
                    data_offset: video_data.len() as u64,
                    data_size: audio_data.len(),
                },
            ];

            let segment_metadata = muxer
                .create_media_segment(&samples)
                .expect("メディアセグメント生成に失敗");
            let segment = build_segment(&samples, &segment_metadata, &[&video_data, &audio_data]);

            println!(
                "メディアセグメント {}: {} バイト",
                seg_idx + 1,
                segment.len()
            );
            segment
        })
        .collect();

    let init_segment = muxer.init_segment_bytes()?;
    println!("初期化セグメント: {} バイト", init_segment.len());

    // sidx 付きセグメントも生成してみる
    let video_data = dummy_video_frame(false, 1024);
    let audio_data = dummy_audio_frame(128);
    let sidx_samples = vec![
        Sample {
            track_kind: TrackKind::Video,
            timescale: video_timescale,
            sample_entry: Some(video_sample_entry.clone()),
            duration: video_frame_duration,
            keyframe: false,
            composition_time_offset: None,
            data_offset: 0,
            data_size: video_data.len(),
        },
        Sample {
            track_kind: TrackKind::Audio,
            timescale: audio_timescale,
            sample_entry: Some(audio_sample_entry.clone()),
            duration: audio_frame_duration,
            keyframe: true,
            composition_time_offset: None,
            data_offset: video_data.len() as u64,
            data_size: audio_data.len(),
        },
    ];
    let sidx_metadata = muxer.create_media_segment_with_sidx(&sidx_samples)?;
    let sidx_segment = build_segment(&sidx_samples, &sidx_metadata, &[&video_data, &audio_data]);
    println!("sidx 付きセグメント: {} バイト", sidx_segment.len());

    // Demuxer で初期化セグメントを処理する
    let mut demuxer = Fmp4SegmentDemuxer::new();
    demuxer.handle_init_segment(&init_segment)?;

    let tracks = demuxer.tracks()?;
    println!("\nトラック数: {}", tracks.len());
    for track in tracks {
        println!(
            "  track_id={}, kind={:?}, timescale={}",
            track.track_id, track.kind, track.timescale
        );
    }

    // メディアセグメントを順番に処理する
    println!("\nサンプル情報:");
    for (i, segment) in segments.iter().enumerate() {
        let demuxed = demuxer.handle_media_segment(segment)?;
        println!("  セグメント {}:", i + 1);
        for sample in &demuxed {
            println!(
                "    track_id={}, timestamp={}, duration={}, keyframe={}, size={}",
                sample.track.track_id,
                sample.timestamp,
                sample.duration,
                sample.keyframe,
                sample.data_size
            );
        }
    }

    // sidx 付きセグメントも処理する（sidx は自動的にスキップされる）
    let sidx_demuxed = demuxer.handle_media_segment(&sidx_segment)?;
    println!("  sidx 付きセグメント:");
    for sample in &sidx_demuxed {
        println!(
            "    track_id={}, timestamp={}, duration={}, size={}",
            sample.track.track_id, sample.timestamp, sample.duration, sample.data_size
        );
    }

    // mfra (Movie Fragment Random Access) ボックスを生成してファイル末尾に付加する
    //
    // mfra はファイルをシークする際のランダムアクセスインデックスとして機能する。
    // 実際のファイルに書き出す場合は、全セグメントの後ろに追記する。
    let mfra = muxer.mfra_bytes()?;
    println!("\nmfra ボックス: {} バイト", mfra.len());

    // mfro ボックスの末尾 4 バイトが mfra 全体のサイズと一致することを確認する
    // (ISO 14496-12 Section 8.8.11: mfro.size は mfra ボックス全体のバイト数)
    let mfro_size = u32::from_be_bytes(
        mfra[mfra.len() - 4..]
            .try_into()
            .expect("mfra は末尾 4 バイトに mfro.size を持つ"),
    );
    assert_eq!(
        mfro_size as usize, // u32 -> usize: 常に安全
        mfra.len(),
        "mfro.size が mfra サイズと一致しない"
    );
    println!("mfro.size = {mfro_size} (mfra 全体サイズと一致)");

    println!("\nOK: fMP4 の mux/demux が正常に完了しました");
    Ok(())
}
