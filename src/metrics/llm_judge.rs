//! LLM-as-Judge 指标 — 通过提示词模板调用 LLM 进行评测
//!
//! 支持通过提示词模板定义不同的 judge 指标，解析 LLM 返回的评分。
//!
//! # 约定
//!
//! 以 `llm_judge_` 为前缀的提示词模板会被自动注册为 LLM-as-Judge 指标。
//! 例如 `prompts/llm_judge_accuracy.toml` → 指标名 `llm_judge_accuracy`。
//!
//! # 评分解析
//!
//! LLM 响应解析顺序详见 [`parse_judge_response`]。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::registry::{Metric, MetricOutput};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// LLM-as-Judge 指标
///
/// 通过提示词模板渲染输入并调用 LLM，解析其返回的评分。
pub struct LlmJudgeMetric {
    /// 指标名称（如 "llm_judge_accuracy"）
    name: String,
    /// 关联的提示词名称（需在 [`PromptRegistry`] 中注册）
    prompt_name: String,
    /// 关联的提示词版本（可选，None 表示使用最新版本）
    prompt_version: Option<String>,
    /// Provider 管理器引用
    providers: std::sync::Arc<ProviderManager>,
    /// 提示词注册表引用
    prompts: std::sync::Arc<PromptRegistry>,
}

impl LlmJudgeMetric {
    /// 创建新的 LLM-as-Judge 指标
    ///
    /// # Arguments
    ///
    /// * `name` - 指标对外名称
    /// * `prompt_name` - 使用的提示词模板名
    /// * `prompt_version` - 版本号，None 取最新
    /// * `providers` - Provider 管理器
    /// * `prompts` - 提示词注册表
    pub fn new(
        name: String,
        prompt_name: String,
        prompt_version: Option<String>,
        providers: std::sync::Arc<ProviderManager>,
        prompts: std::sync::Arc<PromptRegistry>,
    ) -> Self {
        Self {
            name,
            prompt_name,
            prompt_version,
            providers,
            prompts,
        }
    }
}

#[async_trait]
impl Metric for LlmJudgeMetric {
    fn name(&self) -> &str {
        &self.name
    }

    fn metric_type(&self) -> &str {
        "llm"
    }

    /// 参数 schema
    ///
    /// 支持以下参数：
    /// - `provider`：LLM provider 名称
    /// - `model`：模型名称
    /// - `prompt_version`：提示词版本
    /// - `response_regex`：用于提取评分的正则（优先使用）
    /// - `score_scale`：评分量程上限（评分 > 1 时的归一化分母，默认 100）
    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "provider".to_string(),
            json!({"type": "string", "description": "使用的 LLM provider 名称", "required": false}),
        );
        schema.insert(
            "model".to_string(),
            json!({"type": "string", "description": "使用的模型名称", "required": false}),
        );
        schema.insert(
            "prompt_version".to_string(),
            json!({"type": "string", "description": "提示词版本", "required": false}),
        );
        schema.insert(
            "response_regex".to_string(),
            json!({
                "type": "string",
                "description": "用于从 LLM 响应中提取评分的正则表达式。\
                若提供，则优先使用该正则提取评分（取第一个捕获组或整体匹配）。\
                若未提供，则按 JSON / 纯数字 / 默认数字提取顺序解析。",
                "required": false
            }),
        );
        schema.insert(
            "score_scale".to_string(),
            json!({
                "type": "number",
                "description": "评分量程上限。若 LLM 返回的评分 > 1，\
                且未提供 response_regex，则按 score_scale 进行归一化（默认 100）。",
                "required": false
            }),
        );
        schema
    }

    /// 通过 LLM 评估输入并解析评分
    async fn evaluate(
        &self,
        params: &HashMap<String, Value>,
        input: &Value,
    ) -> Result<MetricOutput, EvalError> {
        let provider = params.get("provider").and_then(|v| v.as_str());
        let model = params.get("model").and_then(|v| v.as_str());
        let version = params
            .get("prompt_version")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or_else(|| self.prompt_version.clone());
        let response_regex = params.get("response_regex").and_then(|v| v.as_str());
        let score_scale = params
            .get("score_scale")
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);

        // 加载提示词模板（按名称 + 可选版本）
        let template = self.prompts.get(&self.prompt_name, version.as_deref())?;

        // 准备模板变量：将 input 展开到 tera context
        let mut context = tera::Context::new();
        if let Some(obj) = input.as_object() {
            for (k, v) in obj {
                context.insert(k, v);
            }
        } else {
            // input 非对象时作为 "input" 变量
            context.insert("input", input);
        }

        // 渲染提示词
        let prompt = self.prompts.render(template, context)?;

        // 调用 LLM
        let response = self
            .providers
            .complete(provider, &prompt, model, None)
            .await?;

        // 解析评分（正则优先，fallback 到 JSON/数字/标签解析）
        let (score, parsed_details) = parse_judge_response(&response, response_regex, score_scale)?;

        Ok(MetricOutput {
            score,
            details: json!({
                "raw_response": response,
                "parsed": parsed_details,
                "prompt_name": self.prompt_name,
                "prompt_version": template.version,
                "response_regex": response_regex,
            }),
        })
    }
}

/// 解析 LLM judge 的响应
///
/// 解析顺序：
/// 1. 若提供 `response_regex`，则优先用该正则从响应中提取评分
///    - 取第一个捕获组（若有），否则取整体匹配
///    - 提取出的字符串再尝试解析为 JSON 或数字
///    - 若正则未匹配或提取后解析失败，**fallback** 到默认解析流程
/// 2. 默认解析流程：
///    - 完整 JSON：`{"score": 0.85, "reason": "..."}`
///    - 嵌入 JSON（代码块或文本中）：```` ```json\n{...}\n``` ```` 或 `{...}`
///    - 纯数字：`0.85` 或 `85`（>1 时按 `score_scale` 归一化）
///    - 带标签的数字：`Score: 0.75`
///
/// # Arguments
///
/// * `response` - LLM 原始响应文本
/// * `response_regex` - 可选的自定义正则表达式
/// * `score_scale` - 评分量程上限（用于 >1 数字的归一化，默认 100）
///
/// # Returns
///
/// `(score, details)` 其中 `score ∈ [0.0, 1.0]`（归一化后）
fn parse_judge_response(
    response: &str,
    response_regex: Option<&str>,
    score_scale: f64,
) -> Result<(f64, Value), EvalError> {
    let trimmed = response.trim();

    // 1. 优先使用自定义正则提取，失败则 fallback 到默认解析
    if let Some(regex_str) = response_regex
        && let Ok(re) = regex::Regex::new(regex_str)
        && let Some(caps) = re.captures(trimmed)
        && let Some(captured) = caps.get(1).or_else(|| caps.get(0))
    {
        let text = captured.as_str().to_string();
        // 对提取出的字符串尝试 JSON / 数字解析
        if let Ok(result) = parse_extracted(&text, score_scale, Some(regex_str)) {
            return Ok(result);
        }
        // 正则未匹配或提取后解析失败，fallback 到默认解析
    }

    // 2. 默认解析
    parse_extracted(trimmed, score_scale, None)
}

/// 对提取出的字符串进行 JSON / 数字解析
///
/// `source_regex` 仅用于在 details 中标注来源，可为 None。
fn parse_extracted(
    text: &str,
    score_scale: f64,
    source_regex: Option<&str>,
) -> Result<(f64, Value), EvalError> {
    let trimmed = text.trim();

    // 2.1 完整 JSON 对象
    if let Ok(json_val) = serde_json::from_str::<Value>(trimmed)
        && let Some(raw_score) = extract_score_from_json(&json_val)
    {
        let score = normalize_score(raw_score, score_scale);
        return Ok((
            score,
            json!({
                "source": "json",
                "source_regex": source_regex,
                "score": score,
                "raw_score": raw_score,
                "raw_json": json_val,
            }),
        ));
    }

    // 2.2 嵌入 JSON（代码块或文本中）
    if let Some(json_val) = extract_embedded_json(trimmed)
        && let Some(raw_score) = extract_score_from_json(&json_val)
    {
        let score = normalize_score(raw_score, score_scale);
        return Ok((
            score,
            json!({
                "source": "embedded_json",
                "source_regex": source_regex,
                "score": score,
                "raw_score": raw_score,
                "raw_json": json_val,
            }),
        ));
    }

    // 2.3 纯数字
    if let Ok(num) = trimmed.parse::<f64>() {
        let normalized = normalize_score(num, score_scale);
        return Ok((
            normalized,
            json!({
                "source": "number",
                "source_regex": source_regex,
                "raw_number": num,
                "normalized": normalized,
                "score_scale": score_scale,
            }),
        ));
    }

    // 2.4 带标签的数字（如 "Score: 0.75"）
    let num_match = regex::Regex::new(r"(\d+\.?\d*)")
        .map_err(|e| EvalError::Internal(e.to_string()))?
        .captures(trimmed)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse::<f64>().ok());

    if let Some(num) = num_match {
        let normalized = normalize_score(num, score_scale);
        return Ok((
            normalized,
            json!({
                "source": "extracted_number",
                "source_regex": source_regex,
                "extracted_number": num,
                "normalized": normalized,
                "score_scale": score_scale,
            }),
        ));
    }

    Err(EvalError::JudgeParseFailed(format!(
        "无法从响应中解析评分: {trimmed}"
    )))
}

/// 从 JSON 对象中提取 score 字段
///
/// 支持以下字段名（按优先级）：
/// - `score` / `Score` / `SCORE`
/// - `rating` / `Rating`
/// - `evaluation` / `Evaluation`
/// - `result` / `Result`
fn extract_score_from_json(json_val: &Value) -> Option<f64> {
    let candidates = [
        "score",
        "Score",
        "SCORE",
        "rating",
        "Rating",
        "evaluation",
        "Evaluation",
        "result",
        "Result",
    ];
    for key in candidates {
        if let Some(v) = json_val.get(key) {
            if let Some(n) = v.as_f64() {
                return Some(n);
            }
            // 字符串形式的数字也尝试解析
            if let Some(s) = v.as_str()
                && let Ok(n) = s.trim().parse::<f64>()
            {
                return Some(n);
            }
        }
    }
    None
}

/// 从文本中提取嵌入的 JSON 对象
///
/// 支持以下格式：
/// - ```` ```json\n{...}\n``` ````
/// - ```` ```\n{...}\n``` ````
/// - 文本中第一个 `{...}` 块
fn extract_embedded_json(text: &str) -> Option<Value> {
    // 代码块中的 JSON
    let code_block_re = regex::Regex::new(r"```(?:json)?\s*(\{.*?\})\s*```").ok()?;
    if let Some(caps) = code_block_re.captures(text)
        && let Some(m) = caps.get(1)
        && let Ok(v) = serde_json::from_str::<Value>(m.as_str())
    {
        return Some(v);
    }

    // 文本中第一个 {...} 块（非贪婪）
    let json_block_re = regex::Regex::new(r"(\{[^{}]*\})").ok()?;
    if let Some(caps) = json_block_re.captures(text)
        && let Some(m) = caps.get(1)
        && let Ok(v) = serde_json::from_str::<Value>(m.as_str())
    {
        return Some(v);
    }
    None
}

/// 将评分归一化到 0-1 范围
///
/// - 若 `num ≤ 1.0`，直接返回
/// - 若 `num > 1.0`，按 `score_scale` 归一化（默认 100）
fn normalize_score(num: f64, score_scale: f64) -> f64 {
    if num <= 1.0 {
        num
    } else if score_scale > 0.0 {
        num / score_scale
    } else {
        num / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pure_number() {
        let (score, _) = parse_judge_response("0.85", None, 100.0).unwrap();
        assert!((score - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_parse_percentage() {
        let (score, _) = parse_judge_response("85", None, 100.0).unwrap();
        assert!((score - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_parse_percentage_custom_scale() {
        // 5 分制，4 -> 4/5 = 0.8
        let (score, _) = parse_judge_response("4", None, 5.0).unwrap();
        assert!((score - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_parse_json() {
        let (score, _) =
            parse_judge_response(r#"{"score": 0.9, "reason": "good"}"#, None, 100.0).unwrap();
        assert!((score - 0.9).abs() < 0.001);
    }

    #[test]
    fn test_parse_json_with_label() {
        let (score, _) = parse_judge_response(r#"{"Score": 0.78}"#, None, 100.0).unwrap();
        assert!((score - 0.78).abs() < 0.001);
    }

    #[test]
    fn test_parse_json_string_score() {
        let (score, _) = parse_judge_response(r#"{"score": "0.88"}"#, None, 100.0).unwrap();
        assert!((score - 0.88).abs() < 0.001);
    }

    #[test]
    fn test_parse_json_rating_field() {
        let (score, _) = parse_judge_response(r#"{"rating": 4, "max": 5}"#, None, 5.0).unwrap();
        assert!((score - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_parse_embedded_json_code_block() {
        let resp = "评估完成。\n```json\n{\"score\": 0.92, \"reason\": \"准确\"}\n```\n谢谢。";
        let (score, _) = parse_judge_response(resp, None, 100.0).unwrap();
        assert!((score - 0.92).abs() < 0.001);
    }

    #[test]
    fn test_parse_embedded_json_plain_block() {
        let resp = "结果如下：{\"score\": 0.65, \"reason\": \"部分正确\"} 已完成。";
        let (score, _) = parse_judge_response(resp, None, 100.0).unwrap();
        assert!((score - 0.65).abs() < 0.001);
    }

    #[test]
    fn test_parse_with_label() {
        let (score, _) = parse_judge_response("Score: 0.75", None, 100.0).unwrap();
        assert!((score - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_parse_with_regex_capture_group() {
        // 用捕获组提取数字
        let (score, _) = parse_judge_response(
            "评分结果是：0.83 分。",
            Some(r"评分结果是：(\d+\.?\d*)\s*分"),
            100.0,
        )
        .unwrap();
        assert!((score - 0.83).abs() < 0.001);
    }

    #[test]
    fn test_parse_with_regex_whole_match() {
        // 无捕获组，整体匹配后解析为数字
        let (score, _) = parse_judge_response("0.91", Some(r"\d+\.\d+"), 100.0).unwrap();
        assert!((score - 0.91).abs() < 0.001);
    }

    #[test]
    fn test_parse_with_regex_extract_json() {
        // 用正则提取 JSON 片段
        let resp = "```json\n{\"score\": 0.77, \"reason\": \"ok\"}\n```";
        let (score, _) = parse_judge_response(resp, Some(r"\{[^{}]*\}"), 100.0).unwrap();
        assert!((score - 0.77).abs() < 0.001);
    }

    #[test]
    fn test_parse_regex_no_match_fallback_to_json() {
        // 正则不匹配，但响应是 JSON，应 fallback 到 JSON 解析
        let resp = r#"{"score": 0.88, "reason": "ok"}"#;
        let (score, details) =
            parse_judge_response(resp, Some(r"评分\s*[:：]\s*(\d+)"), 100.0).unwrap();
        assert!((score - 0.88).abs() < 0.001);
        // 走的是 fallback 的 JSON 路径，source 应为 "json"
        assert_eq!(details.get("source").and_then(|v| v.as_str()), Some("json"));
    }

    #[test]
    fn test_parse_regex_no_match_fallback_to_number() {
        // 正则不匹配，但响应是纯数字，应 fallback 到数字解析
        let (score, details) =
            parse_judge_response("0.78", Some(r"评分\s*[:：]\s*(\d+)"), 100.0).unwrap();
        assert!((score - 0.78).abs() < 0.001);
        assert_eq!(
            details.get("source").and_then(|v| v.as_str()),
            Some("number")
        );
    }

    #[test]
    fn test_parse_regex_invalid_falls_back() {
        // 无效正则应 fallback 到默认解析，而不是报错
        let (score, _) = parse_judge_response("0.5", Some(r"("), 100.0).unwrap();
        assert!((score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_parse_failed() {
        let result = parse_judge_response("no score here", None, 100.0);
        assert!(result.is_err());
    }
}
