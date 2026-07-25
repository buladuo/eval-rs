//! 输入预处理模块 — 支持 JSON 路径提取、正则提取、格式转换

pub mod converter;
pub mod json_path;
pub mod regex_extractor;

use serde::Deserialize;
use serde_json::Value;

use crate::error::EvalError;

/// 预处理配置
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PreprocessConfig {
    /// JSON 路径提取
    JsonPath { json_path: String },
    /// 正则提取
    Regex { regex: String },
}

/// 对输入执行预处理
///
/// 根据配置对输入进行提取和转换，返回处理后的值。
pub fn preprocess(input: &Value, config: &PreprocessConfig) -> Result<Value, EvalError> {
    match config {
        PreprocessConfig::JsonPath { json_path } => {
            let extracted = json_path::extract_by_path(input, json_path)?;
            // 如果提取结果是 JSON 字符串，尝试解析为 JSON 对象
            converter::auto_convert(extracted)
        }
        PreprocessConfig::Regex { regex } => {
            let text = input
                .as_str()
                .ok_or_else(|| EvalError::InvalidParams("正则提取要求输入为字符串".to_string()))?;
            let extracted = regex_extractor::extract_by_regex(text, regex)?;
            converter::auto_convert(Value::String(extracted))
        }
    }
}
