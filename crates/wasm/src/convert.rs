//! JavaScript の数値を Rust の整数へ安全に変換するヘルパー
//!
//! JavaScript の数値は f64 であり、そのまま `as u64` などで変換すると
//! 負値・`NaN`・小数が黙って丸められ、利用者の誤りに気付けない。
//! 公開 API の入口では必ずこのモジュールの関数を通す。
use wasm_bindgen::JsError;

/// f64 から u64 の範囲を判定するための排他的上限（2 の 64 乗）
///
/// `u64::MAX as f64` は丸めの結果 2 の 64 乗になるため、比較にはこの値を使う。
pub(crate) const U64_LIMIT: f64 = 18_446_744_073_709_551_616.0;

/// 0 以上の整数として解釈できる f64 を u64 に変換する
pub(crate) fn to_u64(value: f64, name: &str) -> Result<u64, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    if value < 0.0 {
        return Err(format!("{name} must be greater than or equal to 0"));
    }
    if value.fract() != 0.0 {
        return Err(format!("{name} must be an integer"));
    }
    if value >= U64_LIMIT {
        return Err(format!("{name} exceeds u64::MAX"));
    }
    Ok(value as u64)
}

/// 0 以上の整数として解釈できる f64 を usize に変換する
///
/// wasm32 では `usize` が 32 bit なので、上限もそこに合わせる。
pub(crate) fn to_usize(value: f64, name: &str) -> Result<usize, String> {
    let value = to_u64(value, name)?;
    usize::try_from(value).map_err(|_| format!("{name} exceeds usize::MAX"))
}

/// 1 以上の整数として解釈できる f64 を u32 に変換する
pub(crate) fn to_positive_u32(value: f64, name: &str) -> Result<u32, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    if value < 1.0 {
        return Err(format!("{name} must be greater than or equal to 1"));
    }
    if value.fract() != 0.0 {
        return Err(format!("{name} must be an integer"));
    }
    if value > u32::MAX as f64 {
        return Err(format!("{name} exceeds u32::MAX"));
    }
    Ok(value as u32)
}

/// 上記の変換結果を `JsError` に変換する
pub(crate) fn js_error(message: String) -> JsError {
    JsError::new(&message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_u64_accepts_integers() {
        assert_eq!(to_u64(0.0, "position").expect("0 は受理されること"), 0);
        assert_eq!(
            to_u64(1_099_511_627_776.0, "position").expect("2 の 40 乗は受理されること"),
            1_099_511_627_776
        );
    }

    #[test]
    fn test_to_u64_rejects_negative() {
        let error = to_u64(-1.0, "position").expect_err("負値は拒否されること");
        assert!(error.contains("greater than or equal to 0"), "{error}");
    }

    #[test]
    fn test_to_u64_rejects_fraction() {
        let error = to_u64(1.5, "position").expect_err("小数は拒否されること");
        assert!(error.contains("must be an integer"), "{error}");
    }

    #[test]
    fn test_to_u64_rejects_non_finite() {
        assert!(
            to_u64(f64::NAN, "position").is_err(),
            "NaN は拒否されること"
        );
        assert!(
            to_u64(f64::INFINITY, "position").is_err(),
            "無限大は拒否されること"
        );
    }

    #[test]
    fn test_to_positive_u32_rejects_zero() {
        let error = to_positive_u32(0.0, "timescale").expect_err("0 は拒否されること");
        assert!(error.contains("greater than or equal to 1"), "{error}");
    }

    #[test]
    fn test_to_positive_u32_rejects_fraction() {
        assert!(
            to_positive_u32(0.5, "timescale").is_err(),
            "小数の timescale は拒否されること"
        );
    }
}
