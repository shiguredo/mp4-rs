//! MP4 ファイルのデマルチプレックスを行う JavaScript 向けクラス
use wasm_bindgen::prelude::*;

use shiguredo_mp4::demux::{Input, Mp4FileDemuxer as Inner};

use crate::track::{fmt_required_input, fmt_sample, fmt_track_info};

/// MP4 ファイルを段階的に読み込んでメディアサンプルを取り出す
///
/// Sans I/O のため、ファイルの読み込みは呼び出し側が行う。
/// `requiredInput()` が返す位置とサイズに従ってデータを読み、`handleInput()` に渡す。
#[wasm_bindgen]
pub struct Mp4FileDemuxer {
    inner: Inner,
}

#[wasm_bindgen]
impl Mp4FileDemuxer {
    /// デマルチプレクサを作成する
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Inner::new(),
        }
    }

    /// 次に必要な入力データを JSON で返す
    ///
    /// これ以上入力が不要になった場合は `undefined` を返す。
    /// 返り値の形式は `{"position": 0, "size": 32}` であり、
    /// `size` が `null` の場合はファイル末尾までのデータが必要であることを表す。
    #[wasm_bindgen(js_name = requiredInput)]
    pub fn required_input(&self) -> Option<String> {
        self.inner
            .required_input()
            .map(|required| nojson::json(|f| fmt_required_input(f, required)).to_string())
    }

    /// 入力データを供給する
    ///
    /// `position` はファイル先頭からのバイト位置である。
    /// JavaScript の数値は 2 の 53 乗まで正確に表現できるため、
    /// 実用上はファイルサイズの上限に達しない。
    #[wasm_bindgen(js_name = handleInput)]
    pub fn handle_input(&mut self, position: f64, data: &[u8]) {
        self.inner.handle_input(Input {
            position: position as u64,
            data,
        });
    }

    /// トラック情報を JSON の配列で返す
    #[wasm_bindgen(js_name = tracks)]
    pub fn tracks(&mut self) -> Result<String, JsError> {
        let tracks = self
            .inner
            .tracks()
            .map_err(|error| JsError::new(&format!("failed to get tracks: {error}")))?;
        Ok(nojson::json(|f| {
            f.array(|f| {
                for track in tracks {
                    f.element(nojson::json(|f| fmt_track_info(f, track)))?;
                }
                Ok(())
            })
        })
        .to_string())
    }

    /// 時系列順に次のサンプルを JSON で返す
    ///
    /// すべてのサンプルを取得し終えた場合は `undefined` を返す。
    #[wasm_bindgen(js_name = nextSample)]
    pub fn next_sample(&mut self) -> Result<Option<String>, JsError> {
        match self.inner.next_sample() {
            Ok(Some(sample)) => Ok(Some(nojson::json(|f| fmt_sample(f, &sample)).to_string())),
            Ok(None) => Ok(None),
            Err(error) => Err(JsError::new(&format!("failed to get next sample: {error}"))),
        }
    }

    /// 時系列順に前のサンプルを JSON で返す
    ///
    /// ファイルの先頭に達した場合は `undefined` を返す。
    #[wasm_bindgen(js_name = prevSample)]
    pub fn prev_sample(&mut self) -> Result<Option<String>, JsError> {
        match self.inner.prev_sample() {
            Ok(Some(sample)) => Ok(Some(nojson::json(|f| fmt_sample(f, &sample)).to_string())),
            Ok(None) => Ok(None),
            Err(error) => Err(JsError::new(&format!(
                "failed to get previous sample: {error}"
            ))),
        }
    }

    /// 指定した時刻へシークする
    ///
    /// `timestamp / timescale` 秒の位置を探し、その位置を含むサンプルから
    /// 次の `nextSample()` が開始されるようにする。
    #[wasm_bindgen(js_name = seek)]
    pub fn seek(&mut self, timestamp: f64, timescale: f64) -> Result<(), JsError> {
        if timescale <= 0.0 {
            return Err(JsError::new("timescale must be greater than 0"));
        }
        let position = std::time::Duration::from_secs(timestamp as u64)
            .checked_div(timescale as u32)
            .ok_or_else(|| JsError::new("timescale must be greater than 0"))?;
        self.inner
            .seek(position)
            .map_err(|error| JsError::new(&format!("failed to seek: {error}")))
    }
}

impl Default for Mp4FileDemuxer {
    fn default() -> Self {
        Self::new()
    }
}
