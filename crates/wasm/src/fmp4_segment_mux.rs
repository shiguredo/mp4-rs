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
use shiguredo_mp4::mux::{Fmp4SegmentMuxer as Inner, Sample, SegmentMuxerOptions};
use wasm_bindgen::prelude::*;

use crate::convert::{js_error, to_u64};
use crate::mux_sample;

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
    pub fn new(
        #[wasm_bindgen(js_name = creationTimestampSecs)] creation_timestamp_secs: f64,
    ) -> Result<Fmp4SegmentMuxer, JsError> {
        let creation_timestamp_secs =
            to_u64(creation_timestamp_secs, "creationTimestampSecs").map_err(js_error)?;
        // MP4 の時刻は 1904 年 1 月 1 日からの秒数であり、変換時にこの定数を加算する。
        // 加算が u64 を超えないよう、上限をここで抑える
        const MP4_EPOCH_OFFSET_SECS: u64 = 2_082_844_800;
        if creation_timestamp_secs > u64::MAX - MP4_EPOCH_OFFSET_SECS {
            return Err(JsError::new("creationTimestampSecs is too large"));
        }
        let options = SegmentMuxerOptions {
            creation_timestamp: std::time::Duration::from_secs(creation_timestamp_secs),
            ..Default::default()
        };
        Self::from_options(options)
    }

    /// JSON のオプションからマルチプレクサを作成する
    ///
    /// `new()` と異なり、ファイル作成時刻とトラックごとの編集リストを指定できる。
    /// スキーマは `crates/wasm/README.md` に記載している。
    #[wasm_bindgen(js_name = withOptions)]
    pub fn with_options(
        #[wasm_bindgen(js_name = optionsJson)] options_json: &str,
    ) -> Result<Fmp4SegmentMuxer, JsError> {
        let parsed = crate::mux_options::parse(options_json)?;
        let options = SegmentMuxerOptions {
            creation_timestamp: parsed.creation_timestamp,
            video_track: parsed.video_track,
            audio_track: parsed.audio_track,
            subtitle_track: parsed.subtitle_track,
            pssh_boxes: Vec::new(),
        };
        Self::from_options(options)
    }

    /// コアのオプションからインスタンスを構築する
    fn from_options(options: SegmentMuxerOptions) -> Result<Fmp4SegmentMuxer, JsError> {
        let inner = Inner::with_options(options)
            .map_err(|error| JsError::new(&format!("failed to create muxer: {error}")))?;
        Ok(Self { inner })
    }

    /// メディアセグメント全体 (`styp` を含む場合はそれを先頭に、`moof` と `mdat`) を返す
    ///
    /// `samplesJson` の出現順に並んだペイロードを muxer の要求順に並べ替え、
    /// メタデータの後ろに連結したものを返す。したがって返り値をそのまま書き込めば
    /// 完全なメディアセグメントになる。
    #[wasm_bindgen(js_name = createMediaSegment)]
    pub fn create_media_segment(
        &mut self,
        #[wasm_bindgen(js_name = samplesJson)] samples_json: &str,
        #[wasm_bindgen(js_name = sampleData)] sample_data: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        crate::json_limit::check(samples_json)
            .map_err(|error| JsError::new(&format!("failed to parse samples JSON: {error}")))?;
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

    /// `sidx` 付きのメディアセグメント全体を返す
    ///
    /// ペイロードの扱いは [`Self::create_media_segment`] と同じである。
    #[wasm_bindgen(js_name = createMediaSegmentWithSidx)]
    pub fn create_media_segment_with_sidx(
        &mut self,
        #[wasm_bindgen(js_name = samplesJson)] samples_json: &str,
        #[wasm_bindgen(js_name = sampleData)] sample_data: &[u8],
    ) -> Result<Vec<u8>, JsError> {
        crate::json_limit::check(samples_json)
            .map_err(|error| JsError::new(&format!("failed to parse samples JSON: {error}")))?;
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
    pub fn init_segment(&self) -> Result<Vec<u8>, JsError> {
        self.inner
            .init_segment_bytes()
            .map_err(|error| JsError::new(&format!("failed to create init segment: {error}")))
    }

    /// ランダムアクセス用の `mfra` ボックスを返す
    #[wasm_bindgen(js_name = mfra)]
    pub fn mfra(&self) -> Result<Vec<u8>, JsError> {
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
    metadata: mux_sample::MuxSampleMetadata,
    data_size: usize,
}

/// サンプルメタデータの JSON を読み取る
fn parse_json_samples(text: &str) -> Result<Vec<SampleWithSize>, nojson::JsonParseError> {
    let raw_json = nojson::RawJson::parse(text)?;
    raw_json
        .value()
        .to_array()?
        .map(|item| {
            Ok(SampleWithSize {
                metadata: mux_sample::parse_metadata(item)?,
                data_size: mux_sample::required_usize_member(item, "data_size")?,
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
    // 余剰バイトを取りこぼすと、呼び出し側の data_size の誤りに気付けない
    if offset != sample_data.len() {
        return Err(format!(
            "sample data size mismatch: required {offset} bytes, but got {} bytes",
            sample_data.len()
        ));
    }

    let mut ordered_kinds = Vec::new();
    for sample in &samples {
        if !ordered_kinds.contains(&sample.metadata.track_kind) {
            ordered_kinds.push(sample.metadata.track_kind);
        }
    }

    let mut arranged_payload = Vec::new();
    let mut next_offset = 0u64;
    let mut mux_samples = Vec::new();
    for track_kind in ordered_kinds {
        for (sample, payload) in samples.iter().zip(ranges.iter()) {
            if sample.metadata.track_kind != track_kind {
                continue;
            }
            mux_samples.push(Sample {
                encryption_info: None,
                track_kind: sample.metadata.track_kind,
                sample_entry: sample.metadata.sample_entry.clone(),
                keyframe: sample.metadata.keyframe,
                timescale: sample.metadata.timescale,
                duration: sample.metadata.duration,
                composition_time_offset: sample.metadata.composition_time_offset,
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

#[cfg(test)]
mod tests {
    use super::*;
    use shiguredo_mp4::TrackKind;

    /// テスト用のサンプルメタデータを構築する
    fn build_sample(track_kind: TrackKind, data_size: usize) -> SampleWithSize {
        SampleWithSize {
            metadata: mux_sample::MuxSampleMetadata {
                track_kind,
                timescale: std::num::NonZeroU32::new(90000).expect("0 ではない"),
                sample_entry: None,
                duration: 3000,
                keyframe: true,
                composition_time_offset: None,
            },
            data_size,
        }
    }

    #[test]
    fn test_arrange_payload_reorders_by_track() {
        // 映像と音声を交互に並べる
        let samples = vec![
            build_sample(TrackKind::Video, 2),
            build_sample(TrackKind::Audio, 3),
            build_sample(TrackKind::Video, 4),
        ];
        let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9];

        let (mux_samples, payload) = arrange_payload(samples, &data).expect("並べ替えられること");

        // 映像 2 件が先、音声 1 件が後になる
        assert_eq!(mux_samples.len(), 3);
        assert!(matches!(mux_samples[0].track_kind, TrackKind::Video));
        assert_eq!(mux_samples[0].data_offset, 0);
        assert_eq!(mux_samples[0].data_size, 2);
        assert!(matches!(mux_samples[1].track_kind, TrackKind::Video));
        assert_eq!(mux_samples[1].data_offset, 2);
        assert_eq!(mux_samples[1].data_size, 4);
        assert!(matches!(mux_samples[2].track_kind, TrackKind::Audio));
        assert_eq!(mux_samples[2].data_offset, 6);
        assert_eq!(mux_samples[2].data_size, 3);

        // ペイロードも同じ順に並べ替えられる
        assert_eq!(payload, vec![1, 2, 6, 7, 8, 9, 3, 4, 5]);
    }

    #[test]
    fn test_arrange_payload_rejects_short_data() {
        let samples = vec![build_sample(TrackKind::Video, 4)];
        let error = arrange_payload(samples, &[1, 2, 3]).expect_err("データ不足は拒否されること");
        assert!(error.contains("too short"), "{error}");
    }

    #[test]
    fn test_arrange_payload_rejects_excess_data() {
        let samples = vec![build_sample(TrackKind::Video, 2)];
        let error = arrange_payload(samples, &[1, 2, 3]).expect_err("余剰データは拒否されること");
        assert!(error.contains("mismatch"), "{error}");
    }
}
