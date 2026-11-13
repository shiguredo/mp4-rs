//! fMP4 セグメントのマルチプレックスを行う JavaScript 向けクラス
//!
//! # サンプルメタデータ JSON の形式
//!
//! ```json
//! [
//!   {
//!     "track_kind": "video",
//!     "timescale": 90000,
//!     "sample_entry": { ... },
//!     "duration": 3000,
//!     "keyframe": true,
//!     "composition_time_offset": null,
//!     "data_size": 1024
//!   }
//! ]
//! ```
//!
//! `data_size` の合計は `sample_data` の長さと一致している必要がある。
//! サンプルデータはサンプルの出現順に連結されていてよい。
//! 実際の `mdat` ペイロードの配置は、このモジュールが muxer の要求に合わせて並べ替える。
use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind,
    boxes::SampleEntry,
    mux::{Fmp4SegmentMuxer as Inner, Sample, SegmentMuxerOptions},
};
use wasm_bindgen::prelude::*;

use crate::boxes::parse_json as parse_json_sample_entry;
use crate::track::optional_i64_member;

/// fMP4 の初期化セグメントとメディアセグメントを生成する
#[wasm_bindgen]
pub struct Fmp4SegmentMuxer {
    inner: Inner,
}

#[wasm_bindgen]
impl Fmp4SegmentMuxer {
    /// マルチプレクサを作成する
    ///
    /// `creationTimestampSecs` は生成するセグメントの作成時刻（UNIX エポックからの秒数）である。
    #[wasm_bindgen(constructor)]
    pub fn new(creation_timestamp_secs: f64) -> Result<Fmp4SegmentMuxer, JsError> {
        let options = SegmentMuxerOptions {
            creation_timestamp: std::time::Duration::from_secs(creation_timestamp_secs as u64),
            ..Default::default()
        };
        let inner = Inner::with_options(options)
            .map_err(|error| JsError::new(&format!("failed to create muxer: {error}")))?;
        Ok(Self { inner })
    }

    /// メディアセグメントの先頭メタデータ（`moof` と `mdat` ヘッダー）を返す
    ///
    /// 返り値に `mdat` のペイロードは含まれないが、`sampleData` に渡した
    /// ペイロードを muxer の要求順に並べ替えたものを後ろに連結して返す。
    /// したがって返り値をそのまま書き込めば完全なメディアセグメントになる。
    #[wasm_bindgen(js_name = createMediaSegmentMetadata)]
    pub fn create_media_segment_metadata(
        &mut self,
        samples_json: &str,
        sample_data: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        let samples_with_ranges = parse_json_samples(samples_json)
            .map_err(|error| JsError::new(&format!("failed to parse samples JSON: {error}")))?;
        let (samples, payload) = arrange_payload(samples_with_ranges, sample_data)
            .map_err(|message| JsError::new(&message))?;
        let metadata = self
            .inner
            .create_media_segment_metadata(&samples)
            .map_err(|error| JsError::new(&format!("failed to create media segment: {error}")))?;
        Ok(concat(metadata, payload))
    }

    /// `sidx` 付きのメディアセグメント先頭メタデータを返す
    ///
    /// ペイロードの扱いは [`Self::create_media_segment_metadata`] と同じである。
    #[wasm_bindgen(js_name = createMediaSegmentMetadataWithSidx)]
    pub fn create_media_segment_metadata_with_sidx(
        &mut self,
        samples_json: &str,
        sample_data: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        let samples_with_ranges = parse_json_samples(samples_json)
            .map_err(|error| JsError::new(&format!("failed to parse samples JSON: {error}")))?;
        let (samples, payload) = arrange_payload(samples_with_ranges, sample_data)
            .map_err(|message| JsError::new(&message))?;
        let metadata = self
            .inner
            .create_media_segment_metadata_with_sidx(&samples)
            .map_err(|error| {
                JsError::new(&format!(
                    "failed to create media segment with sidx: {error}"
                ))
            })?;
        Ok(concat(metadata, payload))
    }

    /// 初期化セグメント（`ftyp` と `moov`）を返す
    ///
    /// メディアセグメントを 1 つ以上生成した後に呼び出す必要がある。
    #[wasm_bindgen(js_name = initSegment)]
    pub fn init_segment(&mut self) -> Result<Vec<u8>, JsError> {
        self.inner
            .init_segment_bytes()
            .map_err(|error| JsError::new(&format!("failed to create init segment: {error}")))
    }

    /// ランダムアクセス用の `mfra` ボックスを返す
    #[wasm_bindgen(js_name = mfra)]
    pub fn mfra(&mut self) -> Result<Vec<u8>, JsError> {
        self.inner
            .mfra_bytes()
            .map_err(|error| JsError::new(&format!("failed to create mfra box: {error}")))
    }
}

/// メタデータの後ろにペイロードを連結する
fn concat(mut metadata: Vec<u8>, payload: Vec<u8>) -> Vec<u8> {
    metadata.extend_from_slice(&payload);
    metadata
}

/// JSON から読み取ったサンプル 1 件分の情報
struct SampleWithSize {
    track_kind: TrackKind,
    timescale: NonZeroU32,
    sample_entry: Option<SampleEntry>,
    duration: u32,
    keyframe: bool,
    composition_time_offset: Option<i64>,
    data_size: usize,
}

/// サンプルメタデータの JSON を読み取る
fn parse_json_samples(text: &str) -> Result<Vec<SampleWithSize>, nojson::JsonParseError> {
    let raw_json = nojson::RawJson::parse(text)?;
    raw_json
        .value()
        .to_array()?
        .map(|item| {
            let track_kind_value = item.to_member("track_kind")?.required()?;
            let track_kind = match track_kind_value.to_unquoted_string_str()?.as_ref() {
                "audio" => TrackKind::Audio,
                "video" => TrackKind::Video,
                "subtitle" => TrackKind::Subtitle,
                _ => {
                    return Err(
                        track_kind_value.invalid("must be \"audio\", \"video\", or \"subtitle\"")
                    );
                }
            };
            let timescale_member = item.to_member("timescale")?.required()?;
            let timescale_value: u32 = timescale_member.try_into()?;
            let timescale = NonZeroU32::new(timescale_value)
                .ok_or_else(|| timescale_member.invalid("must be greater than 0"))?;
            let duration: u32 = item.to_member("duration")?.required()?.try_into()?;
            let keyframe: bool = item.to_member("keyframe")?.required()?.try_into()?;
            let composition_time_offset = optional_i64_member(item, "composition_time_offset")?;
            let data_size: u64 = item.to_member("data_size")?.required()?.try_into()?;
            let data_size = usize::try_from(data_size).map_err(|_| {
                item.to_member("data_size")
                    .and_then(|member| member.required())
                    .map(|value| value.invalid("data_size exceeds usize::MAX"))
                    .unwrap_or_else(|error| error)
            })?;
            // sample_entry の確保は他のフィールドの解決がすべて成功した後に行う。
            // 途中で失敗した場合に確保済みの領域を残さないためである
            let sample_entry =
                if let Some(sample_entry_value) = item.to_member("sample_entry")?.optional() {
                    if sample_entry_value.kind() == nojson::JsonValueKind::Null {
                        None
                    } else {
                        Some(parse_json_sample_entry(sample_entry_value)?)
                    }
                } else {
                    None
                };
            Ok(SampleWithSize {
                track_kind,
                timescale,
                sample_entry,
                duration,
                keyframe,
                composition_time_offset,
                data_size,
            })
        })
        .collect()
}

/// サンプルデータをトラック単位に並べ替え、各サンプルの `data_offset` を確定する
///
/// muxer は同一トラックのペイロードが連続した 1 区間に並んでいることを前提とするため、
/// トラックの初出順にペイロードを連結し直す。
fn arrange_payload(
    samples: Vec<SampleWithSize>,
    sample_data: &[u8],
) -> Result<(Vec<Sample>, Vec<u8>), String> {
    let mut ranges = Vec::new();
    let mut offset = 0usize;
    for sample in &samples {
        let end = offset
            .checked_add(sample.data_size)
            .ok_or_else(|| "sample data size overflow".to_string())?;
        if end > sample_data.len() {
            return Err(format!(
                "sample data is too short: required at least {end} bytes, but got {} bytes",
                sample_data.len()
            ));
        }
        ranges.push(&sample_data[offset..end]);
        offset = end;
    }

    let mut ordered_kinds = Vec::new();
    for sample in &samples {
        if !ordered_kinds.contains(&sample.track_kind) {
            ordered_kinds.push(sample.track_kind);
        }
    }

    let mut arranged_payload = Vec::new();
    let mut next_offset = 0u64;
    let mut mux_samples = Vec::new();
    for track_kind in ordered_kinds {
        for (sample, payload) in samples.iter().zip(ranges.iter()) {
            if sample.track_kind != track_kind {
                continue;
            }
            mux_samples.push(Sample {
                track_kind: sample.track_kind,
                sample_entry: sample.sample_entry.clone(),
                keyframe: sample.keyframe,
                timescale: sample.timescale,
                duration: sample.duration,
                composition_time_offset: sample.composition_time_offset,
                data_offset: next_offset,
                data_size: payload.len(),
            });
            next_offset = next_offset
                .checked_add(payload.len() as u64)
                .ok_or_else(|| "payload size overflow".to_string())?;
            arranged_payload.extend_from_slice(payload);
        }
    }

    Ok((mux_samples, arranged_payload))
}
