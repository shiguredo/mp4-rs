//! fMP4 セグメントのデマルチプレックスを行う JavaScript 向けクラス
use wasm_bindgen::prelude::*;

use shiguredo_mp4::demux::Fmp4SegmentDemuxer as Inner;

use crate::track::{fmt_sample, fmt_track_info_array};

/// fMP4 の初期化セグメントとメディアセグメントを読み取る
#[wasm_bindgen]
pub struct Fmp4SegmentDemuxer {
    inner: Inner,
}

#[wasm_bindgen]
impl Fmp4SegmentDemuxer {
    /// デマルチプレクサを作成する
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Inner::new(),
        }
    }

    /// 初期化セグメントを読み込む
    #[wasm_bindgen(js_name = handleInitSegment)]
    pub fn handle_init_segment(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.inner
            .handle_init_segment(data)
            .map_err(|error| JsError::new(&format!("failed to handle init segment: {error}")))
    }

    /// トラック情報を JSON の配列で返す
    #[wasm_bindgen(js_name = tracks)]
    pub fn tracks(&self) -> Result<String, JsError> {
        let tracks = self
            .inner
            .tracks()
            .map_err(|error| JsError::new(&format!("failed to get tracks: {error}")))?;
        Ok(fmt_track_info_array(tracks))
    }

    /// メディアセグメントを読み込み、サンプル情報を JSON の配列で返す
    ///
    /// `data_offset` は渡したメディアセグメントの先頭からの相対位置を表す。
    #[wasm_bindgen(js_name = handleMediaSegment)]
    pub fn handle_media_segment(&mut self, data: &[u8]) -> Result<String, JsError> {
        let samples = self
            .inner
            .handle_media_segment(data)
            .map_err(|error| JsError::new(&format!("failed to handle media segment: {error}")))?;
        Ok(nojson::json(|f| {
            f.array(|f| {
                for sample in &samples {
                    f.element(nojson::json(|f| fmt_sample(f, sample)))?;
                }
                Ok(())
            })
        })
        .to_string())
    }
}

impl Default for Fmp4SegmentDemuxer {
    fn default() -> Self {
        Self::new()
    }
}
