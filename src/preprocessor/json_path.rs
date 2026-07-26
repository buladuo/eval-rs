//! JSON 路径提取 — 按点分路径从 JSON 中提取字段值
//!
//! 支持点分路径，如 "data.response.text"。

use serde_json::Value;

use crate::error::EvalError;

/// 按点分路径从 JSON 中提取值。
///
/// 支持点分路径，如 `"data.response.text"`。
///
/// # Arguments
///
/// * `input` - 待提取的 JSON 值。
/// * `path` - 点分路径字符串。
///
/// # Returns
///
/// 成功时返回路径指向的值的克隆。
///
/// # Errors
///
/// 当路径中任意一段键不存在时返回 [`EvalError::JsonPathNotFound`]。
///
/// # Examples
///
/// ```ignore
/// let json: Value = serde_json::from_str(r#"{"data": {"response": {"text": "hello"}}}"#).unwrap();
/// let value = extract_by_path(&json, "data.response.text").unwrap();
/// assert_eq!(value, Value::String("hello".to_string()));
/// ```
pub fn extract_by_path(input: &Value, path: &str) -> Result<Value, EvalError> {
    let mut current = input;

    for key in path.split('.') {
        current = current
            .get(key)
            .ok_or_else(|| EvalError::JsonPathNotFound(path.to_string()))?;
    }

    Ok(current.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_nested() {
        let input = json!({"data": {"response": {"text": "hello"}}});
        let result = extract_by_path(&input, "data.response.text").unwrap();
        assert_eq!(result, json!("hello"));
    }

    #[test]
    fn test_extract_simple() {
        let input = json!({"name": "test"});
        let result = extract_by_path(&input, "name").unwrap();
        assert_eq!(result, json!("test"));
    }

    #[test]
    fn test_path_not_found() {
        let input = json!({"name": "test"});
        let result = extract_by_path(&input, "nonexistent.path");
        assert!(matches!(result, Err(EvalError::JsonPathNotFound(_))));
    }
}
