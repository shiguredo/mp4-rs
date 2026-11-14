//! MP4 ファイルのマルチプレックスを行う JavaScript 向けクラス
use shiguredo_mp4::mux::{Mp4FileMuxer as Inner, Mp4FileMuxerOptions, Sample};
use wasm_bindgen::prelude::*;

use crate::convert::{js_error, to_u64, to_usize};
use crate::mux_sample;

/// マルチプレクサが出力するボックスの 1 片
///
/// `offset` は書き込み先のファイル内バイト位置を表す。
/// データは `takeData()` で取り出す。`free()` を忘れると wasm のメモリが解放されない。
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

    /// 書き込むデータを取り出す
    ///
    /// Rust 側の余分なコピーを避けるため、getter ではなくメソッドにしている。
    /// wasm の境界を越えるときのコピーは 1 回発生する。
    /// 取り出した後に再度呼ぶと空の配列が返る。
    #[wasm_bindgen(js_name = takeData)]
    pub fn take_data(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
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
    /// moov ボックスのサイズである。0 を指定すると faststart は通常無効になる。
    /// 実際に faststart が有効になったかは `isFaststartEnabled()` で確認する。
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(js_name = reservedMoovBoxSize)] reserved_moov_box_size: f64,
    ) -> Result<Mp4FileMuxer, JsError> {
        let reserved_moov_box_size =
            to_usize(reserved_moov_box_size, "reservedMoovBoxSize").map_err(js_error)?;
        let options = Mp4FileMuxerOptions {
            reserved_moov_box_size,
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
    pub fn append_sample(
        &mut self,
        #[wasm_bindgen(js_name = sampleJson)] sample_json: &str,
    ) -> Result<(), JsError> {
        crate::json_limit::check(sample_json)
            .map_err(|error| JsError::new(&format!("failed to parse sample JSON: {error}")))?;
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
        let size = to_u64(size, "size").map_err(js_error)?;
        self.inner
            .advance_position(size)
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
        let output = self.outputs.get_mut(self.next_output_index)?;
        self.next_output_index += 1;
        Some(Mp4FileMuxerOutput {
            offset: output.0 as f64,
            // 同じ出力を二度返さないため、所有権をそのまま渡す
            data: std::mem::take(&mut output.1),
        })
    }
}

/// マルチプレクサに渡すサンプルを JSON から構築する
fn parse_json_mux_sample(text: &str) -> Result<Sample, nojson::JsonParseError> {
    let raw_json = nojson::RawJson::parse(text)?;
    let value = raw_json.value();

    let metadata = mux_sample::parse_metadata(value)?;
    let data_offset: u64 = value.to_member("data_offset")?.required()?.try_into()?;
    let data_size = mux_sample::required_usize_member(value, "data_size")?;

    Ok(Sample {
        track_kind: metadata.track_kind,
        sample_entry: metadata.sample_entry,
        keyframe: metadata.keyframe,
        timescale: metadata.timescale,
        duration: metadata.duration,
        composition_time_offset: metadata.composition_time_offset,
        data_offset,
        data_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// マルチプレックス用のサンプル JSON を組み立てる
    fn build_sample_json(extra: &str) -> String {
        format!(
            r#"{{
                "track_kind": "audio",
                "timescale": 48000,
                "duration": 960,
                "keyframe": true,
                "data_offset": 116,
                "data_size": 3
                {extra}
            }}"#
        )
    }

    #[test]
    fn test_parse_json_mux_sample() {
        let sample = parse_json_mux_sample(&build_sample_json(""))
            .expect("最小のサンプルをパースできること");
        assert_eq!(sample.data_offset, 116);
        assert_eq!(sample.data_size, 3);
        assert_eq!(sample.duration, 960);
    }

    #[test]
    fn test_parse_json_mux_sample_rejects_missing_data_offset() {
        let json = r#"{
            "track_kind": "audio",
            "timescale": 48000,
            "duration": 960,
            "keyframe": true,
            "data_size": 3
        }"#;
        assert!(parse_json_mux_sample(json).is_err());
    }

    #[test]
    fn test_parse_json_mux_sample_accepts_null_composition_time_offset() {
        let sample =
            parse_json_mux_sample(&build_sample_json(r#", "composition_time_offset": null"#))
                .expect("null を受理すること");
        assert_eq!(sample.composition_time_offset, None);
    }
}
