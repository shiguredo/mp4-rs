//! fMP4 ファイル全体のデマルチプレックスを行う JavaScript 向けクラス
use wasm_bindgen::prelude::*;

use shiguredo_mp4::demux::{Fmp4FileDemuxer as Inner, Input};

use crate::convert::{js_error, to_u64};
use crate::track::{fmt_required_input, fmt_sample, fmt_track_info_array};

/// fMP4 ファイルを段階的に読み込んでメディアサンプルを取り出す
///
/// `styp` / `moof` / `mdat` が並んだファイル全体を扱える。
/// Sans I/O のため、ファイルの読み込みは呼び出し側が行う。
/// `requiredInput()` が返す位置とサイズに従ってデータを読み、`handleInput()` に渡す。
#[wasm_bindgen]
pub struct Fmp4FileDemuxer {
    inner: Inner,
}

#[wasm_bindgen]
impl Fmp4FileDemuxer {
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
    /// 整数でない値や負の値を渡すと例外を投げる。
    /// 入力が不正な場合もその場で例外を投げる。
    #[wasm_bindgen(js_name = handleInput)]
    pub fn handle_input(&mut self, position: f64, data: &[u8]) -> Result<(), JsError> {
        let position = to_u64(position, "position").map_err(js_error)?;
        self.inner.handle_input(Input { position, data });
        // コアは入力を内部に保持するだけなので、その場でエラーを確認して返す
        if let Some(error) = self.inner.last_error() {
            return Err(JsError::new(&format!("failed to handle input: {error}")));
        }
        Ok(())
    }

    /// トラック情報を JSON の配列で返す
    #[wasm_bindgen(js_name = tracks)]
    pub fn tracks(&mut self) -> Result<String, JsError> {
        let tracks = self
            .inner
            .tracks()
            .map_err(|error| JsError::new(&format!("failed to get tracks: {error}")))?;
        Ok(fmt_track_info_array(tracks))
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
}

impl Default for Fmp4FileDemuxer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_required_input_before_initialization() {
        let demuxer = Fmp4FileDemuxer::new();
        let json = demuxer.required_input().expect("初期状態では入力が必要");
        assert!(json.contains("\"position\":0"), "{json}");
    }

    #[test]
    fn test_tracks_before_initialization_returns_error() {
        // `JsError` は wasm 以外では生成できないため、コアの挙動を直接確認する
        let mut inner = Inner::new();
        assert!(inner.tracks().is_err());
    }
}
