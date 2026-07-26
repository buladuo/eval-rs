//! 格式转换 — 提取结果的自动格式转换
//!
//! 支持将提取的字符串自动转换为 JSON 对象（若内容是合法 JSON）。

use serde_json::Value;

use crate::error::EvalError;

/// 自动转换格式。
///
/// 若输入是字符串且内容为合法 JSON，则解析为 JSON 对象；
/// 否则保持原值不变。
///
/// # Arguments
///
/// * `value` - 待转换的值。
///
/// # Returns
///
/// 转换后的 [`serde_json::Value`]。若无法解析为 JSON 则原样返回。
///
/// # Errors
///
/// 该函数不会返回错误（始终返回 `Ok`）。
pub fn auto_convert(value: Value) -> Result<Value, EvalError> {
    match value {
        Value::String(ref s) => {
            // 尝试解析为 JSON
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                return Ok(parsed);
            }
            Ok(value)
        }
        other => Ok(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_convert_json_string() {
        let input = json!(r#"{"key": "value"}"#);
        let result = auto_convert(input).unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[test]
    fn test_keep_plain_string() {
        let input = json!("hello world");
        let result = auto_convert(input).unwrap();
        assert_eq!(result, json!("hello world"));
    }

    #[test]
    fn test_keep_non_string() {
        let input = json!(42);
        let result = auto_convert(input).unwrap();
        assert_eq!(result, json!(42));
    }
}
