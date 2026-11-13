//! MP4 ファイルのマルチプレックスを行う JavaScript 向けクラス
use std::num::NonZeroU32;

use shiguredo_mp4::{
    TrackKind,
    mux::{Mp4FileMuxer as Inner, Mp4FileMuxerOptions, Sample},
};
use wasm_bindgen::prelude::*;

use crate::boxes::parse_json as parse_json_sample_entry;
use crate::track::optional_i64_member;

/// マルチプレクサが出力するボックスの 1 片
///
/// `offset` は書き込み先のファイル内バイト位置を表す。
#[wasm_bindgen]
pub struct Mp4FileMuxerOutput {
    offset: f64,
    data: Vec<u8>,
}

#[wasm_bindgen]
impl Mp4FileMuxerOutput {
    /// 書き込み先のファイル内バイト位置を返す
    #[wasm_bindgen(getter)]
    pub fn offset(&self) -> f64 {
        self.offset
    }

    /// 書き込むデータを返す
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }
}

/// MP4 ファイルを構築する
///
/// 使い方は次のとおりである。
///
/// 1. `initialBoxes()` が返すバイト列をファイルの先頭に書き込む
/// 2. サンプルデータを追記し、その位置とサイズを `appendSample()` に渡す
/// 3. `finalize()` を呼び、`nextOutput()` が返すボックスを書き込む
#[wasm_bindgen]
pub struct Mp4FileMuxer {
    inner: Inner,
    outputs: Vec<(u64, Vec<u8>)>,
    next_output_index: usize,
    faststart_enabled: bool,
    moov_box_size: usize,
}

#[wasm_bindgen]
impl Mp4FileMuxer {
    /// マルチプレクサを作成する
    ///
    /// `reservedMoovBoxSize` は faststart 形式のためにファイル先頭へ確保する
    /// moov ボックスのサイズである。0 を指定すると faststart は無効になる。
    #[wasm_bindgen(constructor)]
    pub fn new(reserved_moov_box_size: f64) -> Result<Mp4FileMuxer, JsError> {
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size: reserved_moov_box_size as usize,
            ..Default::default()
        };
        let inner = Inner::with_options(options)
            .map_err(|error| JsError::new(&format!("failed to create muxer: {error}")))?;
        Ok(Self {
            inner,
            outputs: Vec::new(),
            next_output_index: 0,
            faststart_enabled: false,
            moov_box_size: 0,
        })
    }

    /// ファイルの先頭に書き込むべきバイト列を返す
    #[wasm_bindgen(js_name = initialBoxes)]
    pub fn initial_boxes(&self) -> Vec<u8> {
        self.inner.initial_boxes_bytes().to_vec()
    }

    /// 確定した MP4 ファイルで faststart が有効かどうかを返す
    ///
    /// `finalize()` を呼ぶまでは常に `false` を返す。
    #[wasm_bindgen(js_name = isFaststartEnabled)]
    pub fn is_faststart_enabled(&self) -> bool {
        self.faststart_enabled
    }

    /// 確定した moov ボックスのサイズを返す
    ///
    /// `finalize()` を呼ぶまでは常に 0 を返す。
    #[wasm_bindgen(js_name = moovBoxSize)]
    pub fn moov_box_size(&self) -> f64 {
        self.moov_box_size as f64
    }

    /// サンプルデータを追記した位置とサイズを伝える
    ///
    /// サンプルデータをファイルへ書き込んだ後に呼び出す。
    #[wasm_bindgen(js_name = appendSample)]
    pub fn append_sample(&mut self, sample_json: &str) -> Result<(), JsError> {
        let sample = parse_json_mux_sample(sample_json)
            .map_err(|error| JsError::new(&format!("failed to parse sample JSON: {error}")))?;
        self.inner
            .append_sample(&sample)
            .map_err(|error| JsError::new(&format!("failed to append sample: {error}")))
    }

    /// ファイル内の書き込み位置を進める
    ///
    /// サンプルデータ以外のバイト列を書き込んだ場合に、そのサイズを伝える。
    #[wasm_bindgen(js_name = advancePosition)]
    pub fn advance_position(&mut self, size: f64) -> Result<(), JsError> {
        self.inner
            .advance_position(size as u64)
            .map_err(|error| JsError::new(&format!("failed to advance position: {error}")))
    }

    /// マルチプレックス処理を完了する
    ///
    /// 完了後に `nextOutput()` で書き込むべきボックスを取得する。
    #[wasm_bindgen(js_name = finalize)]
    pub fn finalize(&mut self) -> Result<(), JsError> {
        let finalized = self
            .inner
            .finalize()
            .map_err(|error| JsError::new(&format!("failed to finalize: {error}")))?;
        self.faststart_enabled = finalized.is_faststart_enabled();
        self.moov_box_size = finalized.moov_box_size();
        self.outputs = finalized
            .offset_and_bytes_pairs()
            .map(|(offset, data)| (offset, data.to_vec()))
            .collect();
        self.next_output_index = 0;
        Ok(())
    }

    /// 未取得の出力ボックスを 1 つ返す
    ///
    /// すべて取得し終えた場合は `undefined` を返す。
    #[wasm_bindgen(js_name = nextOutput)]
    pub fn next_output(&mut self) -> Option<Mp4FileMuxerOutput> {
        let output = self.outputs.get(self.next_output_index)?;
        self.next_output_index += 1;
        Some(Mp4FileMuxerOutput {
            offset: output.0 as f64,
            data: output.1.clone(),
        })
    }
}

/// マルチプレクサに渡すサンプルを JSON から構築する
fn parse_json_mux_sample(text: &str) -> Result<Sample, nojson::JsonParseError> {
    let raw_json = nojson::RawJson::parse(text)?;
    let value = raw_json.value();

    let track_kind_value = value.to_member("track_kind")?.required()?;
    let track_kind = match track_kind_value.to_unquoted_string_str()?.as_ref() {
        "audio" => TrackKind::Audio,
        "video" => TrackKind::Video,
        "subtitle" => TrackKind::Subtitle,
        _ => {
            return Err(track_kind_value.invalid("must be \"audio\", \"video\", or \"subtitle\""));
        }
    };

    let keyframe: bool = value.to_member("keyframe")?.required()?.try_into()?;
    let timescale_member = value.to_member("timescale")?.required()?;
    let timescale_value: u32 = timescale_member.try_into()?;
    let timescale = NonZeroU32::new(timescale_value)
        .ok_or_else(|| timescale_member.invalid("must be greater than 0"))?;
    let duration: u32 = value.to_member("duration")?.required()?.try_into()?;
    let composition_time_offset = optional_i64_member(value, "composition_time_offset")?;
    let data_offset: u64 = value.to_member("data_offset")?.required()?.try_into()?;
    let data_size: u32 = value.to_member("data_size")?.required()?.try_into()?;

    let sample_entry = if let Some(sample_entry_value) = value.to_member("sample_entry")?.optional()
    {
        if sample_entry_value.kind() == nojson::JsonValueKind::Null {
            None
        } else {
            Some(parse_json_sample_entry(sample_entry_value)?)
        }
    } else {
        None
    };

    Ok(Sample {
        track_kind,
        sample_entry,
        keyframe,
        timescale,
        duration,
        composition_time_offset,
        data_offset,
        data_size: data_size as usize,
    })
}
