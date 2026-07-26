//! 正则提取 — 从文本中通过正则表达式提取内容
//!
//! 支持捕获组，返回第一个匹配的捕获组（若有），否则返回整个匹配。

use regex::Regex;

use crate::error::EvalError;

/// 按正则从文本中提取内容。
///
/// 若正则包含捕获组，返回第一个捕获组的内容；
/// 否则返回整个匹配的内容。
///
/// # Arguments
///
/// * `text` - 待提取的源文本。
/// * `pattern` - 正则表达式字符串，可包含捕获组。
///
/// # Returns
///
/// 成功时返回提取到的字符串。
///
/// # Errors
///
/// 当正则无效时返回 [`EvalError::InvalidParams`]；当无匹配时返回 [`EvalError::RegexNoMatch`]。
pub fn extract_by_regex(text: &str, pattern: &str) -> Result<String, EvalError> {
    let re = Regex::new(pattern)
        .map_err(|e| EvalError::InvalidParams(format!("无效的正则表达式: {e}")))?;

    let caps = re.captures(text).ok_or(EvalError::RegexNoMatch)?;

    // 优先返回第一个捕获组，否则返回整个匹配
    let matched = caps
        .get(1)
        .or_else(|| caps.get(0))
        .ok_or(EvalError::RegexNoMatch)?;

    Ok(matched.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_with_capture_group() {
        let text = "Answer: 42";
        let result = extract_by_regex(text, r"Answer:\s*(.+)").unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_extract_without_capture_group() {
        let text = "hello world";
        let result = extract_by_regex(text, r"world").unwrap();
        assert_eq!(result, "world");
    }

    #[test]
    fn test_no_match() {
        let text = "hello world";
        let result = extract_by_regex(text, r"nonexistent");
        assert!(matches!(result, Err(EvalError::RegexNoMatch)));
    }

    #[test]
    fn test_invalid_regex() {
        let result = extract_by_regex("test", r"[");
        assert!(matches!(result, Err(EvalError::InvalidParams(_))));
    }
}
