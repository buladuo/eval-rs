//! 多步 LLM-as-Judge 指标模块
//!
//! 基于 RAGAS 官方多步流程实现（多 prompt 调用 + 数学聚合），
//! 指标命名扁平化，不区分来源。
//!
//! # 支持的指标
//!
//! | 指标 | 步骤 | 公式 |
//! |------|------|------|
//! | `faithfulness` | 2 步 | 支持的 claims / 总 claims |
//! | `answer_relevancy` | 2 步 | 匹配的问题 / 总生成问题 |
//! | `context_precision` | N 步 | Σ(Precision@k × v_k) / 相关总数 |
//! | `context_recall` | 2 步 | 支持的 claims / 总 claims |
//! | `context_relevancy` | N 步 | 相关 chunk 数 / 总 chunk 数 |
//! | `context_entities_recall` | 2 步 | |RCE ∩ RE| / |RE| |
//! | `noise_sensitivity` | 2 步 | 错误 claims / 总 claims |
//! | `answer_accuracy` | 4 步 | |TP| / (|TP| + 0.5·(|FP|+|FN|)) |
//! | `summarization_score` | 3 步 | 正确问答 / 总问题 |
//!
//! # 共享工具
//!
//! - [`llm_complete`] — 渲染提示词并调用 LLM
//! - [`parse_json`] — 从 LLM 响应中提取 JSON 对象
//! - [`parse_verdict`] — 解析二值判定（0 或 1）

pub mod answer_accuracy;
pub mod answer_relevancy;
pub mod context_entities_recall;
pub mod context_precision;
pub mod context_recall;
pub mod context_relevancy;
pub mod faithfulness;
pub mod noise_sensitivity;
pub mod summarization_score;

pub use answer_accuracy::AnswerAccuracy;
pub use answer_relevancy::AnswerRelevancy;
pub use context_entities_recall::ContextEntitiesRecall;
pub use context_precision::ContextPrecision;
pub use context_recall::ContextRecall;
pub use context_relevancy::ContextRelevancy;
pub use faithfulness::Faithfulness;
pub use noise_sensitivity::NoiseSensitivity;
pub use summarization_score::SummarizationScore;

use std::sync::Arc;

use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// 渲染提示词模板并调用 LLM，返回原始响应文本
///
/// # Arguments
///
/// * `providers` - Provider 管理器
/// * `prompts` - 提示词注册表
/// * `prompt_name` - 提示词名称
/// * `vars` - 模板渲染变量
/// * `provider` - 可选 provider 名
/// * `model` - 可选模型名
/// * `version` - 可选提示词版本
///
/// # Errors
///
/// 提示词未找到、渲染失败、LLM 调用失败等。
pub async fn llm_complete(
    providers: &Arc<ProviderManager>,
    prompts: &Arc<PromptRegistry>,
    prompt_name: &str,
    vars: tera::Context,
    provider: Option<&str>,
    model: Option<&str>,
    version: Option<&str>,
) -> Result<String, EvalError> {
    let template = prompts.get(prompt_name, version)?;
    let rendered = prompts.render(template, vars)?;
    providers.complete(provider, &rendered, model, None).await
}

/// 从 LLM 响应中解析 JSON 对象
///
/// 支持以下格式：
/// - `{"key": "value"}` — 纯 JSON
/// - ```` ```json\n{...}\n``` ```` — 代码块中的 JSON
/// - 文本中嵌入的 `{...}` 块
///
/// # Arguments
///
/// * `response` - LLM 原始响应文本
///
/// # Returns
///
/// 解析成功返回 `serde_json::Value`；失败返回错误。
pub fn parse_json(response: &str) -> Result<serde_json::Value, EvalError> {
    let trimmed = response.trim();

    // 完整 JSON
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }

    // 代码块中的 JSON
    if let Some(v) = extract_code_block_json(trimmed) {
        return Ok(v);
    }

    // 文本中第一个 {...}
    if let Some(v) = extract_inline_json(trimmed) {
        return Ok(v);
    }

    Err(EvalError::JudgeParseFailed(format!(
        "无法从 LLM 响应中提取 JSON: {trimmed}"
    )))
}

/// 从 LLM 响应中解析二值判定（verdict 字段）
///
/// 支持 `verdict` / `correct` / `relevant` 等字段名，
/// 值为 1/0 或 true/false。
pub fn parse_verdict(json: &serde_json::Value, fields: &[&str]) -> Result<bool, EvalError> {
    for field in fields {
        if let Some(v) = json.get(field) {
            return match v {
                serde_json::Value::Number(n) => n
                    .as_i64()
                    .map(|i| i != 0)
                    .or_else(|| n.as_f64().map(|f| f > 0.5))
                    .ok_or_else(|| {
                        EvalError::JudgeParseFailed(format!("verdict 不是有效数字: {v}"))
                    }),
                serde_json::Value::Bool(b) => Ok(*b),
                serde_json::Value::String(s) => {
                    let lower = s.to_lowercase();
                    Ok(lower == "1" || lower == "true" || lower == "yes")
                }
                _ => Err(EvalError::JudgeParseFailed(format!(
                    "verdict 类型不支持: {v}"
                ))),
            };
        }
    }
    Err(EvalError::JudgeParseFailed(format!(
        "JSON 中未找到 verdict 字段 (尝试: {fields:?}): {json}"
    )))
}

/// 从代码块中提取 JSON（```json {...} ``` 或 ``` {...} ```）
fn extract_code_block_json(text: &str) -> Option<serde_json::Value> {
    let re = regex::Regex::new(r"```(?:json)?\s*(\{.*?\})\s*```").ok()?;
    let caps = re.captures(text)?;
    let m = caps.get(1)?;
    serde_json::from_str::<serde_json::Value>(m.as_str()).ok()
}

/// 从文本中提取第一个 {...} 块
fn extract_inline_json(text: &str) -> Option<serde_json::Value> {
    let re = regex::Regex::new(r"(\{[^{}]*\})").ok()?;
    let caps = re.captures(text)?;
    let m = caps.get(1)?;
    serde_json::from_str::<serde_json::Value>(m.as_str()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_plain() {
        let v = parse_json(r#"{"score": 0.9}"#).unwrap();
        assert_eq!(v["score"].as_f64().unwrap(), 0.9);
    }

    #[test]
    fn test_parse_json_code_block() {
        let v = parse_json("```json\n{\"score\": 0.9}\n```").unwrap();
        assert_eq!(v["score"].as_f64().unwrap(), 0.9);
    }

    #[test]
    fn test_parse_json_inline() {
        let v = parse_json("结果：{\"score\": 0.8} 完成。").unwrap();
        assert_eq!(v["score"].as_f64().unwrap(), 0.8);
    }

    #[test]
    fn test_parse_verdict_number() {
        assert!(parse_verdict(&serde_json::json!({"verdict": 1}), &["verdict"]).unwrap());
        assert!(!parse_verdict(&serde_json::json!({"verdict": 0}), &["verdict"]).unwrap());
    }

    #[test]
    fn test_parse_verdict_bool() {
        assert!(parse_verdict(&serde_json::json!({"correct": true}), &["correct"]).unwrap());
    }

    #[test]
    fn test_parse_verdict_string() {
        assert!(parse_verdict(&serde_json::json!({"relevant": "yes"}), &["relevant"]).unwrap());
        assert!(!parse_verdict(&serde_json::json!({"relevant": "no"}), &["relevant"]).unwrap());
    }
}
