//! Noise Sensitivity（噪声敏感度）指标
//!
//! 评估模型回答被无关上下文（噪声）影响而产生错误的程度。分数越低越好。
//!
//! # 流程（2 步 LLM + 数学聚合）
//!
//! 1. 从 response 中分解出声明（claims）
//! 2. 逐条验证每个声明是否基于 ground_truth 正确
//! 3. `score = 不正确的声明数 / 总声明数`（越低越好）
//!
//! # 输入字段
//!
//! - `question` — 用户问题
//! - `reference` — 正确答案（ground truth）
//! - `contexts` — 检索到的上下文列表（包含噪声，字符串数组）
//! - `answer` — 模型回答

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json, parse_verdict};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Noise Sensitivity（噪声敏感度）指标
///
/// 基于多步 LLM-as-Judge 流程实现的评测指标，实现了
/// [`super::super::registry::Metric`] trait。持有一个 LLM provider 管理器与一个提示词
/// 注册表的 [`Arc`] 共享引用，在执行 [`Metric::evaluate`] 时用于渲染并调用 LLM
/// 完成逐声明验证，最后做计数聚合。分数越低越好。
pub struct NoiseSensitivity {
    /// LLM provider 管理器（共享引用），用于调用 LLM 完成评测。
    providers: Arc<ProviderManager>,
    /// 提示词注册表（共享引用），按名称渲染 judge 提示词模板。
    prompts: Arc<PromptRegistry>,
}

impl NoiseSensitivity {
    /// 创建一个新的 Noise Sensitivity 指标实例。
    ///
    /// # Arguments
    ///
    /// * `providers` - LLM provider 管理器共享引用。
    /// * `prompts` - 提示词注册表共享引用。
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for NoiseSensitivity {
    fn name(&self) -> &str {
        "noise_sensitivity"
    }

    fn metric_type(&self) -> &str {
        "llm"
    }

    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "provider".to_string(),
            json!({"type": "string", "description": "LLM provider 名称", "required": false}),
        );
        schema.insert(
            "model".to_string(),
            json!({"type": "string", "description": "模型名称", "required": false}),
        );
        schema.insert(
            "prompt_version".to_string(),
            json!({"type": "string", "description": "提示词版本", "required": false}),
        );
        schema
    }

    /// 执行该指标的评测。
    ///
    /// 按照本模块说明中描述的流程，依次渲染提示词、调用 LLM 完成多步判断，
    /// 最后对结果做数学聚合得到评分。
    ///
    /// # Arguments
    ///
    /// * `params` - 指标参数，支持可选字段 `provider` / `model` / `prompt_version`。
    /// * `input` - 评测输入数据（`serde_json::Value`），具体字段由各指标定义，详见模块级文档。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`MetricOutput`]，其中 `score` 为本指标评分（范围依指标而定）。
    ///
    /// # Errors
    ///
    /// 当必需输入字段缺失、提示词未找到、LLM 调用失败或响应无法解析为预期结构时，
    /// 返回 [`EvalError`]。
    async fn evaluate(
        &self,
        params: &HashMap<String, Value>,
        input: &Value,
    ) -> Result<MetricOutput, EvalError> {
        let provider = params.get("provider").and_then(|v| v.as_str());
        let model = params.get("model").and_then(|v| v.as_str());
        let version = params.get("prompt_version").and_then(|v| v.as_str());

        let question = input
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 question 字段".into()))?;
        let reference = input
            .get("reference")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 reference 字段".into()))?;
        let contexts: Vec<String> = input
            .get("contexts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| EvalError::InvalidParams("缺少 contexts 数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let answer = input
            .get("answer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 answer 字段".into()))?;

        // Step 1: 从 response 中分解出声明
        let mut vars = tera::Context::new();
        vars.insert("response", answer);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "noise_sensitivity_extract_claims",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let claims: Vec<String> = parsed["claims"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("claims 不是数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if claims.is_empty() {
            return Ok(MetricOutput {
                score: 0.0,
                details: json!({
                    "question": question,
                    "reference": reference,
                    "contexts": contexts,
                    "answer": answer,
                    "claims": [],
                    "incorrect": 0,
                    "total": 0,
                }),
            });
        }

        let total = claims.len();
        let mut incorrect = 0usize;
        let mut verdicts = Vec::new();

        // Step 2: 逐条验证每个声明是否基于 ground_truth 正确
        for claim in &claims {
            let mut vars = tera::Context::new();
            vars.insert("ground_truth", reference);
            vars.insert("claim", claim);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "noise_sensitivity_verify_claim",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let correct = parse_verdict(&parsed, &["correct"])?;
            verdicts.push(json!({
                "claim": claim,
                "correct": correct,
            }));
            if !correct {
                incorrect += 1;
            }
        }

        let score = incorrect as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "question": question,
                "reference": reference,
                "contexts": contexts,
                "answer": answer,
                "claims": claims,
                "verdicts": verdicts,
                "incorrect": incorrect,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_no_noise() {
        // 0 incorrect → score 0 = good (low noise)
        assert!((0.0_f64 / 3.0 - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_some_noise() {
        let score = 2.0_f64 / 4.0;
        assert!((score - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_all_noise() {
        assert!((3.0_f64 / 3.0 - 1.0).abs() < 0.001);
    }
}
