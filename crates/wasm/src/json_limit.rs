//! JSON 入力の事前検査
//!
//! `nojson` のパーサは再帰実装であり、ネストの深さに上限を持たない。
//! 深くネストした JSON を渡すと wasm のスタックが枯渇して trap し、
//! 同じインスタンス上の他のオブジェクトも操作できなくなる。
//! パース前に線形走査で上限を検査し、trap ではなく例外として返す。
/// JSON のネストの上限
///
/// このクレートが扱うスキーマの深さは数段であり、128 でも十分に余裕がある。
const MAX_DEPTH: usize = 128;

/// JSON 文字列の長さの上限（8 MiB）
///
/// wasm32 の線形メモリは 4 GiB であり、巨大な入力をそのままパースすると
/// メモリ不足で abort してオブジェクトを解放できなくなる。
const MAX_LENGTH: usize = 8 * 1024 * 1024;

/// JSON 文字列が上限に収まっているか検査する
///
/// 文字列リテラルの中の括弧は深さに数えない。
pub(crate) fn check(text: &str) -> Result<(), String> {
    if text.len() > MAX_LENGTH {
        return Err(format!("JSON must be at most {MAX_LENGTH} bytes"));
    }

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > MAX_DEPTH {
                    return Err(format!("JSON nesting depth must be at most {MAX_DEPTH}"));
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accepts_normal_json() {
        check(r#"{"track_kind":"video","sample_entry":{"kind":"avc1"}}"#).expect("通常の JSON");
    }

    #[test]
    fn test_rejects_deep_nesting() {
        let deep = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        let error = check(&deep).expect_err("深すぎる JSON は拒否されること");
        assert!(error.contains("nesting depth"), "{error}");
    }

    #[test]
    fn test_accepts_depth_at_limit() {
        let text = format!("{}1{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        check(&text).expect("上限ちょうどは受理されること");
    }

    #[test]
    fn test_ignores_brackets_in_string_literal() {
        // 文字列リテラルの中の括弧は深さに数えない
        let text = format!(r#"{{"x":"{}"}}"#, "[".repeat(1000));
        check(&text).expect("文字列内の括弧は無視されること");
    }

    #[test]
    fn test_ignores_escaped_quote() {
        // エスケープされた引用符で文字列の状態を誤らないこと
        let text = format!(r#"{{"x":"a\"{}"}}"#, "[".repeat(1000));
        check(&text).expect("エスケープを考慮すること");
    }

    #[test]
    fn test_rejects_too_long_json() {
        let text = " ".repeat(MAX_LENGTH + 1);
        let error = check(&text).expect_err("長すぎる JSON は拒否されること");
        assert!(error.contains("at most"), "{error}");
    }
}
