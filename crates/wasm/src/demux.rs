//! MP4 ファイルのデマルチプレックスを行う JavaScript 向けクラス
use wasm_bindgen::prelude::*;

use shiguredo_mp4::demux::{Input, Mp4FileDemuxer as Inner};

use crate::convert::{js_error, to_positive_u32, to_u64};
use crate::track::{fmt_required_input, fmt_sample, fmt_track_info_array};

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
    /// 整数でない値や負の値を渡すと例外を投げる。
    #[wasm_bindgen(js_name = handleInput)]
    pub fn handle_input(&mut self, position: f64, data: &[u8]) -> Result<(), JsError> {
        let position = to_u64(position, "position").map_err(js_error)?;
        self.inner.handle_input(Input { position, data });
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
    /// `timestamp` は小数でもよい。ナノ秒未満は切り上げるため、
    /// 境界のごくわずか手前（1 ns 未満）を指定すると次のサンプルに進む。
    #[wasm_bindgen(js_name = seek)]
    pub fn seek(&mut self, timestamp: f64, timescale: f64) -> Result<(), JsError> {
        let position = duration_from_timestamp(timestamp, timescale).map_err(js_error)?;
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

/// `timestamp / timescale` 秒を [`std::time::Duration`] に変換する
///
/// 1 秒未満のシークも指定どおりに扱えるよう、ナノ秒に変換してから切り上げる。
fn duration_from_timestamp(timestamp: f64, timescale: f64) -> Result<std::time::Duration, String> {
    /// u64 で表現できるナノ秒の上限（2 の 64 乗）
    const NANOS_LIMIT: f64 = 18_446_744_073_709_551_616.0;

    let timescale = to_positive_u32(timescale, "timescale")?;
    if !timestamp.is_finite() {
        return Err("timestamp must be finite".to_string());
    }
    if timestamp < 0.0 {
        return Err("timestamp must be greater than or equal to 0".to_string());
    }
    // 切り捨てると、サンプル境界ちょうどの指定が 1 ns だけ手前に落ち、
    // コアの切り捨てと組み合わさって 1 サンプル前から再生されることがある
    let nanos = (timestamp / f64::from(timescale) * 1_000_000_000.0).ceil();
    if !nanos.is_finite() || nanos >= NANOS_LIMIT {
        return Err("timestamp is too large".to_string());
    }
    Ok(std::time::Duration::from_nanos(nanos as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_from_timestamp_does_not_undershoot() {
        // サンプル境界ちょうどの指定が、切り捨てで 1 サンプル前に落ちないこと。
        // コアはナノ秒からタイムスタンプへ戻すときに切り捨てるため、
        // ここで切り捨てると 2999 や 2047 になってしまう。
        for (timestamp, timescale) in [(3000.0, 90000.0), (2048.0, 44100.0)] {
            let duration =
                duration_from_timestamp(timestamp, timescale).expect("境界の指定を受理すること");
            let nanos = duration.as_nanos();
            let recovered = nanos * u128::from(timescale as u32) / 1_000_000_000;
            assert!(
                recovered >= timestamp as u128,
                "{timestamp}/{timescale} が {recovered} に戻らないこと"
            );
        }
    }

    #[test]
    fn test_duration_from_timestamp_accepts_fraction() {
        // 0.5 秒を 0.5 秒のまま扱えること
        let duration = duration_from_timestamp(0.5, 1.0).expect("小数を受理すること");
        assert_eq!(duration, std::time::Duration::from_millis(500));

        // 48000 分の 24000 は 0.5 秒
        let duration = duration_from_timestamp(24000.0, 48000.0).expect("整数を受理すること");
        assert_eq!(duration, std::time::Duration::from_millis(500));
    }

    #[test]
    fn test_duration_from_timestamp_rejects_non_finite() {
        assert!(duration_from_timestamp(f64::NAN, 48000.0).is_err());
        assert!(duration_from_timestamp(f64::INFINITY, 48000.0).is_err());
    }

    #[test]
    fn test_duration_from_timestamp_rejects_negative() {
        let error = duration_from_timestamp(-1.0, 48000.0).expect_err("負値は拒否されること");
        assert!(error.contains("greater than or equal to 0"), "{error}");
    }

    #[test]
    fn test_duration_from_timestamp_rejects_invalid_timescale() {
        // 0.5 は正の値だが、タイムスケールは 1 以上の整数でなければならない
        let error = duration_from_timestamp(1.0, 0.5).expect_err("1 未満は拒否されること");
        assert!(error.contains("greater than or equal to 1"), "{error}");
        // 1.5 のような小数も拒否する
        let error = duration_from_timestamp(1.0, 1.5).expect_err("小数は拒否されること");
        assert!(error.contains("must be an integer"), "{error}");
        assert!(duration_from_timestamp(1.0, 0.0).is_err());
    }
}
