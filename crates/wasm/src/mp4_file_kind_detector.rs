//! 入力ファイルが通常の MP4 か fMP4 かを判定する JavaScript 向けクラス
use wasm_bindgen::prelude::*;

use shiguredo_mp4::demux::{Input, Mp4FileKind, Mp4FileKindDetector as Inner};

use crate::convert::{js_error, to_u64};
use crate::track::fmt_required_input;

/// ファイル先頭部分から MP4 の種類を判定する
#[wasm_bindgen]
pub struct Mp4FileKindDetector {
    inner: Inner,
}

#[wasm_bindgen]
impl Mp4FileKindDetector {
    /// 判定器を作成する
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: Inner::new(),
        }
    }

    /// 次に必要な入力データを JSON で返す
    ///
    /// これ以上入力が不要になった場合は `undefined` を返す。
    #[wasm_bindgen(js_name = requiredInput)]
    pub fn required_input(&self) -> Option<String> {
        self.inner
            .required_input()
            .map(|required| nojson::json(|f| fmt_required_input(f, required)).to_string())
    }

    /// 入力データを供給する
    ///
    /// `position` は `data` の先頭がファイル内の何バイト目かを表す。
    /// 整数でない値や負の値を渡すと例外を投げる。
    #[wasm_bindgen(js_name = handleInput)]
    pub fn handle_input(&mut self, position: f64, data: &[u8]) -> Result<(), JsError> {
        let position = to_u64(position, "position").map_err(js_error)?;
        self.inner.handle_input(Input { position, data });
        Ok(())
    }

    /// 判定結果を返す
    ///
    /// `"mp4"` か `"fragmented-mp4"` を返す。
    /// まだ判定に必要なデータが揃っていない場合は `undefined` を返す。
    #[wasm_bindgen(js_name = fileKind)]
    pub fn file_kind(&self) -> Result<Option<String>, JsError> {
        let kind = self
            .inner
            .file_kind()
            .map_err(|error| JsError::new(&format!("failed to get file kind: {error}")))?;
        Ok(kind.map(|kind| match kind {
            Mp4FileKind::Mp4 => "mp4".to_string(),
            Mp4FileKind::FragmentedMp4 => "fragmented-mp4".to_string(),
        }))
    }
}

impl Default for Mp4FileKindDetector {
    fn default() -> Self {
        Self::new()
    }
}
