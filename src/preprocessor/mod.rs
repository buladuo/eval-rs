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

/// 对输入执行预处理。
///
/// 根据 [`PreprocessConfig`] 配置对输入进行提取与转换，返回处理后的值。
/// 当配置为 [`PreprocessConfig::JsonPath`] 时按点分路径提取并自动转换；
/// 当配置为 [`PreprocessConfig::Regex`] 时要求输入为字符串并按正则提取。
///
/// # Arguments
///
/// * `input` - 待处理的输入数据（`serde_json::Value`）。
/// * `config` - 预处理配置，决定提取与转换方式。
///
/// # Returns
///
/// 成功时返回处理后的 [`serde_json::Value`]。
///
/// # Errors
///
/// 当 JSON 路径不存在、输入非字符串却使用正则提取、或正则表达式无效时返回 [`EvalError`]。
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
