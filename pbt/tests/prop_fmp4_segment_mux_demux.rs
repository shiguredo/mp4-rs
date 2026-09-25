//! fMP4 Mux → Demux Roundtrip の Property-Based Testing
//!
//! Fmp4SegmentMuxer で生成した初期化セグメントとメディアセグメントを
//! Fmp4SegmentDemuxer で解析し、元のデータと一致することを確認するテスト

use std::num::NonZeroU32;

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, FixedPointNumber, TrackKind, Uint, Utf8String,
    boxes::{
        AudioSampleEntryFields, Avc1Box, AvccBox, DopsBox, FtypBox, MfraBox, MoofBox, MoovBox,
        OpusBox, SampleEntry, SidxBox, StppBox, TfdtBox, VisualSampleEntryFields,
    },
    demux::{DemuxError, Fmp4FileDemuxer, Fmp4SegmentDemuxer, Input, TrackInfo},
    mux::{Fmp4SegmentMuxer, Sample, SegmentMuxerOptions},
};

mod helpers;

const VIDEO_TIMESCALE: u32 = 90_000;
const AUDIO_TIMESCALE: u32 = 48_000;
const SUBTITLE_TIMESCALE: u32 = 1_000;

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

/// テスト用の Stpp（TTML）SampleEntry を作成
fn create_stpp_sample_entry() -> SampleEntry {
    SampleEntry::Stpp(StppBox {
        data_reference_index: StppBox::DEFAULT_DATA_REFERENCE_INDEX,
        namespace: Utf8String::new("http://www.w3.org/ns/ttml").expect("null 文字を含まない"),
        schema_location: Utf8String::EMPTY,
        auxiliary_mime_types: Utf8String::EMPTY,
        unknown_boxes: vec![],
    })
}

/// サンプルデータを表す補助構造体
#[derive(Debug, Clone)]
struct TestSample {
    track_index: usize,
    duration: u32,
    keyframe: bool,
    data: Vec<u8>,
}

fn arb_video_sample(ctx: &mut TestCaseContext, track_index: usize) -> TestSample {
    let duration = noprop::sample_u64_in(ctx, 1..3001) as u32;
    let keyframe = noprop::sample_bool(ctx);
    let data_len = noprop::sample_usize_in(ctx, 1..256);
    let data = noprop::sample_bytes_vec(ctx, data_len);
    TestSample {
        track_index,
        duration,
        keyframe,
        data,
    }
}

fn arb_video_sample_with_cto(
    ctx: &mut TestCaseContext,
    track_index: usize,
) -> (TestSample, Option<i64>) {
    let sample = arb_video_sample(ctx, track_index);
    // 1/2 選択で Some/None を決める（旧 `prop::option::of` と同じ確率）
    let cto = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u64_in(ctx, 0..6001) as i64 - 3000)
    } else {
        None
    };
    (sample, cto)
}

fn arb_audio_sample(ctx: &mut TestCaseContext, track_index: usize) -> TestSample {
    let duration = noprop::sample_u64_in(ctx, 1..1921) as u32;
    let data_len = noprop::sample_usize_in(ctx, 1..128);
    let data = noprop::sample_bytes_vec(ctx, data_len);
    TestSample {
        track_index,
        duration,
        keyframe: true,
        data,
    }
}

/// noprop の `sample_usize_in` で長さを引いてから要素を生成するベクタサンプラー
fn sample_vec<T>(
    ctx: &mut TestCaseContext,
    range: std::ops::Range<usize>,
    mut elem: impl FnMut(&mut TestCaseContext) -> T,
) -> Vec<T> {
    let len = noprop::sample_usize_in(ctx, range);
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(elem(ctx));
    }
    result
}

/// `[64, 1921)` の範囲で `width1` と衝突しない `u16` を引く
///
/// 「異なる sample entry を作りたい」という前提条件は draw 時点で決まるため、
/// noprop skill の指針に従い、ケース全体を捨てる `ctx.reject_case()` ではなく
/// 局所リトライの `sample_with_rejection` を使う。値域は 1857 通りあるので
/// 最大 4 回のリトライで実質確実に別の値を得られる
fn sample_distinct_width(ctx: &mut TestCaseContext, width1: u16) -> u16 {
    noprop::sample_with_rejection(ctx, 4, |ctx| {
        let w = noprop::sample_u64_in(ctx, 64..1921) as u16;
        (w != width1).then_some(w)
    })
}

fn video_segment_sample(
    sample_entry: &SampleEntry,
    sample: &TestSample,
    composition_time_offset: Option<i64>,
) -> Sample {
    Sample {
        track_kind: TrackKind::Video,
        timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("非ゼロである"),
        sample_entry: Some(sample_entry.clone()),
        duration: sample.duration,
        keyframe: sample.keyframe,
        composition_time_offset,
        data_offset: 0,
        data_size: sample.data.len(),
    }
}

fn audio_segment_sample(sample_entry: &SampleEntry, sample: &TestSample) -> Sample {
    Sample {
        track_kind: TrackKind::Audio,
        timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("非ゼロである"),
        sample_entry: Some(sample_entry.clone()),
        duration: sample.duration,
        keyframe: sample.keyframe,
        composition_time_offset: None,
        data_offset: 0,
        data_size: sample.data.len(),
    }
}

fn subtitle_segment_sample(sample_entry: &SampleEntry, sample: &TestSample) -> Sample {
    Sample {
        track_kind: TrackKind::Subtitle,
        timescale: NonZeroU32::new(SUBTITLE_TIMESCALE).expect("非ゼロである"),
        sample_entry: Some(sample_entry.clone()),
        duration: sample.duration,
        keyframe: sample.keyframe,
        composition_time_offset: None,
        data_offset: 0,
        data_size: sample.data.len(),
    }
}

fn build_complete_media_segment(
    muxer: &mut Fmp4SegmentMuxer,
    samples: &[Sample],
    payloads: &[&[u8]],
) -> Vec<u8> {
    build_complete_media_segment_impl(muxer, samples, payloads, false)
}

fn build_complete_media_segment_with_sidx(
    muxer: &mut Fmp4SegmentMuxer,
    samples: &[Sample],
    payloads: &[&[u8]],
) -> Vec<u8> {
    build_complete_media_segment_impl(muxer, samples, payloads, true)
}

fn build_complete_media_segment_impl(
    muxer: &mut Fmp4SegmentMuxer,
    samples: &[Sample],
    payloads: &[&[u8]],
    with_sidx: bool,
) -> Vec<u8> {
    assert_eq!(
        samples.len(),
        payloads.len(),
        "samples と payloads の長さが一致しない"
    );

    let mut ordered_kinds = Vec::new();
    for sample in samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut arranged_samples = samples.to_vec();
    let mut payload_bytes = Vec::new();
    let mut next_offset = 0u64;
    for track_kind in ordered_kinds {
        for (index, payload) in payloads.iter().enumerate() {
            if arranged_samples[index].track_kind != track_kind {
                continue;
            }
            arranged_samples[index].data_offset = next_offset;
            arranged_samples[index].data_size = payload.len();
            next_offset = next_offset
                .checked_add(payload.len() as u64)
                .expect("payload サイズがオーバーフローした");
            payload_bytes.extend_from_slice(payload);
        }
    }

    let mut segment = if with_sidx {
        muxer.create_media_segment_metadata_with_sidx(&arranged_samples)
    } else {
        muxer.create_media_segment_metadata(&arranged_samples)
    }
    .expect("media セグメントの作成に失敗した");
    segment.extend_from_slice(&payload_bytes);
    segment
}

/// 入力の供給ループの回数の上限
///
/// `required_input()` が同じ範囲を要求し続ける回帰が入ると、ループが終わらずテストがハングする。
/// 正当なファイルの供給は `ftyp` + `moov` + `moof` + `mdat` で 10 回に満たないため、
/// 余裕を持たせた上限を設けてハングではなく失敗にする
const MAX_FEED_COUNT: usize = 64;

fn feed_fmp4_file_demuxer(demuxer: &mut Fmp4FileDemuxer, file_data: &[u8]) {
    let mut count = 0;
    while let Some(required) = demuxer.required_input() {
        let start = required.position as usize;
        let end = if let Some(required_size) = required.size {
            start.saturating_add(required_size).min(file_data.len())
        } else {
            file_data.len()
        };
        let data = file_data.get(start..end).unwrap_or(&[]);
        demuxer.handle_input(Input {
            position: required.position,
            data,
        });

        count += 1;
        assert!(
            count <= MAX_FEED_COUNT,
            "入力の供給が {MAX_FEED_COUNT} 回を超えた。required_input() が同じ範囲を要求し続けている"
        );
    }
}

/// `Fmp4FileDemuxer` から全サンプルを [`ComparableSample`] として取り出す
///
/// 入力が必要になったら、`file_data` から要求された範囲を渡して続ける
fn collect_comparable_samples(
    demuxer: &mut Fmp4FileDemuxer,
    file_data: &[u8],
) -> Vec<ComparableSample> {
    let mut samples = Vec::new();
    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => samples.push(to_comparable_sample(&sample)),
            Ok(None) => break,
            Err(DemuxError::InputRequired(_)) => feed_fmp4_file_demuxer(demuxer, file_data),
            Err(error) => panic!("next_sample エラー: {error}"),
        }
    }
    samples
}

/// 常に位置 0 から、渡せる範囲（切り詰めたファイル全体）を `handle_input` に渡す
///
/// 要求された範囲より多いデータを渡してもよいことと、ファイル全体を渡す場合も
/// `required_input()` が `Some` を返す間は繰り返し呼ぶ必要があることを確認するために使う
fn feed_fmp4_file_demuxer_with_whole_file(demuxer: &mut Fmp4FileDemuxer, file_data: &[u8]) {
    let mut count = 0;
    while demuxer.required_input().is_some() {
        demuxer.handle_input(Input {
            position: 0,
            data: file_data,
        });

        count += 1;
        assert!(
            count <= MAX_FEED_COUNT,
            "入力の供給が {MAX_FEED_COUNT} 回を超えた。required_input() が同じ範囲を要求し続けている"
        );
    }
}

/// [`Fmp4FileDemuxer`] の最終結果のうち、比較できる形にしたもの
///
/// [`DemuxError`] は `PartialEq` を実装しないため、`Debug` 表現の文字列で比較する
#[derive(Debug, PartialEq)]
enum ComparableFinalResult {
    /// これ以上サンプルがない（`next_sample()` が `Ok(None)` を返した）
    EndOfSamples,
    /// エラーになった
    Error(String),
}

/// `feed` でデータを供給しながら、取り出せるサンプル列と最後の `next_sample()` の結果を返す
fn collect_demux_result(
    file_data: &[u8],
    feed: impl Fn(&mut Fmp4FileDemuxer, &[u8]),
) -> (Vec<ComparableSample>, ComparableFinalResult) {
    let mut demuxer = Fmp4FileDemuxer::new();
    let mut samples = Vec::new();
    loop {
        match demuxer.next_sample() {
            Ok(Some(sample)) => samples.push(to_comparable_sample(&sample)),
            Ok(None) => return (samples, ComparableFinalResult::EndOfSamples),
            Err(DemuxError::InputRequired(_)) => feed(&mut demuxer, file_data),
            Err(error) => return (samples, ComparableFinalResult::Error(format!("{error:?}"))),
        }
    }
}

fn rewrite_init_segment(init_segment: &[u8], f: impl FnOnce(&mut MoovBox)) -> Vec<u8> {
    let (ftyp_box, ftyp_box_size) =
        FtypBox::decode(init_segment).expect("init セグメントからの ftyp デコードに失敗した");
    let (mut moov_box, moov_box_size) = MoovBox::decode(&init_segment[ftyp_box_size..])
        .expect("init セグメントからの moov デコードに失敗した");
    assert_eq!(
        ftyp_box_size + moov_box_size,
        init_segment.len(),
        "このテストでは init セグメントは ftyp + moov のみを含む"
    );
    f(&mut moov_box);

    let mut rewritten = ftyp_box
        .encode_to_vec()
        .expect("init セグメント書き換え中の ftyp エンコードに失敗した");
    let moov_bytes = moov_box
        .encode_to_vec()
        .expect("init セグメント書き換え中の moov エンコードに失敗した");
    rewritten.extend_from_slice(&moov_bytes);
    rewritten
}

fn append_sample_entry_and_set_trex_default(
    init_segment: &[u8],
    sample_entry: SampleEntry,
    default_sample_description_index: u32,
) -> Vec<u8> {
    rewrite_init_segment(init_segment, move |moov_box| {
        let track_id = moov_box.trak_boxes[0].tkhd_box.track_id;
        moov_box.trak_boxes[0]
            .mdia_box
            .minf_box
            .stbl_box
            .stsd_box
            .entries
            .push(sample_entry.clone());
        let trex_box = moov_box
            .mvex_box
            .as_mut()
            .expect("muxer が生成した init セグメントは mvex を含む")
            .trex_boxes
            .iter_mut()
            .find(|trex_box| trex_box.track_id == track_id)
            .expect("最初の track の trex が存在する");
        trex_box.default_sample_description_index = default_sample_description_index;
    })
}

fn rewrite_media_segment_tfhd_sample_description_index(
    media_segment: &[u8],
    sample_description_index: Option<u32>,
) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    for traf_box in &mut moof_box.traf_boxes {
        traf_box.tfhd_box.sample_description_index = sample_description_index;
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した");
    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

fn rewrite_media_segment_mdat_size_zero(media_segment: &[u8]) -> Vec<u8> {
    let (_moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    let mdat_size_offset = moof_box_size;
    assert!(
        media_segment.len() >= mdat_size_offset + 8,
        "media セグメントは moof の後に mdat ヘッダを含む"
    );

    let mut rewritten = media_segment.to_vec();
    rewritten[mdat_size_offset..mdat_size_offset + 4].copy_from_slice(&0u32.to_be_bytes());
    rewritten
}

/// メディアセグメントの `moof` を `default_base_is_moof = false` の形に書き換え、書き換えたセグメントと `traf` の個数を返す
///
/// `moof_position` は `media_segment` 内の `moof` の開始位置。
///
/// muxer は常に `default_base_is_moof = true` かつ `base_data_offset` なしで出力するため、
/// 各 `trun` の `data_offset` は `moof` の先頭からの相対値になっている。
/// `default_base_is_moof = false` かつ `base_data_offset` なしの場合、demuxer は
/// 最初の `traf` では `moof` の先頭を、2 番目以降の `traf` では直前の `traf` のデータ末尾を基準にする。
/// そのため、2 番目以降の `traf` の `trun` の `data_offset` を、直前の `traf` のデータ末尾からの相対値に直す。
/// muxer の出力では、トラックの payload が `traf` と同じ順に隙間なく並ぶため、直した値は 0 になる。
///
/// 直前の `traf` のデータ末尾は、demux の結果ではなく `trun` の `data_offset` とサンプルサイズから求める。
/// demux の結果を使うと、検証対象の計算そのものを期待値に使うことになるためである
fn rewrite_media_segment_default_base_is_moof_false(
    media_segment: &[u8],
    moof_position: usize,
) -> (Vec<u8>, usize) {
    let (mut moof_box, moof_box_size) = MoofBox::decode(&media_segment[moof_position..])
        .expect("media セグメントからの moof デコードに失敗した");

    // `moof` の先頭からの相対位置で、直前の `traf` のデータ末尾を持つ
    let mut prev_traf_data_end: Option<i64> = None;
    for traf_box in &mut moof_box.traf_boxes {
        // muxer の出力が前提どおりであることを確認する。
        // 前提が崩れたときに、黙って別の内容を検証するテストにならないようにするため
        assert!(
            traf_box.tfhd_box.default_base_is_moof,
            "muxer は default_base_is_moof = true で出力する"
        );
        assert_eq!(
            traf_box.tfhd_box.base_data_offset, None,
            "muxer は base_data_offset を明示しない"
        );
        traf_box.tfhd_box.default_base_is_moof = false;

        let base = prev_traf_data_end.unwrap_or(0);
        let mut traf_data_end = base;
        for trun_box in &mut traf_box.trun_boxes {
            let data_start = i64::from(
                trun_box
                    .data_offset
                    .expect("muxer は trun に data_offset を書く"),
            );
            let data_size: i64 = trun_box
                .samples
                .iter()
                .map(|sample| {
                    i64::from(
                        sample
                            .size
                            .expect("muxer は trun に各サンプルのサイズを書く"),
                    )
                })
                .sum();
            traf_data_end = traf_data_end.max(data_start + data_size);
            trun_box.data_offset = Some(
                i32::try_from(data_start - base).expect("書き換えた data_offset は i32 に収まる"),
            );
        }
        prev_traf_data_end = Some(traf_data_end);
    }

    let moof_bytes = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した");
    // フラグのビットと data_offset の値を変えるだけなので、moof のサイズは変わらない
    assert_eq!(
        moof_bytes.len(),
        moof_box_size,
        "書き換えで moof のサイズが変わった"
    );

    let mut rewritten = media_segment[..moof_position].to_vec();
    rewritten.extend_from_slice(&moof_bytes);
    rewritten.extend_from_slice(&media_segment[moof_position + moof_box_size..]);
    (rewritten, moof_box.traf_boxes.len())
}

/// メディアセグメントの先頭にある `moof` を `f` で書き換え、`trun` の `data_offset` を補正したセグメントを返す
///
/// muxer は `default_base_is_moof = true` かつ `base_data_offset` なしで出力するため、
/// 各 `trun` の `data_offset` は `moof` の先頭からの相対値になっている。
/// 書き換えで `moof` のサイズが変わると、後ろに続く `mdat` の位置も同じだけずれる。
/// そのため、サイズの差を各 `trun` の `data_offset` に足して、サンプルデータを指す位置を保つ。
///
/// `data_offset` がない `trun` は補正しない（直前の `trun` のデータ末尾から始まるため）。
///
/// `f` の中で書く `data_offset` は、書き換える前の `moof` の先頭を基準にした値にする
fn rewrite_media_segment_moof(media_segment: &[u8], f: impl FnOnce(&mut MoofBox)) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    f(&mut moof_box);

    // `data_offset` の補正は、書き換えた後のすべての `trun` の基準が `moof` の先頭であることを前提にしている。
    // `f` が追加した `traf` も補正の対象になるため、書き換えた後に確認する。
    // 前提が崩れたときに、黙って別の内容を検証するテストにならないようにするためである
    for traf_box in &moof_box.traf_boxes {
        assert!(
            traf_box.tfhd_box.default_base_is_moof,
            "書き換えた後の traf は default_base_is_moof = true である"
        );
        assert_eq!(
            traf_box.tfhd_box.base_data_offset, None,
            "書き換えた後の traf は base_data_offset を明示しない"
        );
    }

    let rewritten_moof_size = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した")
        .len();
    let size_delta = i32::try_from(rewritten_moof_size as i64 - moof_box_size as i64)
        .expect("moof のサイズの差は i32 に収まる");
    for traf_box in &mut moof_box.traf_boxes {
        for trun_box in &mut traf_box.trun_boxes {
            // `data_offset` がない `trun` は直前の `trun` のデータ末尾から始まるため、補正が要らない
            if let Some(data_offset) = trun_box.data_offset {
                trun_box.data_offset = Some(
                    data_offset
                        .checked_add(size_delta)
                        .expect("補正した data_offset は i32 に収まる"),
                );
            }
        }
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した");
    // `data_offset` の値を変えるだけなので、補正で moof のサイズは変わらない
    assert_eq!(
        rewritten.len(),
        rewritten_moof_size,
        "data_offset の補正で moof のサイズが変わった"
    );

    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

/// このファイルで共通の PBT ケース数（旧 `with_cases(256)` を維持）
const CASES: usize = 256;

/// Options で指定した language / name が init セグメントの demux で復元される
///
/// 3 kind（映像・音声・字幕）すべてで反映されることを固定する
#[test]
fn track_metadata_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let video_meta = helpers::arb_track_metadata(ctx);
        let audio_meta = helpers::arb_track_metadata(ctx);
        let subtitle_meta = helpers::arb_track_metadata(ctx);

        let options = SegmentMuxerOptions {
            video_track: video_meta.clone(),
            audio_track: audio_meta.clone(),
            subtitle_track: subtitle_meta.clone(),
            ..Default::default()
        };
        let mut muxer = Fmp4SegmentMuxer::with_options(options)
            .expect("Fmp4SegmentMuxer::with_options に失敗した");

        let video_entry = create_avc1_sample_entry(320, 240);
        let audio_entry = create_opus_sample_entry();
        let subtitle_entry = create_stpp_sample_entry();
        let video = TestSample {
            track_index: 0,
            duration: 3000,
            keyframe: true,
            data: vec![0x11; 16],
        };
        let audio = TestSample {
            track_index: 1,
            duration: 960,
            keyframe: true,
            data: vec![0x22; 8],
        };
        let subtitle = TestSample {
            track_index: 2,
            duration: 1000,
            keyframe: true,
            data: vec![0x33; 4],
        };
        let samples = [
            video_segment_sample(&video_entry, &video, None),
            audio_segment_sample(&audio_entry, &audio),
            subtitle_segment_sample(&subtitle_entry, &subtitle),
        ];
        let payloads: [&[u8]; 3] = [&video.data, &audio.data, &subtitle.data];
        let _segment_bytes = build_complete_media_segment(&mut muxer, &samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let (ftyp_box, ftyp_size) =
            FtypBox::decode(&init_bytes).expect("ftyp のデコードに失敗した");
        let _ = ftyp_box;
        let moov_bytes = &init_bytes[ftyp_size..];
        let (moov_box, moov_size) = MoovBox::decode(moov_bytes).expect("moov のデコードに失敗した");
        assert_eq!(
            moov_size,
            moov_bytes.len(),
            "moov の decode サイズがバイト列長と一致しない"
        );
        assert_eq!(moov_box.trak_boxes.len(), 3);
        helpers::assert_track_metadata(&moov_box.trak_boxes[0], &video_meta);
        helpers::assert_track_metadata(&moov_box.trak_boxes[1], &audio_meta);
        helpers::assert_track_metadata(&moov_box.trak_boxes[2], &subtitle_meta);

        // demuxer でも init を受理できることを確認する
        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 3);
        Ok(())
    })?;
    Ok(())
}

/// 単一映像トラックの init + メディアセグメント roundtrip
#[test]
fn video_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 1..10, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].kind, TrackKind::Video);
        assert_eq!(tracks[0].timescale.get(), 90000);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            assert_eq!(ds.duration, orig.duration);
            assert_eq!(ds.keyframe, orig.keyframe);
            assert_eq!(ds.data_size, orig.data.len());

            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }
        Ok(())
    })?;
    Ok(())
}

/// 単一音声トラックの roundtrip
#[test]
fn audio_only_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let samples = sample_vec(ctx, 1..10, |ctx| arb_audio_sample(ctx, 0));

        let sample_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| audio_segment_sample(&sample_entry, sample))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].kind, TrackKind::Audio);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            assert_eq!(ds.duration, orig.duration);
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }
        Ok(())
    })?;
    Ok(())
}

/// 同一トラック内で 2 番目以降のサンプルが `sample_entry: None` でも
/// 直前に観測した sample entry を継承してメディアセグメントを生成できることを確認する
#[test]
fn sample_entry_can_be_omitted_after_first_sample() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 2..10, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .enumerate()
            .map(|(index, sample)| {
                let mut segment_sample = video_segment_sample(&sample_entry, sample, None);
                if index > 0 {
                    segment_sample.sample_entry = None;
                }
                segment_sample
            })
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), samples.len());
        assert_eq!(demuxed[0].sample_entry, Some(&sample_entry));
        for sample in demuxed.iter().skip(1) {
            assert!(sample.sample_entry.is_none());
        }

        for (orig, demuxed_sample) in samples.iter().zip(demuxed.iter()) {
            assert_eq!(demuxed_sample.duration, orig.duration);
            assert_eq!(demuxed_sample.keyframe, orig.keyframe);
            assert_eq!(demuxed_sample.data_size, orig.data.len());

            let actual = &segment_bytes[demuxed_sample.data_offset as usize
                ..demuxed_sample.data_offset as usize + demuxed_sample.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }
        Ok(())
    })?;
    Ok(())
}

/// 映像＋音声の 2 トラック roundtrip
#[test]
fn video_audio_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let video_samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));
        let audio_samples = sample_vec(ctx, 1..5, |ctx| arb_audio_sample(ctx, 1));

        let video_sample_entry = create_avc1_sample_entry(width, height);
        let audio_sample_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("muxer の作成に失敗した");

        let mut all_samples: Vec<TestSample> = Vec::new();
        let max_len = video_samples.len().max(audio_samples.len());
        for i in 0..max_len {
            if let Some(s) = video_samples.get(i) {
                all_samples.push(s.clone());
            }
            if let Some(s) = audio_samples.get(i) {
                all_samples.push(s.clone());
            }
        }

        let fmp4_samples: Vec<Sample> = all_samples
            .iter()
            .map(|sample| {
                if sample.track_index == 0 {
                    video_segment_sample(&video_sample_entry, sample, None)
                } else {
                    audio_segment_sample(&audio_sample_entry, sample)
                }
            })
            .collect();
        let payloads: Vec<&[u8]> = all_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let demux_tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(demux_tracks.len(), 2);

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), video_samples.len() + audio_samples.len());

        let demuxed_video: Vec<_> = demuxed.iter().filter(|s| s.track.track_id == 1).collect();
        assert_eq!(demuxed_video.len(), video_samples.len());
        for (orig, ds) in video_samples.iter().zip(demuxed_video.iter()) {
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }

        let demuxed_audio: Vec<_> = demuxed.iter().filter(|s| s.track.track_id == 2).collect();
        assert_eq!(demuxed_audio.len(), audio_samples.len());
        for (orig, ds) in audio_samples.iter().zip(demuxed_audio.iter()) {
            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }
        Ok(())
    })?;
    Ok(())
}

/// composition_time_offset が roundtrip で保持されることを確認する
#[test]
fn composition_time_offset_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let samples_with_cto = sample_vec(ctx, 1..10, |ctx| arb_video_sample_with_cto(ctx, 0));

        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples_with_cto
            .iter()
            .map(|(sample, cto)| video_segment_sample(&sample_entry, sample, *cto))
            .collect();
        let payloads: Vec<&[u8]> = samples_with_cto
            .iter()
            .map(|(sample, _)| sample.data.as_slice())
            .collect();
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), samples_with_cto.len());

        // いずれかのサンプルに CTO がある場合、muxer は全サンプルを Some(x) に正規化する
        let has_any_cto = samples_with_cto.iter().any(|(_, c)| c.is_some());

        for ((_, expected_cto), ds) in samples_with_cto.iter().zip(demuxed.iter()) {
            let normalized = if has_any_cto {
                Some(expected_cto.unwrap_or(0))
            } else {
                None
            };
            assert_eq!(ds.composition_time_offset, normalized);
        }
        Ok(())
    })?;
    Ok(())
}

/// mfra_bytes が正しいバイト列を生成することを確認する
#[test]
fn mfra_bytes_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let segments = sample_vec(ctx, 1..5, |ctx| {
            sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0))
        });

        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let mut segment_sizes = Vec::new();

        for segment_samples in &segments {
            let fmp4_samples: Vec<Sample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = segment_samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
            segment_sizes.push(segment.len() as u64);
        }

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let mfra = muxer.mfra_bytes().expect("mfra の構築に失敗した");

        // mfra が valid な MP4 ボックスとしてデコードできること
        let (mfra_box, decoded_size) = MfraBox::decode(&mfra).expect("mfra のデコードに失敗した");
        assert_eq!(decoded_size, mfra.len());

        // tfra のエントリ数はセグメント数と一致すること
        assert_eq!(mfra_box.tfra_boxes.len(), 1);
        assert_eq!(mfra_box.tfra_boxes[0].entries.len(), segments.len());

        let init_size = init_bytes.len() as u64;
        let mut expected_moof_offset = init_size;
        for (entry, segment_size) in mfra_box.tfra_boxes[0]
            .entries
            .iter()
            .zip(segment_sizes.iter().copied())
        {
            assert_eq!(entry.moof_offset, expected_moof_offset);
            expected_moof_offset += segment_size;
        }

        // mfro.size が mfra 全体のサイズと一致すること
        assert_eq!(mfra_box.mfro_box.size, mfra.len() as u32);
        Ok(())
    })?;
    Ok(())
}

/// sidx あり／なしを混在させたときにも tfra.moof_offset が実 moof 位置を指すことを確認する
///
/// sidx 付きセグメントはメディアセグメントの直前に sidx を置くため、
/// tfra.moof_offset は init + それまでのセグメント合計 + 自セグメントの sidx サイズ を指す必要がある。
#[test]
fn mfra_bytes_roundtrip_with_sidx_mix() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let segments = sample_vec(ctx, 1..5, |ctx| {
            let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));
            let with_sidx = noprop::sample_bool(ctx);
            (samples, with_sidx)
        });

        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        // 各セグメントについて (sidx サイズ, セグメント全体のバイト数) を記録する。
        // sidx なしの場合は sidx サイズを 0 とする。
        let mut segment_layouts: Vec<(u64, u64)> = Vec::new();

        for (segment_samples, with_sidx) in &segments {
            let fmp4_samples: Vec<Sample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = segment_samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let segment = if *with_sidx {
                build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads)
            } else {
                build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads)
            };

            // sidx サイズはセグメント先頭から SidxBox をデコードして得る（サイズ算出を実装側と分離するため）
            let sidx_size = if *with_sidx {
                let (_sidx_box, decoded) =
                    SidxBox::decode(&segment).expect("セグメントからの sidx デコードに失敗した");
                decoded as u64
            } else {
                0
            };
            segment_layouts.push((sidx_size, segment.len() as u64));
        }

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let mfra = muxer.mfra_bytes().expect("mfra の構築に失敗した");

        let (mfra_box, decoded_size) = MfraBox::decode(&mfra).expect("mfra のデコードに失敗した");
        assert_eq!(decoded_size, mfra.len());

        // 単一映像トラックのみのため tfra_box は 1 つ、エントリ数はセグメント数と一致する
        assert_eq!(mfra_box.tfra_boxes.len(), 1);
        assert_eq!(mfra_box.tfra_boxes[0].entries.len(), segments.len());

        // 実 moof 位置 = セグメント先頭 + sidx サイズ（sidx なしなら 0）を各エントリで検証する
        let init_size = init_bytes.len() as u64;
        let mut media_head = init_size;
        for (entry, (sidx_size, segment_size)) in mfra_box.tfra_boxes[0]
            .entries
            .iter()
            .zip(segment_layouts.iter().copied())
        {
            let expected_moof_offset = media_head + sidx_size;
            assert_eq!(entry.moof_offset, expected_moof_offset);
            media_head += segment_size;
        }

        // mfro.size が mfra 全体のサイズと一致すること
        assert_eq!(mfra_box.mfro_box.size, mfra.len() as u32);
        Ok(())
    })?;
    Ok(())
}

/// 映像 + 音声のマルチトラックで sidx あり／なし混在時にも tfra.moof_offset が実 moof 位置を指すことを確認する
///
/// 各セグメントで音声サンプルを 0 個以上（映像は必ず 1 個以上）に振り分けることで、以下の分岐をカバーする:
/// - 両トラックが同じセグメントに含まれる（両方の tfra エントリに sidx 加算が入ることの検証）
/// - 音声トラックが 2 セグメント目以降に初めて登場する（`pre_tfra_lens.get(track_index) = None → unwrap_or(0)` に落ちる経路）
/// - 既存トラックで今回サンプルが無い（`entries.len() == pre_len` で加算スキップされる経路）
#[test]
fn mfra_bytes_roundtrip_with_sidx_mix_multi_track() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let segments = sample_vec(ctx, 1..5, |ctx| {
            let video_samples = sample_vec(ctx, 1..3, |ctx| arb_video_sample(ctx, 0));
            let audio_samples = sample_vec(ctx, 0..3, |ctx| arb_audio_sample(ctx, 1));
            let with_sidx = noprop::sample_bool(ctx);
            (video_samples, audio_samples, with_sidx)
        });

        let video_entry = create_avc1_sample_entry(320, 240);
        let audio_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        // 各セグメントについて (実 moof 位置 = セグメント先頭 + sidx サイズ, has_audio) を記録する
        let mut segment_moof_positions: Vec<(u64, bool)> = Vec::new();
        let mut cumulative_bytes = 0u64;

        for (video_samples, audio_samples, with_sidx) in &segments {
            let has_audio = !audio_samples.is_empty();

            // 映像を先に並べることで video が track_id=1、audio が track_id=2 になるよう固定する
            let mut fmp4_samples: Vec<Sample> = Vec::new();
            let mut payloads: Vec<&[u8]> = Vec::new();
            for sample in video_samples {
                fmp4_samples.push(video_segment_sample(&video_entry, sample, None));
                payloads.push(sample.data.as_slice());
            }
            for sample in audio_samples {
                fmp4_samples.push(audio_segment_sample(&audio_entry, sample));
                payloads.push(sample.data.as_slice());
            }

            let segment = if *with_sidx {
                build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads)
            } else {
                build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads)
            };

            let sidx_size = if *with_sidx {
                let (_sidx_box, decoded) =
                    SidxBox::decode(&segment).expect("セグメントからの sidx デコードに失敗した");
                decoded as u64
            } else {
                0
            };

            segment_moof_positions.push((cumulative_bytes + sidx_size, has_audio));
            cumulative_bytes += segment.len() as u64;
        }

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let mfra = muxer.mfra_bytes().expect("mfra の構築に失敗した");

        let (mfra_box, decoded_size) = MfraBox::decode(&mfra).expect("mfra のデコードに失敗した");
        assert_eq!(decoded_size, mfra.len());

        // 映像トラックは全セグメントで必ずサンプルを持つ。音声はサンプルがあるセグメントだけ tfra エントリを持つ
        let has_audio_any = segment_moof_positions.iter().any(|(_, a)| *a);
        let expected_track_count = 1 + usize::from(has_audio_any);
        assert_eq!(mfra_box.tfra_boxes.len(), expected_track_count);

        let init_size = init_bytes.len() as u64;

        // 映像トラック（track_id=1）は全セグメントの moof を指すこと
        let video_tfra = mfra_box
            .tfra_boxes
            .iter()
            .find(|t| t.track_id == 1)
            .expect("track_id=1 の video tfra_box が存在する");
        assert_eq!(video_tfra.entries.len(), segment_moof_positions.len());
        for (entry, (moof_pos, _)) in video_tfra
            .entries
            .iter()
            .zip(segment_moof_positions.iter().copied())
        {
            assert_eq!(entry.moof_offset, init_size + moof_pos);
        }

        // 音声トラック（track_id=2）は音声サンプルがあったセグメントの moof のみを指すこと
        if has_audio_any {
            let audio_tfra = mfra_box
                .tfra_boxes
                .iter()
                .find(|t| t.track_id == 2)
                .expect("track_id=2 の audio tfra_box が存在する");
            let expected_audio_offsets: Vec<u64> = segment_moof_positions
                .iter()
                .filter(|(_, a)| *a)
                .map(|(p, _)| init_size + *p)
                .collect();
            assert_eq!(audio_tfra.entries.len(), expected_audio_offsets.len());
            for (entry, expected) in audio_tfra.entries.iter().zip(expected_audio_offsets.iter()) {
                assert_eq!(entry.moof_offset, *expected);
            }
        }

        // mfro.size が mfra 全体のサイズと一致すること
        assert_eq!(mfra_box.mfro_box.size, mfra.len() as u32);
        Ok(())
    })?;
    Ok(())
}

/// sidx 付きセグメントが正しく demux できることを確認する
#[test]
fn sidx_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();

        // sidx 付きセグメントを生成する
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes =
            build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        // sidx は自動的にスキップされて正常に demux できる
        let demuxed = demuxer
            .handle_media_segment(&segment_bytes)
            .expect("sidx セグメントの処理に失敗した");

        assert_eq!(demuxed.len(), samples.len());

        for (orig, ds) in samples.iter().zip(demuxed.iter()) {
            assert_eq!(ds.duration, orig.duration);
            assert_eq!(ds.keyframe, orig.keyframe);
            assert_eq!(ds.data_size, orig.data.len());

            let actual =
                &segment_bytes[ds.data_offset as usize..ds.data_offset as usize + ds.data_size];
            assert_eq!(actual, orig.data.as_slice());
        }
        Ok(())
    })?;
    Ok(())
}

/// 読み飛ばせるトップレベルボックスの種別の候補
///
/// ISO/IEC 14496-12:2022 では、`styp` (8.16.2) / `sidx` (8.16.3) / `ssix` (8.16.4) / `prft` (8.16.5) が
/// メディアセグメントの `moof` の前に置かれ得る。`free` / `skip` (8.1.2) も置かれ得る。
/// `ftyp` / `moov` / `mdat` は 8.16 でメディアセグメントの `moof` の前に置くものとして挙がっていないが、
/// `handle_media_segment` の doc で読み飛ばすと明記しているため候補に含める
const SKIPPABLE_BOX_TYPES: [[u8; 4]; 9] = [
    *b"styp", *b"sidx", *b"ssix", *b"prft", *b"free", *b"skip", *b"ftyp", *b"moov", *b"mdat",
];

/// `moof` より前に置くトップレベルボックスを 1 個生成し、エンコード済みのバイト列と、largesize 形式を使ったかどうかを返す
fn arb_leading_box(ctx: &mut TestCaseContext) -> (Vec<u8>, bool) {
    arb_skippable_box(ctx, &[])
}

/// `moof` より前、`moof` と `mdat` の間、`mdat` の後ろに置くトップレベルボックスを 1 個生成し、
/// エンコード済みのバイト列と、largesize 形式を使ったかどうかを返す
///
/// 種別は [`SKIPPABLE_BOX_TYPES`] と、`moof` / `uuid` 以外の任意の 4CC から引く。
/// demuxer はこれらのボックスの中身を解釈しないため、ペイロードは任意のバイト列でよい。
/// ヘッダーは 32 ビットの size と、size=1 + 種別に続く 64 ビットの largesize の両方の形式を使う。
///
/// `excluded_types` の種別は候補から外す。`mdat` を置けない位置で使う。
///
/// `moof` は常に候補から外す。`moof` は `mdat` を探す読み飛ばしの終了条件であり、
/// `mdat` より先や後ろに置くと、1 回の呼び出しで扱える `moof` + `mdat` 1 組という制限に反するためである。
/// `uuid` も常に候補から外す。
/// ISO/IEC 14496-12:2022 の 4.2.2 では largesize の後ろに 16 バイトの usertype が続くが、
/// `BoxHeader` は usertype を largesize より前にあるものとして読み書きするため、
/// largesize 形式の `uuid` を正しく読み飛ばせない。
/// 32 ビット size の `uuid` は usertype を含めて size が 24 以上なら読み飛ばせるが、
/// 形式を分けて生成する複雑さに見合わないため、こちらも候補から外す
fn arb_skippable_box(ctx: &mut TestCaseContext, excluded_types: &[[u8; 4]]) -> (Vec<u8>, bool) {
    let candidates: Vec<[u8; 4]> = SKIPPABLE_BOX_TYPES
        .iter()
        .copied()
        .filter(|box_type| !excluded_types.contains(box_type))
        .collect();
    assert!(
        !candidates.is_empty(),
        "除外した結果、選べる既知の種別が 1 つもない"
    );

    // 既知の種別それぞれと任意の 4CC を同じ重みで選ぶ。
    // 既知の種別の枝には種別の数だけ重みを与え、その中から一様に選ぶ
    let box_type = match noprop::sample_weighted_index(ctx, &[candidates.len() as u32, 1]) {
        0 => noprop::sample_choice(ctx, &candidates),
        _ => {
            // `moof` / `uuid` / 除外した種別を引く確率は 1 回あたり 2 / 2^32 以下なので、
            // 最大 4 回の局所リトライで実質確実に別の値を得られる
            noprop::sample_with_rejection(ctx, 4, |ctx| {
                let box_type = noprop::sample_bytes::<4>(ctx);
                (box_type != *b"moof"
                    && box_type != *b"uuid"
                    && !excluded_types.contains(&box_type))
                .then_some(box_type)
            })
        }
    };
    let payload_len = noprop::sample_usize_in(ctx, 0..64);
    let payload = noprop::sample_bytes_vec(ctx, payload_len);
    let large_size = noprop::sample_bool(ctx);

    let mut box_bytes = Vec::new();
    if large_size {
        // size=1 (4 バイト) + 種別 (4 バイト) + largesize (8 バイト) の 16 バイトがヘッダーになる
        box_bytes.extend_from_slice(&1u32.to_be_bytes());
        box_bytes.extend_from_slice(&box_type);
        box_bytes.extend_from_slice(&(16 + payload_len as u64).to_be_bytes());
    } else {
        // size (4 バイト) + 種別 (4 バイト) の 8 バイトがヘッダーになる
        box_bytes.extend_from_slice(&(8 + payload_len as u32).to_be_bytes());
        box_bytes.extend_from_slice(&box_type);
    }
    box_bytes.extend_from_slice(&payload);
    (box_bytes, large_size)
}

/// 32 ビットの size=0 のボックス（入力の末尾まで続く）を生成する
///
/// 種別は `free` とし、ペイロードは任意のバイト列にする。
/// 入力の末尾に置く前提であり、後ろに別のボックスを置いてはいけない
fn build_size_zero_box(ctx: &mut TestCaseContext) -> Vec<u8> {
    let payload_len = noprop::sample_usize_in(ctx, 0..64);
    let payload = noprop::sample_bytes_vec(ctx, payload_len);

    let mut box_bytes = Vec::new();
    box_bytes.extend_from_slice(&0u32.to_be_bytes());
    box_bytes.extend_from_slice(b"free");
    box_bytes.extend_from_slice(&payload);
    box_bytes
}

/// メディアセグメントの `moof` の直後に `between_bytes` を挿し込み、`trun` の `data_offset` をその合計サイズだけ増やす
///
/// `adjust_all_trafs` が true ならすべての `traf` の `trun` を、false なら最初の `traf` の `trun` だけを増やす。
/// `default_base_is_moof = true` では 2 番目以降の `traf` も `moof` の先頭を基準にするため true を、
/// `default_base_is_moof = false` では 2 番目以降の `traf` が直前の `traf` のデータ末尾を基準にするため false を使う。
/// 後者では、最初の `traf` のデータ末尾が動くことで 2 番目以降の基準も同じだけ動く
fn insert_boxes_between_moof_and_mdat(
    media_segment: &[u8],
    between_bytes: &[u8],
    adjust_all_trafs: bool,
) -> Vec<u8> {
    let (mut moof_box, moof_box_size) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    let size_delta =
        i32::try_from(between_bytes.len()).expect("挿し込むボックスの合計サイズは i32 に収まる");

    for (index, traf_box) in moof_box.traf_boxes.iter_mut().enumerate() {
        if !adjust_all_trafs && index > 0 {
            continue;
        }
        for trun_box in &mut traf_box.trun_boxes {
            let data_offset = trun_box
                .data_offset
                .expect("muxer は trun に data_offset を書く");
            trun_box.data_offset = Some(
                data_offset
                    .checked_add(size_delta)
                    .expect("補正した data_offset は i32 に収まる"),
            );
        }
    }

    let mut rewritten = moof_box
        .encode_to_vec()
        .expect("media セグメント書き換え中の moof エンコードに失敗した");
    // `data_offset` の値だけを変えるので `moof` のサイズは変わらない
    assert_eq!(
        rewritten.len(),
        moof_box_size,
        "data_offset の補正で moof のサイズが変わった"
    );
    rewritten.extend_from_slice(between_bytes);
    rewritten.extend_from_slice(&media_segment[moof_box_size..]);
    rewritten
}

/// `moof` より前に任意のトップレベルボックスを置いても、置かない場合と比べて `data_offset` 以外は同じサンプル列が得られることを確認する
///
/// `sidx` あり / なしのメディアセグメントの前に、[`arb_leading_box`] で生成したボックスを 1 個以上置く。
/// 元のセグメントが `sidx` ありなら `sidx` の前に別のボックスが並ぶため、
/// `styp` + `sidx` + `moof` や `sidx` + `sidx` + `moof` のような並びも生成される。
///
/// muxer は `tfhd` に `base_data_offset` を明示しないため、`data_offset` は `moof` の位置に追従する。
/// そのため、置かない場合との違いは、`data_offset` が置いたボックスの合計サイズ分ずれることだけである。
///
/// `default_base_is_moof` は、muxer の出力そのままの true と、
/// [`rewrite_media_segment_default_base_is_moof_false`] で false に書き換えた場合の両方を確認する。
/// false の場合、最初の `traf` は `moof` の位置を基準にする。
/// 前にボックスを置くと `moof` の位置が変わるため、その基準が正しく使われることを確認できる。
///
/// ここでは置いた場合と置かない場合の差分だけを見る。
/// 置かない場合の demux 結果そのものの正しさは、`video_audio_roundtrip` / `sidx_roundtrip` /
/// `composition_time_offset_roundtrip` などの roundtrip に任せる
#[test]
fn leading_boxes_before_moof_are_skipped() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // 元のセグメントが sidx なし / sidx ありのそれぞれで、前に置いたボックスを読み飛ばせたケース数。
    // 各ケースで 1/2 の確率で選ぶので、`CASES`（256）ケースで片方を一度も通らない確率は 2^-256 程度
    let without_sidx_cases = std::cell::Cell::new(0usize);
    let with_sidx_cases = std::cell::Cell::new(0usize);
    // largesize 形式のボックスを読み飛ばせたケース数。
    // 各ボックスで 1/2 の確率で選ぶので、`CASES`（256）ケースで一度も通らない確率は 2^-256 以下
    let large_size_cases = std::cell::Cell::new(0usize);
    // default_base_is_moof = false で traf が 2 個以上あるケース数。
    // false を 1/2、音声サンプルが 1 個以上（traf が 2 個）を 4/5 の確率で選ぶので 1 ケースあたり 2/5 となり、
    // `CASES`（256）ケースで一度も通らない確率は (3/5)^256 程度
    let base_is_moof_false_multi_traf_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        // sidx を付ける muxer は PTS が負になる入力を拒否するため、
        // composition_time_offset は非負の範囲から引く
        let video_samples = sample_vec(ctx, 1..5, |ctx| {
            let sample = arb_video_sample(ctx, 0);
            let composition_time_offset = if noprop::sample_bool(ctx) {
                Some(noprop::sample_u64_in(ctx, 0..3001) as i64)
            } else {
                None
            };
            (sample, composition_time_offset)
        });
        // 音声トラックがない場合と、traf が 2 個になる場合の両方を含める
        let audio_samples = sample_vec(ctx, 0..5, |ctx| arb_audio_sample(ctx, 1));
        let with_sidx = noprop::sample_bool(ctx);
        let default_base_is_moof = noprop::sample_bool(ctx);
        let leading_boxes = sample_vec(ctx, 1..5, arb_leading_box);

        let video_sample_entry = create_avc1_sample_entry(320, 240);
        let audio_sample_entry = create_opus_sample_entry();

        // 映像と音声を交互に並べる
        let mut fmp4_samples = Vec::new();
        let mut payloads: Vec<&[u8]> = Vec::new();
        for i in 0..video_samples.len().max(audio_samples.len()) {
            if let Some((sample, composition_time_offset)) = video_samples.get(i) {
                fmp4_samples.push(video_segment_sample(
                    &video_sample_entry,
                    sample,
                    *composition_time_offset,
                ));
                payloads.push(&sample.data);
            }
            if let Some(sample) = audio_samples.get(i) {
                fmp4_samples.push(audio_segment_sample(&audio_sample_entry, sample));
                payloads.push(&sample.data);
            }
        }

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let segment_bytes = if with_sidx {
            build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads)
        } else {
            build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads)
        };
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut prefixed_segment_bytes = Vec::new();
        let mut uses_large_size = false;
        for (box_bytes, large_size) in &leading_boxes {
            prefixed_segment_bytes.extend_from_slice(box_bytes);
            uses_large_size |= *large_size;
        }
        let prefix_size = prefixed_segment_bytes.len() as u64;

        // 前にボックスを置く側のセグメントだけを、必要なら default_base_is_moof = false に書き換える。
        // 比較元（前にボックスを置かない側）は muxer の出力そのままにし、同じサンプル位置になることを確認する
        let rewritten_traf_count = if default_base_is_moof {
            prefixed_segment_bytes.extend_from_slice(&segment_bytes);
            None
        } else {
            let moof_position = if with_sidx {
                let (_sidx_box, sidx_box_size) = SidxBox::decode(&segment_bytes)
                    .expect("media セグメントからの sidx デコードに失敗した");
                sidx_box_size
            } else {
                0
            };
            let (rewritten, traf_count) =
                rewrite_media_segment_default_base_is_moof_false(&segment_bytes, moof_position);
            prefixed_segment_bytes.extend_from_slice(&rewritten);
            Some(traf_count)
        };

        // demuxer はトラックごとに直前の sample description index を覚えており、
        // 2 回目の呼び出しでは sample_entry が None になる。
        // 比較する 2 つの入力は、それぞれ別の demuxer で処理する
        let mut expected_demuxer = Fmp4SegmentDemuxer::new();
        expected_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let expected = expected_demuxer
            .handle_media_segment(&segment_bytes)
            .expect("前にボックスを置かない media セグメントの処理に失敗した");

        let mut actual_demuxer = Fmp4SegmentDemuxer::new();
        actual_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let actual = actual_demuxer
            .handle_media_segment(&prefixed_segment_bytes)
            .expect("前にボックスを置いた media セグメントの処理に失敗した");

        assert_eq!(
            actual.len(),
            expected.len(),
            "前にボックスを置いてもサンプル数は変わらない"
        );
        for (index, (actual_sample, expected_sample)) in actual.iter().zip(expected.iter()).enumerate()
        {
            // `Sample` にフィールドが増えたときに比較漏れがコンパイルエラーになるよう、
            // 期待値側を `..` を使わずに分解する
            let shiguredo_mp4::demux::Sample {
                track,
                sample_entry,
                keyframe,
                timestamp,
                duration,
                data_offset,
                data_size,
                composition_time_offset,
            } = *expected_sample;
            assert_eq!(
                (
                    actual_sample.track,
                    actual_sample.sample_entry,
                    actual_sample.keyframe,
                    actual_sample.timestamp,
                    actual_sample.duration,
                    actual_sample.data_size,
                    actual_sample.composition_time_offset,
                ),
                (
                    track,
                    sample_entry,
                    keyframe,
                    timestamp,
                    duration,
                    data_size,
                    composition_time_offset,
                ),
                "{index} 番目のサンプルの data_offset 以外のフィールドは、前にボックスを置いても変わらない"
            );
            // data_offset だけが前に置いたボックスの合計サイズ分ずれる
            assert_eq!(
                actual_sample.data_offset,
                data_offset + prefix_size,
                "{index} 番目のサンプルの data_offset は前に置いたボックスの合計サイズ分だけずれる"
            );

            // ずれた先に元のサンプルデータがある
            let actual_start = actual_sample.data_offset as usize;
            let expected_start = data_offset as usize;
            assert_eq!(
                &prefixed_segment_bytes[actual_start..actual_start + actual_sample.data_size],
                &segment_bytes[expected_start..expected_start + data_size],
                "{index} 番目のサンプルのずれた先に元のサンプルデータがない"
            );
        }

        if with_sidx {
            with_sidx_cases.set(with_sidx_cases.get() + 1);
        } else {
            without_sidx_cases.set(without_sidx_cases.get() + 1);
        }
        if uses_large_size {
            large_size_cases.set(large_size_cases.get() + 1);
        }
        if rewritten_traf_count.is_some_and(|count| count >= 2) {
            base_is_moof_false_multi_traf_cases
                .set(base_is_moof_false_multi_traf_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        without_sidx_cases.get() > 0,
        "sidx なしのセグメントの前にボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_sidx_cases.get() > 0,
        "sidx ありのセグメントの前にボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        large_size_cases.get() > 0,
        "largesize 形式のボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        base_is_moof_false_multi_traf_cases.get() > 0,
        "default_base_is_moof = false で traf が 2 個以上のケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// `moof` と `mdat` の間、および `mdat` の後ろに任意のトップレベルボックスを置いても、
/// 置かない場合と比べて `data_offset` 以外は同じサンプル列が得られることを確認する
///
/// `moof` と `mdat` の間に置いたボックスの合計サイズだけ `data_offset` がずれる。
/// `mdat` の後ろに置いたボックスは `data_offset` に影響しない。
/// `mdat` の後ろの最後のボックスが 32 ビットの size=0 になる場合も確認する。
///
/// `default_base_is_moof` は、muxer の出力そのままの true と、
/// [`rewrite_media_segment_default_base_is_moof_false`] で false に書き換えた場合の両方を確認する。
/// false の場合、2 番目以降の `traf` の基準は直前の `traf` のデータ末尾なので、
/// `data_offset` を増やすのは最初の `traf` の `trun` だけになる。
///
/// ここでは置いた場合と置かない場合の差分だけを見る。
/// 置かない場合の demux 結果そのものの正しさは `video_audio_roundtrip` などの roundtrip に任せる
#[test]
fn boxes_between_moof_and_mdat_and_after_mdat_are_skipped() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // 間にボックスを置いたケース数。空でない確率は 3/4 なので、
    // `CASES`（256）ケースで一度も通らない確率は (1/4)^256 程度
    let with_between_cases = std::cell::Cell::new(0usize);
    // 後ろにボックスを置いたケース数。空でない確率は 3/4
    let with_after_cases = std::cell::Cell::new(0usize);
    // 後ろの最後のボックスを 32 ビットの size=0 にしたケース数。
    // `mdat` の size を 0 にする場合は後ろにボックスを置けないため、各ケースで 1/4 の確率で選ぶ
    let with_size_zero_after_cases = std::cell::Cell::new(0usize);
    // largesize 形式のボックスを読み飛ばせたケース数。各ボックスで 1/2 の確率で選ぶ
    let large_size_cases = std::cell::Cell::new(0usize);
    // `mdat` の size を 0 にしたケース数。各ケースで 1/2 の確率で選ぶ
    let with_mdat_size_zero_cases = std::cell::Cell::new(0usize);
    // `mdat` の size が 0 で、かつ `moof` と `mdat` の間にボックスを置いたケース数。
    // 各ケースで 1/2 × 3/4 の確率で選ぶので、`CASES`（256）ケースで一度も通らない確率は (5/8)^256 程度
    let mdat_size_zero_with_between_cases = std::cell::Cell::new(0usize);
    // default_base_is_moof = false で traf が 2 個以上あるケース数。
    // false を 1/2、音声サンプルが 1 個以上（traf が 2 個）を 4/5 の確率で選ぶので 1 ケースあたり 2/5 となる
    let base_is_moof_false_multi_traf_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let video_samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));
        // 音声トラックがない場合と、traf が 2 個になる場合の両方を含める
        let audio_samples = sample_vec(ctx, 0..5, |ctx| arb_audio_sample(ctx, 1));
        let default_base_is_moof = noprop::sample_bool(ctx);
        // 間に置くボックスからは `mdat` を外す。`mdat` を置くと、そこが `mdat` として扱われる
        let between_boxes = sample_vec(ctx, 0..4, |ctx| arb_skippable_box(ctx, &[*b"mdat"]));
        // 後ろに置くボックスは `mdat` でもよい。`mdat` の後ろの `mdat` も読み飛ばしの対象である
        let after_boxes = sample_vec(ctx, 0..4, |ctx| arb_skippable_box(ctx, &[]));
        let with_size_zero_after = noprop::sample_bool(ctx);

        let video_sample_entry = create_avc1_sample_entry(320, 240);
        let audio_sample_entry = create_opus_sample_entry();

        // 映像と音声を交互に並べる
        let mut fmp4_samples = Vec::new();
        let mut payloads: Vec<&[u8]> = Vec::new();
        for i in 0..video_samples.len().max(audio_samples.len()) {
            if let Some(sample) = video_samples.get(i) {
                fmp4_samples.push(video_segment_sample(&video_sample_entry, sample, None));
                payloads.push(&sample.data);
            }
            if let Some(sample) = audio_samples.get(i) {
                fmp4_samples.push(audio_segment_sample(&audio_sample_entry, sample));
                payloads.push(&sample.data);
            }
        }

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let mut segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        // `mdat` の size を 0 にすると `mdat` が入力の末尾まで続くため、`mdat` の後ろにはボックスを置けない。
        // `mdat` と読み飛ばしの組み合わせで `mdat` の位置の扱いが崩れないことを確認する
        let with_mdat_size_zero = noprop::sample_bool(ctx);
        if with_mdat_size_zero {
            segment_bytes = rewrite_media_segment_mdat_size_zero(&segment_bytes);
        }

        let mut between_bytes = Vec::new();
        let mut uses_large_size = false;
        for (box_bytes, large_size) in &between_boxes {
            between_bytes.extend_from_slice(box_bytes);
            uses_large_size |= *large_size;
        }
        let mut after_bytes = Vec::new();
        if !with_mdat_size_zero {
            for (box_bytes, large_size) in &after_boxes {
                after_bytes.extend_from_slice(box_bytes);
                uses_large_size |= *large_size;
            }
            if with_size_zero_after {
                after_bytes.extend_from_slice(&build_size_zero_box(ctx));
            }
        }

        // 比較元のセグメント。`default_base_is_moof = false` の場合は、書き換えた後のものを比較元にする
        let (reference_segment, rewritten_traf_count) = if default_base_is_moof {
            (segment_bytes, None)
        } else {
            let (rewritten, traf_count) =
                rewrite_media_segment_default_base_is_moof_false(&segment_bytes, 0);
            (rewritten, Some(traf_count))
        };

        // ボックスを挿し込む側のセグメント。
        // `default_base_is_moof = false` では、`data_offset` を増やすのは最初の `traf` の `trun` だけになる
        let mut inserted_segment = insert_boxes_between_moof_and_mdat(
            &reference_segment,
            &between_bytes,
            default_base_is_moof,
        );
        inserted_segment.extend_from_slice(&after_bytes);

        // demuxer はトラックごとに直前の sample description index を覚えており、
        // 2 回目の呼び出しでは sample_entry が None になる。
        // 比較する 2 つの入力は、それぞれ別の demuxer で処理する
        let mut expected_demuxer = Fmp4SegmentDemuxer::new();
        expected_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let expected = expected_demuxer
            .handle_media_segment(&reference_segment)
            .expect("ボックスを置かない media セグメントの処理に失敗した");

        let mut actual_demuxer = Fmp4SegmentDemuxer::new();
        actual_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let actual = actual_demuxer
            .handle_media_segment(&inserted_segment)
            .expect("ボックスを置いた media セグメントの処理に失敗した");

        assert_eq!(
            actual.len(),
            expected.len(),
            "ボックスを置いてもサンプル数は変わらない"
        );
        for (index, (actual_sample, expected_sample)) in actual.iter().zip(expected.iter()).enumerate()
        {
            // `Sample` にフィールドが増えたときに比較漏れがコンパイルエラーになるよう、
            // 期待値側を `..` を使わずに分解する
            let shiguredo_mp4::demux::Sample {
                track,
                sample_entry,
                keyframe,
                timestamp,
                duration,
                data_offset,
                data_size,
                composition_time_offset,
            } = *expected_sample;
            assert_eq!(
                (
                    actual_sample.track,
                    actual_sample.sample_entry,
                    actual_sample.keyframe,
                    actual_sample.timestamp,
                    actual_sample.duration,
                    actual_sample.data_size,
                    actual_sample.composition_time_offset,
                ),
                (
                    track,
                    sample_entry,
                    keyframe,
                    timestamp,
                    duration,
                    data_size,
                    composition_time_offset,
                ),
                "{index} 番目のサンプルの data_offset 以外のフィールドは、ボックスを置いても変わらない"
            );
            // `moof` と `mdat` の間に置いたボックスの合計サイズ分だけずれる。
            // `mdat` の後ろに置いたボックスは `data_offset` に影響しない
            assert_eq!(
                actual_sample.data_offset,
                data_offset + between_bytes.len() as u64,
                "{index} 番目のサンプルの data_offset は、間に置いたボックスの合計サイズ分だけずれる"
            );

            // ずれた先に元のサンプルデータがある
            let actual_start = actual_sample.data_offset as usize;
            let expected_start = data_offset as usize;
            assert_eq!(
                &inserted_segment[actual_start..actual_start + actual_sample.data_size],
                &reference_segment[expected_start..expected_start + data_size],
                "{index} 番目のサンプルのずれた先に元のサンプルデータがない"
            );
        }

        if !between_bytes.is_empty() {
            with_between_cases.set(with_between_cases.get() + 1);
        }
        if !after_bytes.is_empty() {
            with_after_cases.set(with_after_cases.get() + 1);
        }
        if with_size_zero_after && !with_mdat_size_zero {
            with_size_zero_after_cases.set(with_size_zero_after_cases.get() + 1);
        }
        if uses_large_size {
            large_size_cases.set(large_size_cases.get() + 1);
        }
        if with_mdat_size_zero {
            with_mdat_size_zero_cases.set(with_mdat_size_zero_cases.get() + 1);
        }
        if with_mdat_size_zero && !between_bytes.is_empty() {
            mdat_size_zero_with_between_cases
                .set(mdat_size_zero_with_between_cases.get() + 1);
        }
        if rewritten_traf_count.is_some_and(|count| count >= 2) {
            base_is_moof_false_multi_traf_cases
                .set(base_is_moof_false_multi_traf_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        with_between_cases.get() > 0,
        "moof と mdat の間にボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_after_cases.get() > 0,
        "mdat の後ろにボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_size_zero_after_cases.get() > 0,
        "mdat の後ろの最後のボックスを 32 ビットの size=0 にしたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        large_size_cases.get() > 0,
        "largesize 形式のボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_mdat_size_zero_cases.get() > 0,
        "mdat の size を 0 にしたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        mdat_size_zero_with_between_cases.get() > 0,
        "mdat の size を 0 にして、かつ間にボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        base_is_moof_false_multi_traf_cases.get() > 0,
        "default_base_is_moof = false で traf が 2 個以上のケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// sidx.referenced_size が payload を含むセグメント全体サイズを指すことを確認する
#[test]
fn sidx_referenced_size_includes_payload() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes =
            build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads);

        let (sidx_box, sidx_size) =
            SidxBox::decode(&segment_bytes).expect("sidx のデコードに失敗した");
        assert_eq!(sidx_box.references.len(), 1);
        assert_eq!(
            sidx_box.references[0].referenced_size as usize,
            segment_bytes.len() - sidx_size,
        );
        Ok(())
    })?;
    Ok(())
}

/// CTO=None 入力では sidx の `starts_with_sap` / `sap_type` が
/// samples[0].keyframe と一致すること
///
/// arb_video_sample は keyframe: bool をランダム化するが CTO=None のため PTS は
/// 単調非減少で samples[0] が最小 PTS を採る。したがって EPT サンプル == samples[0]
/// となり、`starts_with_sap == samples[0].keyframe` /
/// `sap_type == u8::from(samples[0].keyframe)` を任意入力で固定する。
/// `sap_delta_time` は現行の `0` のまま変更されない。
#[test]
fn sidx_starts_with_sap_matches_first_sample_keyframe() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let segment_bytes =
            build_complete_media_segment_with_sidx(&mut muxer, &fmp4_samples, &payloads);

        let (sidx_box, _) = SidxBox::decode(&segment_bytes).expect("sidx のデコードに失敗した");
        assert_eq!(sidx_box.references.len(), 1);

        let expected_starts_with_sap = samples[0].keyframe;
        assert_eq!(
            sidx_box.references[0].starts_with_sap,
            expected_starts_with_sap
        );
        assert_eq!(
            sidx_box.references[0].sap_type,
            u8::from(expected_starts_with_sap)
        );
        assert_eq!(sidx_box.references[0].sap_delta_time, 0);
        Ok(())
    })?;
    Ok(())
}

/// 最初のサンプルに `sample_entry` がない場合は
/// `create_media_segment_metadata_with_sidx()` がエラーを返すことを確認する
#[test]
fn sidx_rejects_missing_sample_entry_on_first_sample() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let duration = noprop::sample_u64_in(ctx, 1..3001) as u32;
        let keyframe = noprop::sample_bool(ctx);
        let data_len = noprop::sample_usize_in(ctx, 1..256);
        let data = noprop::sample_bytes_vec(ctx, data_len);

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let sample = Sample {
            track_kind: TrackKind::Video,
            timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("非ゼロである"),
            sample_entry: None,
            duration,
            keyframe,
            composition_time_offset: None,
            data_offset: 0,
            data_size: data.len(),
        };

        let result = muxer.create_media_segment_metadata_with_sidx(&[sample]);

        match result {
            Err(shiguredo_mp4::mux::MuxError::MissingSampleEntry { track_kind }) => {
                assert_eq!(track_kind, TrackKind::Video);
            }
            other => panic!("予期しない結果: {other:?}"),
        }
        Ok(())
    })?;
    Ok(())
}

/// `trex.default_sample_description_index` が `stsd` の先頭以外を指す場合でも
/// 対応する sample entry が使われることを確認する
#[test]
fn sample_entry_uses_trex_default_index() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let bootstrap_sample = video_segment_sample(&alternative_sample_entry, &samples[0], None);
        let bootstrap_segment = build_complete_media_segment(
            &mut muxer,
            &[bootstrap_sample],
            &[samples[0].data.as_slice()],
        );
        assert!(!bootstrap_segment.is_empty());
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let init_bytes = append_sample_entry_and_set_trex_default(
            &init_bytes,
            alternative_sample_entry.clone(),
            2,
        );

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| Sample {
                sample_entry: Some(original_sample_entry.clone()),
                ..video_segment_sample(&original_sample_entry, sample, None)
            })
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let media_segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let demuxed = demuxer
            .handle_media_segment(&media_segment)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed[0].sample_entry, Some(&alternative_sample_entry));
        for sample in demuxed.iter().skip(1) {
            assert!(sample.sample_entry.is_none());
        }
        Ok(())
    })?;
    Ok(())
}

/// `tfhd.sample_description_index` が `trex.default_sample_description_index` より優先されることを確認する
#[test]
fn sample_entry_prefers_tfhd_index() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let _ = build_complete_media_segment(
            &mut muxer,
            &[video_segment_sample(
                &original_sample_entry,
                &samples[0],
                None,
            )],
            &[samples[0].data.as_slice()],
        );
        let _ = build_complete_media_segment(
            &mut muxer,
            &[video_segment_sample(
                &alternative_sample_entry,
                &samples[0],
                None,
            )],
            &[samples[0].data.as_slice()],
        );
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let init_bytes =
            append_sample_entry_and_set_trex_default(&init_bytes, alternative_sample_entry, 2);

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let media_segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let media_segment =
            rewrite_media_segment_tfhd_sample_description_index(&media_segment, Some(1));

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let demuxed = demuxer
            .handle_media_segment(&media_segment)
            .expect("media セグメントの処理に失敗した");

        assert_eq!(demuxed[0].sample_entry, Some(&original_sample_entry));
        for sample in demuxed.iter().skip(1) {
            assert!(sample.sample_entry.is_none());
        }
        Ok(())
    })?;
    Ok(())
}

/// sample description index が切り替わった最初のサンプルでだけ
/// `sample_entry` が再度 `Some` になることを確認する
#[test]
fn sample_entry_is_emitted_only_on_change() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let first_segment_samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));
        let second_segment_samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let first_segment_input: Vec<Sample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<Sample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&alternative_sample_entry, sample, None))
            .collect();

        let first_payloads: Vec<&[u8]> = first_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let second_payloads: Vec<&[u8]> = second_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let first_segment =
            build_complete_media_segment(&mut muxer, &first_segment_input, &first_payloads);
        let second_segment =
            build_complete_media_segment(&mut muxer, &second_segment_input, &second_payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        let first_demuxed = demuxer
            .handle_media_segment(&first_segment)
            .expect("最初の media セグメントの処理に失敗した");
        assert_eq!(first_demuxed[0].sample_entry, Some(&original_sample_entry));
        for sample in first_demuxed.iter().skip(1) {
            assert!(sample.sample_entry.is_none());
        }

        let second_demuxed = demuxer
            .handle_media_segment(&second_segment)
            .expect("2 つ目の media セグメントの処理に失敗した");
        assert_eq!(
            second_demuxed[0].sample_entry,
            Some(&alternative_sample_entry)
        );
        for sample in second_demuxed.iter().skip(1) {
            assert!(sample.sample_entry.is_none());
        }
        Ok(())
    })?;
    Ok(())
}

/// `handle_media_segment()` が複数の `moof` + `mdat` ペアを 1 回で受け取った場合に
/// `mdat` の後ろの `moof` を読み飛ばさずエラーを返すことを確認する
///
/// `mdat` の後ろの `moof` 以外のトップレベルボックスは読み飛ばすが、
/// `moof` は 1 回の呼び出しで扱えるペアが 1 組だけという制限に反するためエラーにする
#[test]
fn rejects_multiple_moof_mdat_pairs_in_one_input() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let first_segment_samples = sample_vec(ctx, 1..4, |ctx| arb_video_sample(ctx, 0));
        let second_segment_samples = sample_vec(ctx, 1..4, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let first_segment_input: Vec<Sample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<Sample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();

        let first_payloads: Vec<&[u8]> = first_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let second_payloads: Vec<&[u8]> = second_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let mut concatenated =
            build_complete_media_segment(&mut muxer, &first_segment_input, &first_payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        concatenated.extend_from_slice(&build_complete_media_segment(
            &mut muxer,
            &second_segment_input,
            &second_payloads,
        ));

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");

        // `mdat` の後ろの `moof` を読み飛ばさず、狙った理由でエラーになることを確認する
        match demuxer.handle_media_segment(&concatenated) {
            Err(DemuxError::DecodeError(error)) => {
                assert_eq!(
                    error.reason, "found moof box after mdat in media segment",
                    "mdat の後ろの moof を拒否したことを示すエラー理由を期待した"
                );
            }
            Err(other) => panic!("DecodeError を期待したが {other:?} だった"),
            Ok(samples) => panic!("DecodeError を期待したが Ok({}) だった", samples.len()),
        }
        Ok(())
    })?;
    Ok(())
}

/// demux したサンプルを比較できる形にしたもの
///
/// `demux::Sample` は `PartialEq` を実装しないため、そのすべてのフィールドを比較できる値として持つ。
/// demuxer の借用を残さないように、参照ではなく所有する値で持つ
#[derive(Debug, PartialEq)]
struct ComparableSample {
    track: TrackInfo,
    sample_entry: Option<SampleEntry>,
    keyframe: bool,
    timestamp: u64,
    duration: u32,
    data_offset: u64,
    data_size: usize,
    composition_time_offset: Option<i64>,
}

/// demux したサンプル 1 個を、比較できる形 [`ComparableSample`] に変換する
fn to_comparable_sample(sample: &shiguredo_mp4::demux::Sample<'_>) -> ComparableSample {
    // `Sample` にフィールドが増えたときに比較漏れがコンパイルエラーになるよう、`..` を使わずに分解する
    let shiguredo_mp4::demux::Sample {
        track,
        sample_entry,
        keyframe,
        timestamp,
        duration,
        data_offset,
        data_size,
        composition_time_offset,
    } = *sample;
    ComparableSample {
        track: track.clone(),
        sample_entry: sample_entry.cloned(),
        keyframe,
        timestamp,
        duration,
        data_offset,
        data_size,
        composition_time_offset,
    }
}

/// demux したサンプルの列を、比較できる形 [`ComparableSample`] の列に変換する
fn to_comparable_samples(samples: &[shiguredo_mp4::demux::Sample<'_>]) -> Vec<ComparableSample> {
    samples.iter().map(to_comparable_sample).collect()
}

/// `handle_media_segment()` がエラーを返す入力の種類
///
/// いずれも、同じ呼び出しでサンプルを 1 つ以上処理した後でエラーになる
#[derive(Debug, Clone, Copy)]
enum InvalidMediaSegmentKind {
    /// `moof` + `mdat` を 2 組連結した入力（`mdat` の後ろに `moof` があるためエラーになる）
    ConcatenatedPairs,
    /// 2 番目の `traf` の track_id を、`moov` に存在しない値に書き換えた入力
    UnknownTrackIdInSecondTraf,
    /// 2 番目の `traf` の sample description index を、`stsd` の範囲外の値に書き換えた入力
    SampleDescriptionIndexOutOfRangeInSecondTraf,
    /// 最後のサンプルのサイズを 1 増やして、`mdat` の範囲を超えさせた入力
    LastSampleExceedsMdat,
    /// 最初の `traf` の `tfdt` を `u64::MAX` にして、最初のサンプルの後でデコード時間を溢れさせた入力
    DecodeTimeOverflow,
}

/// [`InvalidMediaSegmentKind`] のすべての種類
///
/// 種類を追加したときは、ここにも追加する
const INVALID_MEDIA_SEGMENT_KINDS: [InvalidMediaSegmentKind; 5] = [
    InvalidMediaSegmentKind::ConcatenatedPairs,
    InvalidMediaSegmentKind::UnknownTrackIdInSecondTraf,
    InvalidMediaSegmentKind::SampleDescriptionIndexOutOfRangeInSecondTraf,
    InvalidMediaSegmentKind::LastSampleExceedsMdat,
    InvalidMediaSegmentKind::DecodeTimeOverflow,
];

/// [`InvalidMediaSegmentKind::SampleDescriptionIndexOutOfRangeInSecondTraf`] で書き込む sample description index
///
/// どのトラックの `stsd` にもこの数のサンプルエントリーは入らないため、常に範囲外になる
const OUT_OF_RANGE_SAMPLE_DESCRIPTION_INDEX: u32 = u32::MAX;

/// 正しいメディアセグメントから、`kind` の種類のエラーになる入力を作る
///
/// 次の前提を満たすセグメントを渡すこと:
/// - `traf` を 2 つ以上含み、各 `traf` がサンプルを 1 つ以上含む
/// - 最初の `traf` の最初のサンプルの尺が 1 以上である
///
/// `unknown_track_id` は `moov` に存在しない track_id とする
fn build_invalid_media_segment(
    kind: InvalidMediaSegmentKind,
    media_segment: &[u8],
    unknown_track_id: u32,
) -> Vec<u8> {
    // 前提が崩れると、最初のサンプルを処理する前にエラーになることがある。
    // そうなると、状態が変わらないことを確かめても何も検証しないテストになるため、どの種類でも最初に前提を確認する
    let (moof_box, _) =
        MoofBox::decode(media_segment).expect("media セグメントからの moof デコードに失敗した");
    assert!(
        moof_box.traf_boxes.len() >= 2,
        "エラーになる入力の元にするセグメントは traf を 2 つ以上含む"
    );
    for traf_box in &moof_box.traf_boxes {
        assert!(
            traf_box
                .trun_boxes
                .iter()
                .any(|trun_box| !trun_box.samples.is_empty()),
            "エラーになる入力の元にするセグメントの各 traf はサンプルを 1 つ以上含む"
        );
    }
    let first_sample_duration = moof_box.traf_boxes[0]
        .trun_boxes
        .iter()
        .flat_map(|trun_box| &trun_box.samples)
        .next()
        .and_then(|sample| sample.duration)
        .expect("muxer は trun に各サンプルの尺を書く");
    assert!(
        first_sample_duration >= 1,
        "エラーになる入力の元にするセグメントの最初のサンプルの尺は 1 以上である"
    );

    match kind {
        InvalidMediaSegmentKind::ConcatenatedPairs => {
            let mut concatenated = media_segment.to_vec();
            concatenated.extend_from_slice(media_segment);
            concatenated
        }
        InvalidMediaSegmentKind::UnknownTrackIdInSecondTraf => {
            rewrite_media_segment_moof(media_segment, |moof_box| {
                moof_box.traf_boxes[1].tfhd_box.track_id = unknown_track_id;
            })
        }
        InvalidMediaSegmentKind::SampleDescriptionIndexOutOfRangeInSecondTraf => {
            rewrite_media_segment_moof(media_segment, |moof_box| {
                moof_box.traf_boxes[1].tfhd_box.sample_description_index =
                    Some(OUT_OF_RANGE_SAMPLE_DESCRIPTION_INDEX);
            })
        }
        InvalidMediaSegmentKind::LastSampleExceedsMdat => {
            // muxer の出力では、トラックの payload が traf と同じ順に隙間なく並ぶ。
            // そのため、最後の traf の最後のサンプルは mdat の末尾で終わり、サイズを 1 増やすと範囲を超える
            rewrite_media_segment_moof(media_segment, |moof_box| {
                let last_sample = moof_box
                    .traf_boxes
                    .last_mut()
                    .and_then(|traf_box| traf_box.trun_boxes.last_mut())
                    .and_then(|trun_box| trun_box.samples.last_mut())
                    .expect("最後の traf の最後の trun はサンプルを含む");
                let size = last_sample
                    .size
                    .expect("muxer は trun に各サンプルのサイズを書く");
                last_sample.size =
                    Some(size.checked_add(1).expect("サンプルサイズは u32 に収まる"));
            })
        }
        InvalidMediaSegmentKind::DecodeTimeOverflow => {
            // 最初のサンプルの尺は 1 以上なので、最初のサンプルの後でデコード時間の加算が溢れる
            rewrite_media_segment_moof(media_segment, |moof_box| {
                moof_box.traf_boxes[0].tfdt_box = Some(TfdtBox {
                    version: 1,
                    base_media_decode_time: u64::MAX,
                });
            })
        }
    }
}

/// `kind` の種類の入力を渡したときに、エラーの `reason` が始まる文字列を返す
///
/// 書き換えた入力が、狙ったものとは別の理由（サンプルを処理する前の検査など）でエラーになっていないことを確かめるために使う。
/// 別の理由でエラーになると、状態が変わらないことを確かめても、何も検証していないテストになるためである
fn expected_error_reason_prefix(kind: InvalidMediaSegmentKind) -> String {
    match kind {
        InvalidMediaSegmentKind::ConcatenatedPairs => {
            "found moof box after mdat in media segment".to_owned()
        }
        InvalidMediaSegmentKind::UnknownTrackIdInSecondTraf => {
            "unknown track_id in media segment".to_owned()
        }
        InvalidMediaSegmentKind::SampleDescriptionIndexOutOfRangeInSecondTraf => {
            format!(
                "sample_description_index={OUT_OF_RANGE_SAMPLE_DESCRIPTION_INDEX} is out of range"
            )
        }
        InvalidMediaSegmentKind::LastSampleExceedsMdat => {
            "sample data range exceeds mdat boundary".to_owned()
        }
        InvalidMediaSegmentKind::DecodeTimeOverflow => "trun decode time overflow".to_owned(),
    }
}

/// `handle_media_segment()` がエラーを返しても、demuxer の内部状態が変わらないことを確認する
///
/// 映像トラックが sample description index 2 を使うセグメントを、エラーになる形に書き換えて渡す。
/// 渡す前の demuxer は、index 1 のセグメントを処理した後の状態と、初期化した直後の状態の両方を試す。
/// エラーになる入力の種類は [`InvalidMediaSegmentKind`] のとおりである。
/// 次の 2 点を確かめる:
/// - エラーを返した呼び出しの前後で、demuxer の `Debug` 出力（内部状態すべて）が一致する
/// - その後に正しいセグメントを渡した結果が、エラーになる入力を渡さなかった demuxer の結果と一致する
///
/// 処理の途中で内部状態を更新する実装では、エラーの前にサンプルを処理したトラックの index が更新されたまま残る。
/// その結果、正しいセグメントの各トラックの最初のサンプルや、index が切り替わった最初の映像サンプルで
/// `sample_entry` が `None` になり、利用者はサンプルエントリーを受け取れなくなる
#[test]
fn media_segment_error_does_not_change_state() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // 1 つ目のセグメントを処理してからエラーになる入力を渡したケース数と、初期化直後に渡したケース数。
    // 各ケースで 1/2 の確率で選ぶので、`CASES`（256）ケースで片方を一度も通らない確率は 2^-256 程度
    let after_first_segment_cases = std::cell::Cell::new(0usize);
    let right_after_init_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        // 映像と音声のサンプルをどちらも 1 つ以上にする。
        // `build_invalid_media_segment` の前提を満たすためである
        let first_video_samples = sample_vec(ctx, 1..4, |ctx| arb_video_sample(ctx, 0));
        let first_audio_samples = sample_vec(ctx, 1..4, |ctx| arb_audio_sample(ctx, 1));
        let second_video_samples = sample_vec(ctx, 1..4, |ctx| arb_video_sample(ctx, 0));
        let second_audio_samples = sample_vec(ctx, 1..4, |ctx| arb_audio_sample(ctx, 1));
        // エラーになる入力の前に、1 つ目のセグメント（映像の index が 1）を処理しておくかどうか。
        // 処理しない場合は、初期化直後の demuxer にエラーになる入力を渡す
        let handle_first_segment = noprop::sample_bool(ctx);

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let audio_sample_entry = create_opus_sample_entry();
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        // 映像、音声の順に traf と payload が並ぶセグメントを組み立てる
        let mut build_segment = |video_sample_entry: &SampleEntry,
                                 video_samples: &[TestSample],
                                 audio_samples: &[TestSample]| {
            let mut segment_samples = Vec::new();
            let mut payloads = Vec::new();
            for sample in video_samples {
                segment_samples.push(video_segment_sample(video_sample_entry, sample, None));
                payloads.push(sample.data.as_slice());
            }
            for sample in audio_samples {
                segment_samples.push(audio_segment_sample(&audio_sample_entry, sample));
                payloads.push(sample.data.as_slice());
            }
            build_complete_media_segment(&mut muxer, &segment_samples, &payloads)
        };
        let first_segment = build_segment(
            &original_sample_entry,
            &first_video_samples,
            &first_audio_samples,
        );
        let second_segment = build_segment(
            &alternative_sample_entry,
            &second_video_samples,
            &second_audio_samples,
        );
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        // エラーになる入力を渡さなかった場合の結果を期待値にする
        let mut reference_demuxer = Fmp4SegmentDemuxer::new();
        reference_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        if handle_first_segment {
            reference_demuxer
                .handle_media_segment(&first_segment)
                .expect("1 つ目の media セグメントの処理に失敗した");
        }
        let expected = to_comparable_samples(
            &reference_demuxer
                .handle_media_segment(&second_segment)
                .expect("2 つ目の media セグメントの処理に失敗した"),
        );
        // 最初の映像サンプルで、index 2 のサンプルエントリーが返ることを前提にする
        assert_eq!(
            expected[0].sample_entry.as_ref(),
            Some(&alternative_sample_entry),
            "2 つ目のセグメントの最初の映像サンプルで index 2 のサンプルエントリーが返らない"
        );

        let unknown_track_id = reference_demuxer
            .tracks()
            .expect("init セグメントの処理後はトラック情報を取得できる")
            .iter()
            .map(|track| track.track_id)
            .max()
            .expect("トラックが存在する")
            .checked_add(1)
            .expect("track_id は u32 に収まる");

        for kind in INVALID_MEDIA_SEGMENT_KINDS {
            let invalid_segment =
                build_invalid_media_segment(kind, &second_segment, unknown_track_id);

            let mut demuxer = Fmp4SegmentDemuxer::new();
            demuxer
                .handle_init_segment(&init_bytes)
                .expect("init セグメントの処理に失敗した");
            if handle_first_segment {
                demuxer
                    .handle_media_segment(&first_segment)
                    .expect("1 つ目の media セグメントの処理に失敗した");
            }

            let state_before_error = format!("{demuxer:?}");
            // 成功時の戻り値は demuxer を借用し続けるため、サンプル数に変換して借用を終わらせる。
            // こうしないと、この後で demuxer の `Debug` 出力を取得できない
            let result = demuxer
                .handle_media_segment(&invalid_segment)
                .map(|samples| samples.len());
            let Err(DemuxError::DecodeError(error)) = &result else {
                panic!("{kind:?} の入力がデコードエラーにならなかった: {result:?}");
            };
            let expected_reason_prefix = expected_error_reason_prefix(kind);
            assert!(
                error.reason.starts_with(&expected_reason_prefix),
                "{kind:?} の入力が狙った理由でエラーにならなかった: {}",
                error.reason
            );
            assert_eq!(
                format!("{demuxer:?}"),
                state_before_error,
                "{kind:?} の入力でエラーを返した後に内部状態が変わった"
            );

            let actual = to_comparable_samples(
                &demuxer
                    .handle_media_segment(&second_segment)
                    .expect("エラーの後の media セグメントの処理に失敗した"),
            );
            assert_eq!(
                actual, expected,
                "{kind:?} の入力でエラーを返した後の demux 結果が、エラーを経ない場合と一致しない"
            );
        }

        if handle_first_segment {
            after_first_segment_cases.set(after_first_segment_cases.get() + 1);
        } else {
            right_after_init_cases.set(right_after_init_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        after_first_segment_cases.get() > 0,
        "1 つ目のセグメントを処理してからエラーになる入力を渡したケースが 1 つもなかった\n{runner}"
    );
    assert!(
        right_after_init_cases.get() > 0,
        "初期化直後にエラーになる入力を渡したケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// `moof` にある唯一の `traf` を、同じトラックの 2 つの `traf` に分ける
///
/// 先頭から `split_at` 個のサンプルを 1 番目の `traf` に、残りを 2 番目の `traf` に入れる。
/// それぞれの `tfhd.sample_description_index` は `sample_description_indices` の値にする。
/// 2 番目の `traf` の `tfdt` と `trun` の `data_offset` は、1 番目の `traf` のサンプルの尺とサイズの合計だけ進める。
/// `data_offset` は書き換える前の `moof` の先頭を基準にした値のままにする。
/// `traf` の追加や、32 ビットに収まらない `tfdt` の値による version 1 への変化で `moof` のサイズが変わっても、
/// その差は [`rewrite_media_segment_moof`] が補正する
fn split_single_traf(
    moof_box: &mut MoofBox,
    split_at: usize,
    sample_description_indices: [u32; 2],
) {
    assert_eq!(
        moof_box.traf_boxes.len(),
        1,
        "分ける前の moof は traf を 1 つだけ含む"
    );
    let mut first_traf_box = moof_box.traf_boxes.remove(0);
    assert_eq!(
        first_traf_box.trun_boxes.len(),
        1,
        "muxer は traf ごとに trun を 1 つ出力する"
    );

    let second_samples = first_traf_box.trun_boxes[0].samples.split_off(split_at);
    let first_samples = &first_traf_box.trun_boxes[0].samples;
    let first_duration: u64 = first_samples
        .iter()
        .map(|sample| {
            u64::from(
                sample
                    .duration
                    .expect("muxer は trun に各サンプルの尺を書く"),
            )
        })
        .sum();
    let first_size: i64 = first_samples
        .iter()
        .map(|sample| {
            i64::from(
                sample
                    .size
                    .expect("muxer は trun に各サンプルのサイズを書く"),
            )
        })
        .sum();

    let mut second_traf_box = first_traf_box.clone();
    second_traf_box.trun_boxes[0].samples = second_samples;
    let first_data_offset = first_traf_box.trun_boxes[0]
        .data_offset
        .expect("muxer は trun に data_offset を書く");
    second_traf_box.trun_boxes[0].data_offset = Some(
        i32::try_from(i64::from(first_data_offset) + first_size)
            .expect("2 番目の traf の data_offset は i32 に収まる"),
    );
    let second_tfdt_box = second_traf_box
        .tfdt_box
        .as_mut()
        .expect("muxer は traf に tfdt を出力する");
    second_tfdt_box.base_media_decode_time = second_tfdt_box
        .base_media_decode_time
        .checked_add(first_duration)
        .expect("2 番目の traf のデコード時間は u64 に収まる");

    first_traf_box.tfhd_box.sample_description_index = Some(sample_description_indices[0]);
    second_traf_box.tfhd_box.sample_description_index = Some(sample_description_indices[1]);
    moof_box.traf_boxes = vec![first_traf_box, second_traf_box];
}

/// 同じ `moof` に同じトラックの `traf` が 2 つある場合も、
/// sample description index が直前の値から変わったサンプルでだけ `sample_entry` が `Some` になることを確認する
///
/// ISO/IEC 14496-12:2022 の 8.8.6.1 は、1 つの `moof` に同じトラックの `traf` を複数置くことを認めている。
/// 2 番目の `traf` は、同じ呼び出しで先に処理した `traf` の sample description index と比べる必要がある。
/// muxer はトラックごとに `traf` を 1 つしか出力しないため、`moof` を書き換えて `traf` を 2 つに分ける。
///
/// 直前のセグメントの有無とその sample description index、2 つの `traf` の sample description index の
/// すべての組み合わせを試す。期待値は、「直前の値と異なるときだけ通知する」という規則から実装と独立に求める
#[test]
fn sample_entry_emission_with_split_trafs_of_same_track() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let samples = sample_vec(ctx, 2..6, |ctx| arb_video_sample(ctx, 0));
        // 2 つの traf がどちらもサンプルを 1 つ以上含むように分ける
        let split_at = noprop::sample_usize_in(ctx, 1..samples.len());

        // sample description index 1 と 2 に対応するサンプルエントリー
        let sample_entries = [
            create_avc1_sample_entry(width1, 240),
            create_avc1_sample_entry(width2, 240),
        ];
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let segment_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entries[0], sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let base_segment = build_complete_media_segment(&mut muxer, &segment_samples, &payloads);
        // init セグメントの stsd に 2 つ目のサンプルエントリーを登録するためだけに使う
        let _ = build_complete_media_segment(
            &mut muxer,
            &[video_segment_sample(&sample_entries[1], &samples[0], None)],
            &[samples[0].data.as_slice()],
        );
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        for previous_index in [None, Some(1u32), Some(2)] {
            // 直前に処理しておくセグメント。`previous_index` が None のときは何も処理しない
            let previous_segment = previous_index.map(|index| {
                rewrite_media_segment_moof(&base_segment, |moof_box| {
                    moof_box.traf_boxes[0].tfhd_box.sample_description_index = Some(index);
                })
            });

            for traf_indices in [[1u32, 1], [1, 2], [2, 1], [2, 2]] {
                let mut demuxer = Fmp4SegmentDemuxer::new();
                demuxer
                    .handle_init_segment(&init_bytes)
                    .expect("init セグメントの処理に失敗した");
                if let Some(previous_segment) = &previous_segment {
                    demuxer
                        .handle_media_segment(previous_segment)
                        .expect("直前の media セグメントの処理に失敗した");
                }

                let split_segment = rewrite_media_segment_moof(&base_segment, |moof_box| {
                    split_single_traf(moof_box, split_at, traf_indices);
                });
                let demuxed = demuxer
                    .handle_media_segment(&split_segment)
                    .expect("traf を分けた media セグメントの処理に失敗した");
                assert_eq!(demuxed.len(), samples.len(), "サンプル数が一致しない");
                // traf を分けても、各サンプルが元の payload を指していることを確認する
                for (demuxed_sample, sample) in demuxed.iter().zip(&samples) {
                    let start = demuxed_sample.data_offset as usize;
                    assert_eq!(
                        &split_segment[start..start + demuxed_sample.data_size],
                        sample.data.as_slice(),
                        "traf を分けたセグメントのサンプルが元の payload を指していない"
                    );
                }

                // sample description index は traf ごとに決まるため、`sample_entry` が `Some` になり得るのは
                // 各 traf の最初のサンプルだけである。
                // 直前の index（最初の traf では直前のセグメントの index、2 番目の traf では最初の traf の index）と
                // 異なるときだけ、その index のサンプルエントリーが返る
                let mut expected: Vec<Option<&SampleEntry>> = vec![None; samples.len()];
                let mut current_index = previous_index;
                for (first_sample_position, index) in
                    [(0, traf_indices[0]), (split_at, traf_indices[1])]
                {
                    if current_index != Some(index) {
                        expected[first_sample_position] =
                            Some(&sample_entries[index as usize - 1]);
                    }
                    current_index = Some(index);
                }
                let actual: Vec<Option<&SampleEntry>> =
                    demuxed.iter().map(|sample| sample.sample_entry).collect();
                assert_eq!(
                    actual, expected,
                    "直前の index が {previous_index:?}、2 つの traf の index が {traf_indices:?} のときの sample_entry が期待と異なる"
                );
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// Fmp4FileDemuxer でも sample entry の切り替わりが反映されることを確認する
#[test]
fn fmp4_file_demuxer_propagates_sample_entry_changes() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let first_segment_samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));
        let second_segment_samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let alternative_sample_entry = create_avc1_sample_entry(width2, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let first_segment_input: Vec<Sample> = first_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let second_segment_input: Vec<Sample> = second_segment_samples
            .iter()
            .map(|sample| video_segment_sample(&alternative_sample_entry, sample, None))
            .collect();

        let first_payloads: Vec<&[u8]> = first_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let second_payloads: Vec<&[u8]> = second_segment_samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let first_segment =
            build_complete_media_segment(&mut muxer, &first_segment_input, &first_payloads);
        let second_segment =
            build_complete_media_segment(&mut muxer, &second_segment_input, &second_payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        let mut file_data = init_bytes;
        file_data.extend_from_slice(&first_segment);
        file_data.extend_from_slice(&second_segment);

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        let mut sample_entry_flags = Vec::new();
        loop {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break Some(sample),
                    Ok(None) => break None,
                    Err(DemuxError::InputRequired(_)) => {
                        feed_fmp4_file_demuxer(&mut demuxer, &file_data)
                    }
                    Err(error) => panic!("next_sample エラー: {error}"),
                }
            };

            let Some(sample) = sample else {
                break;
            };
            let sample_entry = sample.sample_entry.cloned();
            sample_entry_flags.push(sample_entry);
        }

        assert_eq!(
            sample_entry_flags.len(),
            first_segment_samples.len() + second_segment_samples.len()
        );
        assert_eq!(sample_entry_flags[0].as_ref(), Some(&original_sample_entry));
        for sample_entry in sample_entry_flags
            .iter()
            .take(first_segment_samples.len())
            .skip(1)
        {
            assert!(sample_entry.is_none());
        }
        assert_eq!(
            sample_entry_flags[first_segment_samples.len()].as_ref(),
            Some(&alternative_sample_entry),
        );
        for sample_entry in sample_entry_flags
            .iter()
            .skip(first_segment_samples.len() + 1)
        {
            assert!(sample_entry.is_none());
        }
        Ok(())
    })?;
    Ok(())
}

/// 範囲外の sample description index はエラーになることを確認する
#[test]
fn invalid_sample_description_index_is_rejected() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width1 = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let width2 = sample_distinct_width(ctx, width1);
        let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));

        let original_sample_entry = create_avc1_sample_entry(width1, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let _ = build_complete_media_segment(
            &mut muxer,
            &[video_segment_sample(
                &create_avc1_sample_entry(width2, 240),
                &samples[0],
                None,
            )],
            &[samples[0].data.as_slice()],
        );
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let init_bytes = append_sample_entry_and_set_trex_default(
            &init_bytes,
            create_avc1_sample_entry(width2, 240),
            1,
        );

        let fmp4_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&original_sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let media_segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let media_segment =
            rewrite_media_segment_tfhd_sample_description_index(&media_segment, Some(3));

        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let result = demuxer.handle_media_segment(&media_segment);

        assert!(matches!(result, Err(DemuxError::DecodeError(_))));
        Ok(())
    })?;
    Ok(())
}

/// Fmp4FileDemuxer が mux したファイルを正しく demux できることを確認する
#[test]
fn fmp4_file_demuxer_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let segments = sample_vec(ctx, 1..4, |ctx| {
            sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0))
        });

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        // 全セグメントをひとつのバイト列に連結して完全な fMP4 ファイルを組み立てる
        let mut file_data = Vec::new();
        let mut all_samples: Vec<TestSample> = Vec::new();

        for segment_samples in &segments {
            let fmp4_samples: Vec<Sample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = segment_samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
            all_samples.extend_from_slice(segment_samples);
            file_data.extend_from_slice(&segment_bytes);
        }
        let mut init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        init_bytes.extend_from_slice(&file_data);
        let file_data = init_bytes;

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        // トラック情報の確認
        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].kind, TrackKind::Video);
        assert_eq!(tracks[0].timescale.get(), 90000);

        // サンプルを順番に取り出して元データと照合する
        let mut expected_decode_time: u64 = 0;
        for (i, orig) in all_samples.iter().enumerate() {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break sample,
                    Ok(None) => panic!("sample が予期せず終端した"),
                    Err(DemuxError::InputRequired(_)) => {
                        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
                    }
                    Err(error) => panic!("next_sample エラー: {error}"),
                }
            };

            assert_eq!(sample.track.track_id, 1);
            assert_eq!(sample.timestamp, expected_decode_time);
            assert_eq!(sample.duration, orig.duration);
            assert_eq!(sample.keyframe, orig.keyframe);
            assert_eq!(
                &file_data
                    [sample.data_offset as usize..sample.data_offset as usize + sample.data_size],
                orig.data.as_slice(),
            );
            assert_eq!(sample.sample_entry.is_some(), i == 0);

            expected_decode_time += orig.duration as u64;
        }

        // 全サンプルを読み終えたら None が返ることを確認する
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
        let last = demuxer.next_sample().expect("next_sample エラー");
        assert!(
            last.is_none(),
            "これ以上 sample は無い想定だが {last:?} が返った"
        );
        Ok(())
    })?;
    Ok(())
}

/// `mdat size=0` のメディアセグメントを含む fMP4 ファイルでも
/// `Fmp4FileDemuxer` が末尾までの `mdat` として処理できることを確認する
#[test]
fn fmp4_file_demuxer_accepts_mdat_size_zero() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let width = noprop::sample_u64_in(ctx, 64..1921) as u16;
        let height = noprop::sample_u64_in(ctx, 64..1081) as u16;
        let samples = sample_vec(ctx, 1..10, |ctx| arb_video_sample(ctx, 0));

        let sample_entry = create_avc1_sample_entry(width, height);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let segment_samples: Vec<Sample> = samples
            .iter()
            .map(|sample| video_segment_sample(&sample_entry, sample, None))
            .collect();
        let payloads: Vec<&[u8]> = samples
            .iter()
            .map(|sample| sample.data.as_slice())
            .collect();
        let media_segment = build_complete_media_segment(&mut muxer, &segment_samples, &payloads);
        let media_segment = rewrite_media_segment_mdat_size_zero(&media_segment);

        let mut file_data = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        file_data.extend_from_slice(&media_segment);

        let mut demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut demuxer, &file_data);

        let tracks = demuxer.tracks().expect("tracks の取得に失敗した");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].kind, TrackKind::Video);

        let mut expected_decode_time = 0u64;
        for (i, expected) in samples.iter().enumerate() {
            let sample = loop {
                match demuxer.next_sample() {
                    Ok(Some(sample)) => break sample,
                    Ok(None) => panic!("sample が予期せず終端した"),
                    Err(DemuxError::InputRequired(_)) => {
                        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
                    }
                    Err(error) => panic!("next_sample エラー: {error}"),
                }
            };

            assert_eq!(sample.track.track_id, 1);
            assert_eq!(sample.timestamp, expected_decode_time);
            assert_eq!(sample.duration, expected.duration);
            assert_eq!(sample.keyframe, expected.keyframe);
            assert_eq!(
                &file_data
                    [sample.data_offset as usize..sample.data_offset as usize + sample.data_size],
                expected.data.as_slice(),
            );
            assert_eq!(sample.sample_entry.is_some(), i == 0);
            expected_decode_time += expected.duration as u64;
        }

        feed_fmp4_file_demuxer(&mut demuxer, &file_data);
        let last = demuxer.next_sample().expect("next_sample エラー");
        assert!(
            last.is_none(),
            "これ以上 sample は無い想定だが {last:?} が返った"
        );
        Ok(())
    })?;
    Ok(())
}

/// `Fmp4FileDemuxer` が、`moof` と `mdat` の間、および `mdat` の後ろにボックスがあるファイルを処理できることを確認する
///
/// メディアセグメントを 2 つ以上連結したファイルを使う。
/// `mdat` の後ろに置いたボックスを読み飛ばして、次の `moof` まで進めることを確かめるためである。
/// `data_offset` はファイル先頭からの位置なので、各サンプルの `data_offset` は
/// ファイル上でそのサンプルのデータより前に置いたボックスの合計サイズ分ずれる。
/// 前のメディアセグメントの `mdat` の後ろに置いたボックスも、次のメディアセグメントのサンプルより前にある
#[test]
fn fmp4_file_demuxer_skips_boxes_between_moof_and_mdat() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // `moof` と `mdat` の間にボックスを置いたセグメントを含むケース数。空でない確率は 2/3
    let with_between_cases = std::cell::Cell::new(0usize);
    // `mdat` の後ろにボックスを置いたセグメントを含むケース数。空でない確率は 2/3
    let with_after_cases = std::cell::Cell::new(0usize);
    // `mdat` の size が 0 で、かつ `moof` と `mdat` の間にボックスを置いたセグメントを含むケース数。
    // 最後のセグメントが選ばれる確率は 1、size=0 は 1/2、間のボックスが空でない確率は 2/3
    let mdat_size_zero_with_between_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        // メディアセグメントを 2 つ以上にする
        let segment_count = noprop::sample_usize_in(ctx, 2..4);
        // 最後のメディアセグメントの `mdat` の size を 0 にするかどうか。
        // `mdat` がファイルの末尾まで続くため、この場合は `mdat` の後ろにボックスを置けない
        let with_mdat_size_zero_in_last_segment = noprop::sample_bool(ctx);

        let mut reference_segments = Vec::new();
        let mut inserted_segments = Vec::new();
        // 各セグメントのサンプルの `data_offset` のずれ
        let mut segment_shifts = Vec::new();
        // 各セグメントのサンプル数
        let mut segment_sample_counts = Vec::new();
        // 前のセグメントまでに置いたボックスの合計サイズ
        let mut accumulated_shift = 0u64;

        for segment_index in 0..segment_count {
            let samples = sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0));
            let fmp4_samples: Vec<Sample> = samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let mut segment_bytes =
                build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);

            // 最後のセグメントの `mdat` の size を 0 にすると、`mdat` がファイルの末尾まで続く。
            // `mdat` と読み飛ばしの組み合わせで `mdat` の位置の扱いが崩れないことを確認する
            let mdat_size_zero =
                segment_index + 1 == segment_count && with_mdat_size_zero_in_last_segment;
            if mdat_size_zero {
                segment_bytes = rewrite_media_segment_mdat_size_zero(&segment_bytes);
            }

            // 間に置くボックスからは `mdat` を外す。`mdat` を置くと、そこが `mdat` として扱われる
            let between_boxes = sample_vec(ctx, 0..3, |ctx| arb_skippable_box(ctx, &[*b"mdat"]));
            // 後ろに置くボックスは `mdat` でもよい。`mdat` の後ろの `mdat` も読み飛ばしの対象である
            let after_boxes = sample_vec(ctx, 0..3, |ctx| arb_skippable_box(ctx, &[]));
            let mut between_bytes = Vec::new();
            for (box_bytes, _large_size) in &between_boxes {
                between_bytes.extend_from_slice(box_bytes);
            }
            let mut after_bytes = Vec::new();
            if !mdat_size_zero {
                for (box_bytes, _large_size) in &after_boxes {
                    after_bytes.extend_from_slice(box_bytes);
                }
            }
            if mdat_size_zero && !between_bytes.is_empty() {
                mdat_size_zero_with_between_cases
                    .set(mdat_size_zero_with_between_cases.get() + 1);
            }

            // muxer の出力は `default_base_is_moof = true` なので、すべての `traf` の `data_offset` を増やす
            let mut inserted_segment =
                insert_boxes_between_moof_and_mdat(&segment_bytes, &between_bytes, true);
            inserted_segment.extend_from_slice(&after_bytes);

            segment_shifts.push(accumulated_shift + between_bytes.len() as u64);
            segment_sample_counts.push(samples.len());
            reference_segments.push(segment_bytes);
            inserted_segments.push(inserted_segment);
            accumulated_shift += between_bytes.len() as u64 + after_bytes.len() as u64;

            if !between_bytes.is_empty() {
                with_between_cases.set(with_between_cases.get() + 1);
            }
            if !after_bytes.is_empty() {
                with_after_cases.set(with_after_cases.get() + 1);
            }
        }

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let mut reference_file = init_bytes.clone();
        let mut inserted_file = init_bytes.clone();
        for segment_bytes in &reference_segments {
            reference_file.extend_from_slice(segment_bytes);
        }
        for segment_bytes in &inserted_segments {
            inserted_file.extend_from_slice(segment_bytes);
        }

        // サンプルごとの `data_offset` のずれ。セグメント内では同じ値になる
        let mut expected_shift_per_sample = Vec::new();
        for (segment_index, sample_count) in segment_sample_counts.iter().enumerate() {
            for _ in 0..*sample_count {
                expected_shift_per_sample.push(segment_shifts[segment_index]);
            }
        }

        let mut reference_demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut reference_demuxer, &reference_file);
        let reference_samples = collect_comparable_samples(&mut reference_demuxer, &reference_file);

        let mut actual_demuxer = Fmp4FileDemuxer::new();
        feed_fmp4_file_demuxer(&mut actual_demuxer, &inserted_file);
        let actual_samples = collect_comparable_samples(&mut actual_demuxer, &inserted_file);

        assert_eq!(
            expected_shift_per_sample.len(),
            reference_samples.len(),
            "ボックスを置かないファイルのサンプル数が、組み立てたセグメントのサンプル数と一致しない"
        );
        assert_eq!(
            actual_samples.len(),
            reference_samples.len(),
            "ボックスを置いてもサンプル数は変わらない"
        );

        for (index, (actual, expected)) in actual_samples
            .iter()
            .zip(reference_samples.iter())
            .enumerate()
        {
            assert_eq!(
                (
                    &actual.track,
                    &actual.sample_entry,
                    actual.keyframe,
                    actual.timestamp,
                    actual.duration,
                    actual.data_size,
                    actual.composition_time_offset,
                ),
                (
                    &expected.track,
                    &expected.sample_entry,
                    expected.keyframe,
                    expected.timestamp,
                    expected.duration,
                    expected.data_size,
                    expected.composition_time_offset,
                ),
                "{index} 番目のサンプルの data_offset 以外のフィールドは、ボックスを置いても変わらない"
            );
            assert_eq!(
                actual.data_offset,
                expected.data_offset + expected_shift_per_sample[index],
                "{index} 番目のサンプルの data_offset は、ファイル上で前に置いたボックスの合計サイズ分だけずれる"
            );

            // ずれた先に元のサンプルデータがある
            let actual_start = actual.data_offset as usize;
            let expected_start = expected.data_offset as usize;
            assert_eq!(
                &inserted_file[actual_start..actual_start + actual.data_size],
                &reference_file[expected_start..expected_start + expected.data_size],
                "{index} 番目のサンプルのずれた先に元のサンプルデータがない"
            );
        }
        Ok(())
    })?;

    assert!(
        with_between_cases.get() > 0,
        "moof と mdat の間にボックスを置いたセグメントが 1 つもなかった\n{runner}"
    );
    assert!(
        with_after_cases.get() > 0,
        "mdat の後ろにボックスを置いたセグメントが 1 つもなかった\n{runner}"
    );
    assert!(
        mdat_size_zero_with_between_cases.get() > 0,
        "mdat の size を 0 にして、かつ間にボックスを置いたセグメントが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// `Fmp4FileDemuxer` に要求された範囲だけを渡した場合と、常に位置 0 からファイル全体を渡した場合で、
/// 取り出せるサンプル列と最後の `next_sample()` の結果が一致することを確認する
///
/// 次のファイルを含める。
///
/// - メディアセグメントが 1 つのファイルと 2 つ以上のファイル
/// - 最後の `mdat` が 32 バイト未満のファイル
/// - 最後の `mdat` の後ろにボックスがあるファイル
/// - 最後の `mdat` の size が 0 のファイル（`segment_size` が `None` になり、切り詰めを通らない経路になる）
/// - `moof` と `mdat` の間にボックスがあるファイル
///
/// さらに、ファイルの途中で切れた入力も使う。切る位置は `moof` と `mdat` の範囲に限る。
/// 読み飛ばすボックスの、サイズを読める位置より後ろで切ると、次の要求位置が入力の終端より後ろになり、
/// 入力の終端をファイルの終端とみなせない場合に当たるためである
#[test]
fn fmp4_file_demuxer_accepts_whole_file_input() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // メディアセグメントが 1 つのケース数と 2 つ以上のケース数。
    // `segment_count` は 1 が 1/3、2 以上が 2/3 の確率なので、`CASES`（256）ケースで両方を通る
    let single_segment_cases = std::cell::Cell::new(0usize);
    let multi_segment_cases = std::cell::Cell::new(0usize);
    // 最後の `mdat` が 32 バイト未満になるケース数。各ケースで 1/2 の確率で選ぶ
    let small_last_mdat_cases = std::cell::Cell::new(0usize);
    // 最後の `mdat` の後ろにボックスを置いたケース数。各ケースで 1/2 の確率で選ぶ
    let with_trailing_box_cases = std::cell::Cell::new(0usize);
    // 最後の `mdat` の size を 0 にしたケース数。後ろにボックスを置かない場合の 1/2 の確率で選ぶ
    let with_mdat_size_zero_cases = std::cell::Cell::new(0usize);
    // `moof` と `mdat` の間にボックスを置いたケース数。1 つ以上のセグメントで置かれる確率は 3/4 以上
    let with_between_box_cases = std::cell::Cell::new(0usize);
    // ファイルの途中で切った入力を使ったケース数と、切らなかったケース数。各ケースで 1/2 の確率で選ぶ
    let truncated_cases = std::cell::Cell::new(0usize);
    let whole_file_cases = std::cell::Cell::new(0usize);
    // `moof` または `mdat` の途中で切ったケース数。セグメントの内側を選ぶ確率は 1/2 以上
    let inside_segment_truncated_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let segment_count = noprop::sample_usize_in(ctx, 1..4);
        let small_last_mdat = noprop::sample_bool(ctx);
        let with_trailing_box = noprop::sample_bool(ctx);
        // 最後の `mdat` の size を 0 にすると `mdat` がファイルの末尾まで続くため、
        // 後ろにボックスを置く場合は使わない（サイズを読めない位置にボックスが入るため）
        let with_mdat_size_zero = !with_trailing_box && noprop::sample_bool(ctx);

        // init セグメントを除いたファイル本体と、切る位置の候補。
        // 候補は (セグメントの先頭, セグメントの末尾, 切る範囲の先頭, 切る範囲の末尾)
        let mut file_body = Vec::new();
        let mut truncatable_ranges = Vec::new();
        // 全セグメントのサンプル数の合計
        let mut total_sample_count = 0usize;
        // `moof` と `mdat` の間にボックスを置いたかどうか
        let mut with_between_box = false;

        for segment_index in 0..segment_count {
            let is_last = segment_index + 1 == segment_count;
            let samples = if is_last && small_last_mdat {
                // 最後の `mdat` が 32 バイト未満になるように、payload を短くする
                sample_vec(ctx, 1..3, |ctx| {
                    let mut sample = arb_video_sample(ctx, 0);
                    let data_len = noprop::sample_usize_in(ctx, 1..8);
                    sample.data = noprop::sample_bytes_vec(ctx, data_len);
                    sample
                })
            } else {
                sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0))
            };
            let fmp4_samples: Vec<Sample> = samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let mut segment_bytes =
                build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
            let (_moof_box, moof_size) = MoofBox::decode(&segment_bytes)
                .expect("生成したセグメントの moof デコードに失敗した");

            // 最後のセグメントでは、`mdat` の宣言サイズが 32 バイト未満になっていることを確かめる。
            // フラグだけで数えると、payload の長さを変えたときに検証対象が消えても気づけない
            if is_last && small_last_mdat {
                let mdat_size = u32::from_be_bytes(
                    segment_bytes[moof_size..moof_size + 4]
                        .try_into()
                        .expect("mdat のサイズフィールドは 4 バイト"),
                );
                assert!(
                    mdat_size < 32,
                    "最後の mdat が 32 バイト未満になっていない: {mdat_size}"
                );
            }

            // 最後のセグメントの `mdat` の size を 0 にする。
            // この場合 `segment_size` が `None` になり、`available_bytes` の切り詰めを通らない経路になる
            if is_last && with_mdat_size_zero {
                segment_bytes = rewrite_media_segment_mdat_size_zero(&segment_bytes);
            }

            // メディアセグメントの先頭に、`moof` と `mdat` の間に置くボックスを挿し込む。
            // セグメントの範囲は読み飛ばしたボックスを含むため、位置 0 からファイル全体を渡した場合に
            // 切り詰めが読み飛ばし分を切らないことを確認できる
            let between_boxes = sample_vec(ctx, 0..3, |ctx| arb_skippable_box(ctx, &[*b"mdat"]));
            let mut between_bytes = Vec::new();
            for (box_bytes, _large_size) in &between_boxes {
                between_bytes.extend_from_slice(box_bytes);
            }
            with_between_box |= !between_bytes.is_empty();
            let segment_bytes =
                insert_boxes_between_moof_and_mdat(&segment_bytes, &between_bytes, true);

            let start = file_body.len();
            file_body.extend_from_slice(&segment_bytes);
            let segment_end = file_body.len();
            // 切る位置は `moof` と `mdat` の範囲に限る。
            // 間に置いたボックスの内部で切ると、そのボックスの宣言サイズだけ読み飛ばした先が
            // 入力の終端より後ろになり、入力の終端をファイルの終端とみなせない場合に当たる
            truncatable_ranges.push((start, segment_end, start, start + moof_size));
            truncatable_ranges.push((
                start,
                segment_end,
                start + moof_size + between_bytes.len(),
                segment_end,
            ));
            total_sample_count += samples.len();

            // 最後のメディアセグメントの `mdat` の後ろに、読み飛ばされるボックスを置く
            if is_last && with_trailing_box {
                let (box_bytes, _large_size) = arb_skippable_box(ctx, &[*b"moof", *b"mdat"]);
                file_body.extend_from_slice(&box_bytes);
            }
        }

        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");
        let mut file_data = init_bytes.clone();
        file_data.extend_from_slice(&file_body);
        // 切る位置の候補を、ファイル先頭からの位置に直す
        let truncatable_ranges: Vec<(usize, usize, usize, usize)> = truncatable_ranges
            .into_iter()
            .map(|(segment_start, segment_end, start, end)| {
                let offset = init_bytes.len();
                (
                    offset + segment_start,
                    offset + segment_end,
                    offset + start,
                    offset + end,
                )
            })
            .collect();

        // ファイルの途中で切るかどうか。
        // セグメントの境界で切った場合は完全なメディアセグメントまでしか含まないため、
        // セグメントの内側（`moof` または `mdat` の途中）で切ったかどうかも覚えておく。
        // セグメントの先頭で切るのはファイルがそのセグメントの直前で終わる場合なので、内側には含めない
        let truncated = noprop::sample_bool(ctx);
        let mut truncated_inside_segment = false;
        let input_data = if truncated {
            let (segment_start, segment_end, start, end) =
                noprop::sample_choice(ctx, &truncatable_ranges);
            let truncated_len = noprop::sample_usize_in(ctx, start..end + 1);
            truncated_inside_segment = segment_start < truncated_len && truncated_len < segment_end;
            truncated_cases.set(truncated_cases.get() + 1);
            file_data[..truncated_len].to_vec()
        } else {
            whole_file_cases.set(whole_file_cases.get() + 1);
            file_data
        };

        // 要求された範囲だけを渡した場合と、常に位置 0 からファイル全体を渡した場合を比べる
        let expected = collect_demux_result(&input_data, feed_fmp4_file_demuxer);
        let actual = collect_demux_result(&input_data, feed_fmp4_file_demuxer_with_whole_file);
        assert_eq!(
            actual, expected,
            "位置 0 からファイル全体を渡した結果が、要求された範囲だけを渡した結果と一致しない"
        );

        if !truncated {
            // 切っていないファイルでは、全サンプルが取り出せてファイルの終端に達する。
            // 比較元と比較先がどちらも誤っていて一致する、という偽の成功を防ぐ
            assert_eq!(
                expected.1,
                ComparableFinalResult::EndOfSamples,
                "切っていないファイルでファイルの終端に達しなかった"
            );
            assert_eq!(
                expected.0.len(),
                total_sample_count,
                "切っていないファイルで全サンプルを取り出せなかった"
            );
        } else if truncated_inside_segment {
            // `moof` または `mdat` の途中で切った場合は、その範囲をデコードできずにエラーになる。
            // 両方の供給方法が同じ誤りで一致する偽の成功を防ぐ
            assert!(
                matches!(expected.1, ComparableFinalResult::Error(_)),
                "moof または mdat の途中で切った入力でエラーにならなかった: {:?}",
                expected.1
            );
            inside_segment_truncated_cases.set(inside_segment_truncated_cases.get() + 1);
        }

        // 各ケースは切っていない場合にだけ数える。
        // 切っていない場合は、そのファイルで全サンプルが取り出せて `Ok(None)` になることを確かめており、
        // 「そのファイルで正しく終端まで読める」ことの検証と組み合わせるためである
        if !truncated {
            if segment_count == 1 {
                single_segment_cases.set(single_segment_cases.get() + 1);
            } else {
                multi_segment_cases.set(multi_segment_cases.get() + 1);
            }
            if small_last_mdat {
                small_last_mdat_cases.set(small_last_mdat_cases.get() + 1);
            }
            if with_trailing_box {
                with_trailing_box_cases.set(with_trailing_box_cases.get() + 1);
            }
            if with_mdat_size_zero {
                with_mdat_size_zero_cases.set(with_mdat_size_zero_cases.get() + 1);
            }
            if with_between_box {
                with_between_box_cases.set(with_between_box_cases.get() + 1);
            }
        }
        Ok(())
    })?;

    assert!(
        single_segment_cases.get() > 0,
        "切っていない、メディアセグメントが 1 つのファイルのケースが 1 つもなかった\n{runner}"
    );
    assert!(
        multi_segment_cases.get() > 0,
        "切っていない、メディアセグメントが 2 つ以上のファイルのケースが 1 つもなかった\n{runner}"
    );
    assert!(
        small_last_mdat_cases.get() > 0,
        "切っていない、最後の mdat が 32 バイト未満のケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_trailing_box_cases.get() > 0,
        "切っていない、最後の mdat の後ろにボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_mdat_size_zero_cases.get() > 0,
        "切っていない、最後の mdat の size を 0 にしたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        with_between_box_cases.get() > 0,
        "切っていない、moof と mdat の間にボックスを置いたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        truncated_cases.get() > 0,
        "ファイルの途中で切った入力を使ったケースが 1 つもなかった\n{runner}"
    );
    assert!(
        inside_segment_truncated_cases.get() > 0,
        "moof または mdat の途中で切ったケースが 1 つもなかった\n{runner}"
    );
    assert!(
        whole_file_cases.get() > 0,
        "ファイルを切らなかったケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// 未対応のハンドラー種別のトラックを含む fMP4 でも、対応しているトラックのサンプルが取り出せることを確認する
///
/// 1 トラックの `hdlr` のハンドラー種別を `moov` の書き換えで `meta` に変える。
/// 返るサンプル列が、書き換えない場合の結果からそのトラックのサンプルを除いたものと一致することを確かめる。
/// `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の両方と、`default_base_is_moof` が true / false の両方を含める
#[test]
fn unsupported_track_traf_is_skipped() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // 先頭の `traf` のトラックを読み飛ばしたケース数。
    // `default_base_is_moof = false` の場合、2 番目以降の `traf` の基準位置が読み飛ばす `traf` のデータ末尾になる
    let first_traf_skipped_cases = std::cell::Cell::new(0usize);
    // `default_base_is_moof = false` に書き換えたケース数。各ケースで 1/2 の確率で選ぶ
    let base_is_moof_false_cases = std::cell::Cell::new(0usize);
    // 先頭の `traf` を読み飛ばし、かつ `default_base_is_moof = false` のケース数。
    // この組み合わせでだけ、読み飛ばす `traf` のデータ末尾が次の `traf` の基準位置になる
    let first_traf_skipped_with_base_is_moof_false_cases = std::cell::Cell::new(0usize);
    // 映像のトラックを読み飛ばしたケース数と音声のトラックを読み飛ばしたケース数。各ケースで 1/2 の確率で選ぶ
    let skip_video_cases = std::cell::Cell::new(0usize);
    let skip_audio_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let video_samples = sample_vec(ctx, 1..4, |ctx| arb_video_sample(ctx, 0));
        let audio_samples = sample_vec(ctx, 1..4, |ctx| arb_audio_sample(ctx, 1));
        let skip_video = noprop::sample_bool(ctx);
        let default_base_is_moof = noprop::sample_bool(ctx);

        let video_sample_entry = create_avc1_sample_entry(320, 240);
        let audio_sample_entry = create_opus_sample_entry();

        // 映像と音声を交互に並べる
        let mut fmp4_samples = Vec::new();
        let mut payloads: Vec<&[u8]> = Vec::new();
        for i in 0..video_samples.len().max(audio_samples.len()) {
            if let Some(sample) = video_samples.get(i) {
                fmp4_samples.push(video_segment_sample(&video_sample_entry, sample, None));
                payloads.push(&sample.data);
            }
            if let Some(sample) = audio_samples.get(i) {
                fmp4_samples.push(audio_segment_sample(&audio_sample_entry, sample));
                payloads.push(&sample.data);
            }
        }

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_bytes = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        // `default_base_is_moof = false` の場合は、比較元と比較先の両方に同じ書き換えを使う
        let segment_bytes = if default_base_is_moof {
            segment_bytes
        } else {
            let (rewritten, traf_count) =
                rewrite_media_segment_default_base_is_moof_false(&segment_bytes, 0);
            assert!(traf_count >= 2, "映像と音声の 2 つの traf がある");
            base_is_moof_false_cases.set(base_is_moof_false_cases.get() + 1);
            rewritten
        };

        // 読み飛ばすトラックを選び、その `hdlr` のハンドラー種別を `meta` に書き換える。
        // muxer は映像のトラックを先に登録するため、trak の順は映像、音声になる
        let skip_track_index = if skip_video { 0 } else { 1 };
        let skipped_track_id = std::cell::Cell::new(0u32);
        let rewritten_init = rewrite_init_segment(&init_bytes, |moov_box| {
            let trak = &mut moov_box.trak_boxes[skip_track_index];
            let expected_handler_type = if skip_video { *b"vide" } else { *b"soun" };
            assert_eq!(
                trak.mdia_box.hdlr_box.handler_type, expected_handler_type,
                "書き換えるトラックのハンドラー種別が想定と異なる"
            );
            skipped_track_id.set(trak.tkhd_box.track_id);
            trak.mdia_box.hdlr_box.handler_type = *b"meta";
        });
        let skipped_track_id = skipped_track_id.get();

        // 読み飛ばすトラックが先頭の `traf` かどうかを数える
        let (moof_box, _) = MoofBox::decode(&segment_bytes)
            .expect("書き換えたメディアセグメントからの moof デコードに失敗した");
        if moof_box.traf_boxes[0].tfhd_box.track_id == skipped_track_id {
            first_traf_skipped_cases.set(first_traf_skipped_cases.get() + 1);
            if !default_base_is_moof {
                first_traf_skipped_with_base_is_moof_false_cases
                    .set(first_traf_skipped_with_base_is_moof_false_cases.get() + 1);
            }
        }

        // 参考: ハンドラー種別を書き換えない init セグメントで、両方のトラックのサンプルを取り出す
        let mut reference_demuxer = Fmp4SegmentDemuxer::new();
        reference_demuxer
            .handle_init_segment(&init_bytes)
            .expect("init セグメントの処理に失敗した");
        let reference_samples = to_comparable_samples(
            &reference_demuxer
                .handle_media_segment(&segment_bytes)
                .expect("media セグメントの処理に失敗した"),
        );
        let expected: Vec<ComparableSample> = reference_samples
            .into_iter()
            .filter(|sample| sample.track.track_id != skipped_track_id)
            .collect();
        assert!(
            !expected.is_empty(),
            "読み飛ばすトラック以外のサンプルが 1 つもない"
        );

        // 実際: ハンドラー種別を `meta` に書き換えた init セグメント
        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&rewritten_init)
            .expect("init セグメントの処理に失敗した");
        let track_count = demuxer.tracks().expect("tracks の取得に失敗した").len();
        assert_eq!(
            track_count, 1,
            "未対応のハンドラー種別のトラックはトラック情報に登録されない"
        );
        let actual = to_comparable_samples(
            &demuxer
                .handle_media_segment(&segment_bytes)
                .expect("media セグメントの処理に失敗した"),
        );
        assert_eq!(
            actual, expected,
            "未対応のトラックのサンプルを取り除いた結果と一致しない"
        );

        // メディアセグメントを 2 つ連結したファイルでも同じことを確認する
        let mut reference_file = init_bytes;
        let mut actual_file = rewritten_init;
        for _ in 0..2 {
            reference_file.extend_from_slice(&segment_bytes);
            actual_file.extend_from_slice(&segment_bytes);
        }

        let mut reference_file_demuxer = Fmp4FileDemuxer::new();
        let reference_file_samples =
            collect_comparable_samples(&mut reference_file_demuxer, &reference_file);
        let expected_file: Vec<ComparableSample> = reference_file_samples
            .into_iter()
            .filter(|sample| sample.track.track_id != skipped_track_id)
            .collect();
        assert!(
            !expected_file.is_empty(),
            "Fmp4FileDemuxer で読み飛ばすトラック以外のサンプルが 1 つもない"
        );

        let mut file_demuxer = Fmp4FileDemuxer::new();
        let actual_file_samples = collect_comparable_samples(&mut file_demuxer, &actual_file);
        assert_eq!(
            actual_file_samples, expected_file,
            "Fmp4FileDemuxer で未対応のトラックのサンプルを取り除いた結果と一致しない"
        );

        if skip_video {
            skip_video_cases.set(skip_video_cases.get() + 1);
        } else {
            skip_audio_cases.set(skip_audio_cases.get() + 1);
        }
        Ok(())
    })?;

    assert!(
        first_traf_skipped_cases.get() > 0,
        "先頭の traf のトラックを読み飛ばしたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        base_is_moof_false_cases.get() > 0,
        "default_base_is_moof = false に書き換えたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        first_traf_skipped_with_base_is_moof_false_cases.get() > 0,
        "先頭の traf を読み飛ばし、かつ default_base_is_moof = false のケースが 1 つもなかった\n{runner}"
    );
    assert!(
        skip_video_cases.get() > 0,
        "映像のトラックを読み飛ばしたケースが 1 つもなかった\n{runner}"
    );
    assert!(
        skip_audio_cases.get() > 0,
        "音声のトラックを読み飛ばしたケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// 同じトラックの `traf` が分かれていて `tfdt` の順が入れ替わった入力でも、
/// `Fmp4FileDemuxer` が panic せず、取り出し順で `sample_entry` の約束を守ることを確認する
///
/// `Fmp4FileDemuxer` はサンプルをタイムスタンプの順に並べ替えるため、`traf` / `trun` の並び順と
/// 取り出し順が入れ替わることがある。内部の demuxer は `traf` / `trun` の並び順で「最初のサンプル」を
/// 決めるので、並べ替えの結果、`sample_entry` が `None` のサンプルが同じトラックの先頭に来る。
/// そのときに panic せず、ファイル全体の取り出し順で最初のサンプルに `sample_entry` が付くことを確かめる
#[test]
fn fmp4_file_demuxer_swapped_traf_order_keeps_sample_entry_contract() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    // 並べ替えで `tfdt` を 0 にした方の `traf` のサンプルが先に来たケース数
    let swapped_order_cases = std::cell::Cell::new(0usize);

    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));
        let sample_entry = create_avc1_sample_entry(320, 240);

        let mut fmp4_samples = Vec::new();
        let mut payloads: Vec<&[u8]> = Vec::new();
        for sample in &samples {
            fmp4_samples.push(video_segment_sample(&sample_entry, sample, None));
            payloads.push(&sample.data);
        }

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let media_segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_segment = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        // `traf` を複製して 2 つにし、1 つ目の `tfdt` を 90000、複製した 2 つ目を 0 にする。
        // サンプルの尺は 90000 より十分小さいので、並べ替えると 2 つ目の `traf` のサンプルが先に来る
        let media_segment = rewrite_media_segment_moof(&media_segment, |moof_box| {
            let mut duplicated_traf_box = moof_box.traf_boxes[0].clone();
            moof_box.traf_boxes[0].tfdt_box = Some(TfdtBox {
                version: 1,
                base_media_decode_time: 90_000,
            });
            duplicated_traf_box.tfdt_box = Some(TfdtBox {
                version: 0,
                base_media_decode_time: 0,
            });
            moof_box.traf_boxes.push(duplicated_traf_box);
        });

        let mut file_data = init_segment;
        file_data.extend_from_slice(&media_segment);

        let mut demuxer = Fmp4FileDemuxer::new();
        let demuxed = collect_comparable_samples(&mut demuxer, &file_data);

        // 2 つの `traf` が同じサンプルを 1 回ずつ返す
        assert_eq!(
            demuxed.len(),
            2 * samples.len(),
            "2 つの traf のサンプルがすべて取り出せる"
        );
        assert!(
            demuxed
                .windows(2)
                .all(|pair| pair[0].timestamp <= pair[1].timestamp),
            "取り出し順のタイムスタンプが昇順になっている"
        );
        // `tfdt` を 0 にした方の `traf` のサンプル（timestamp が 0）が先頭に来ていれば、
        // 並べ替えで `traf` の並び順と取り出し順が入れ替わっている
        if demuxed.first().expect("サンプルがある").timestamp == 0
            && demuxed.last().expect("サンプルがある").timestamp >= 90_000
        {
            swapped_order_cases.set(swapped_order_cases.get() + 1);
        }
        assert!(
            demuxed
                .first()
                .expect("サンプルがある")
                .sample_entry
                .is_some(),
            "取り出し順で最初のサンプルには sample_entry が付く"
        );
        let with_sample_entry_count = demuxed
            .iter()
            .filter(|sample| sample.sample_entry.is_some())
            .count();
        assert_eq!(
            with_sample_entry_count, 1,
            "サンプルエントリーが変わらないので、sample_entry は最初のサンプルにだけ付く"
        );
        Ok(())
    })?;

    assert!(
        swapped_order_cases.get() > 0,
        "並べ替えで traf の並び順と取り出し順が入れ替わったケースが 1 つもなかった\n{runner}"
    );
    Ok(())
}

/// `data_offset` を除いた比較用のフィールドをまとめる
///
/// `ComparableSample` にフィールドが増えたときに比較漏れがコンパイルエラーになるよう、
/// `..` を使わずに分解する
fn comparable_without_data_offset(
    sample: &ComparableSample,
) -> (
    TrackInfo,
    Option<SampleEntry>,
    bool,
    u64,
    u32,
    usize,
    Option<i64>,
) {
    let ComparableSample {
        track,
        sample_entry,
        keyframe,
        timestamp,
        duration,
        data_offset: _,
        data_size,
        composition_time_offset,
    } = sample;
    (
        track.clone(),
        sample_entry.clone(),
        *keyframe,
        *timestamp,
        *duration,
        *data_size,
        *composition_time_offset,
    )
}

/// 2 つのサンプル列が、`data_offset` 以外のフィールドと、`data_offset` と `data_size` が指す
/// バイト列で一致することを確認する
fn assert_samples_match_without_data_offset(
    reference_samples: &[ComparableSample],
    reference_data: &[u8],
    actual_samples: &[ComparableSample],
    actual_data: &[u8],
) {
    assert_eq!(
        actual_samples.len(),
        reference_samples.len(),
        "サンプル数が一致する"
    );
    for (reference_sample, actual_sample) in reference_samples.iter().zip(actual_samples) {
        // `data_offset` は `moof` のサイズが変わる分だけずれるので、それ以外を比較する
        assert_eq!(
            comparable_without_data_offset(actual_sample),
            comparable_without_data_offset(reference_sample),
            "data_offset 以外が一致する"
        );
        assert_eq!(
            &actual_data[actual_sample.data_offset as usize..][..actual_sample.data_size],
            &reference_data[reference_sample.data_offset as usize..][..reference_sample.data_size],
            "data_offset と data_size が指すバイト列が一致する"
        );
    }
}

/// `data_offset` のない 2 つ目以降の `trun` のサンプルが、直前の `trun` のデータの直後から
/// 取り出されることを確認する
///
/// muxer の出力の `trun` をサンプルごとに分け、2 つ目以降の `data_offset` を省く。
/// 分けない場合と比べて、`data_offset` 以外のフィールドと、`data_offset` と `data_size` が指す
/// バイト列が一致することを `Fmp4SegmentDemuxer` と `Fmp4FileDemuxer` の両方で確かめる
#[test]
fn trun_without_data_offset_starts_after_previous_run() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let samples = sample_vec(ctx, 2..5, |ctx| arb_video_sample(ctx, 0));
        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut fmp4_samples = Vec::new();
        let mut payloads: Vec<&[u8]> = Vec::new();
        for sample in &samples {
            fmp4_samples.push(video_segment_sample(&sample_entry, sample, None));
            payloads.push(&sample.data);
        }

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let media_segment = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
        let init_segment = muxer
            .init_segment_bytes()
            .expect("init セグメントの構築に失敗した");

        // `trun` をサンプルごとに分け、2 つ目以降の `data_offset` を省く
        let split_segment = rewrite_media_segment_moof(&media_segment, |moof_box| {
            let trun_box = moof_box.traf_boxes[0].trun_boxes[0].clone();
            let data_offset = trun_box.data_offset.expect("muxer は data_offset を書く");
            let mut split_trun_boxes = Vec::new();
            for (i, sample) in trun_box.samples.iter().enumerate() {
                let mut split_trun_box = trun_box.clone();
                split_trun_box.samples = vec![sample.clone()];
                split_trun_box.data_offset = (i == 0).then_some(data_offset);
                split_trun_boxes.push(split_trun_box);
            }
            moof_box.traf_boxes[0].trun_boxes = split_trun_boxes;
        });

        // 分けた後の `trun` が 2 つ以上あり、2 つ目以降の `data_offset` が省かれていることを確かめる
        let (split_moof_box, _) = MoofBox::decode(&split_segment)
            .expect("書き換えたメディアセグメントからの moof デコードに失敗した");
        let split_trun_boxes = &split_moof_box.traf_boxes[0].trun_boxes;
        assert_eq!(
            split_trun_boxes.len(),
            samples.len(),
            "サンプルごとに trun を分ける"
        );
        assert!(
            split_trun_boxes[1..]
                .iter()
                .all(|trun_box| trun_box.data_offset.is_none()),
            "2 つ目以降の trun の data_offset を省く"
        );

        // 比較元: 分けない場合
        let mut reference_demuxer = Fmp4SegmentDemuxer::new();
        reference_demuxer
            .handle_init_segment(&init_segment)
            .expect("init セグメントの処理に失敗した");
        let reference_samples = to_comparable_samples(
            &reference_demuxer
                .handle_media_segment(&media_segment)
                .expect("分割前の media セグメントの処理に失敗した"),
        );

        // 比較先: `data_offset` を省いた `trun` に分けた場合
        let mut demuxer = Fmp4SegmentDemuxer::new();
        demuxer
            .handle_init_segment(&init_segment)
            .expect("init セグメントの処理に失敗した");
        let actual_samples = to_comparable_samples(
            &demuxer
                .handle_media_segment(&split_segment)
                .expect("分割後の media セグメントの処理に失敗した"),
        );

        assert_samples_match_without_data_offset(
            &reference_samples,
            &media_segment,
            &actual_samples,
            &split_segment,
        );

        // `Fmp4FileDemuxer` でも同じことを確かめる
        let mut reference_file = init_segment.clone();
        reference_file.extend_from_slice(&media_segment);
        let mut split_file = init_segment;
        split_file.extend_from_slice(&split_segment);

        let mut reference_file_demuxer = Fmp4FileDemuxer::new();
        let reference_file_samples =
            collect_comparable_samples(&mut reference_file_demuxer, &reference_file);
        let mut file_demuxer = Fmp4FileDemuxer::new();
        let actual_file_samples = collect_comparable_samples(&mut file_demuxer, &split_file);

        assert_samples_match_without_data_offset(
            &reference_file_samples,
            &reference_file,
            &actual_file_samples,
            &split_file,
        );

        Ok(())
    })?;

    Ok(())
}

/// timestamp が複数セグメントにわたって正しく累積されることを確認する
#[test]
fn timestamp_accumulation() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let samples_per_segment = sample_vec(ctx, 2..5, |ctx| {
            sample_vec(ctx, 1..5, |ctx| arb_video_sample(ctx, 0))
        });

        let sample_entry = create_avc1_sample_entry(320, 240);
        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");

        let mut expected_decode_time: u64 = 0;
        let mut demuxer = Fmp4SegmentDemuxer::new();
        let mut initialized = false;

        for segment_samples in &samples_per_segment {
            let fmp4_samples: Vec<Sample> = segment_samples
                .iter()
                .map(|sample| video_segment_sample(&sample_entry, sample, None))
                .collect();
            let payloads: Vec<&[u8]> = segment_samples
                .iter()
                .map(|sample| sample.data.as_slice())
                .collect();
            let segment_bytes = build_complete_media_segment(&mut muxer, &fmp4_samples, &payloads);
            if !initialized {
                let init_bytes = muxer
                    .init_segment_bytes()
                    .expect("init セグメントの構築に失敗した");
                demuxer
                    .handle_init_segment(&init_bytes)
                    .expect("init セグメントの処理に失敗した");
                initialized = true;
            }

            let demuxed = demuxer
                .handle_media_segment(&segment_bytes)
                .expect("media セグメントの処理に失敗した");

            assert_eq!(demuxed[0].timestamp, expected_decode_time);

            expected_decode_time += segment_samples
                .iter()
                .map(|s| s.duration as u64)
                .sum::<u64>();
        }
        Ok(())
    })?;
    Ok(())
}

/// トラック payload 間に隙間がある配置は拒否する
#[test]
fn rejects_gapped_track_payload_layout() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let video_size = noprop::sample_usize_in(ctx, 1..256);
        let audio_size = noprop::sample_usize_in(ctx, 1..256);

        let mut muxer = Fmp4SegmentMuxer::new().expect("Fmp4SegmentMuxer::new に失敗した");
        let video_sample = Sample {
            track_kind: TrackKind::Video,
            sample_entry: Some(create_avc1_sample_entry(320, 240)),
            keyframe: true,
            timescale: NonZeroU32::new(VIDEO_TIMESCALE).expect("非ゼロである"),
            duration: 3000,
            composition_time_offset: None,
            data_offset: 0,
            data_size: video_size,
        };
        let audio_sample = Sample {
            track_kind: TrackKind::Audio,
            sample_entry: Some(create_opus_sample_entry()),
            keyframe: true,
            timescale: NonZeroU32::new(AUDIO_TIMESCALE).expect("非ゼロである"),
            duration: 960,
            composition_time_offset: None,
            data_offset: video_size as u64 + 1,
            data_size: audio_size,
        };

        let result = muxer.create_media_segment_metadata(&[video_sample, audio_sample]);
        assert!(matches!(
            result,
            Err(shiguredo_mp4::mux::MuxError::EncodeError(_))
        ));
        Ok(())
    })?;
    Ok(())
}
