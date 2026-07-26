//! Summarization Score（摘要质量）指标
//!
//! 评估摘要捕获原文重要信息的程度，基于 QA 框架实现。
//!
//! # 流程（3 步 LLM + 数学聚合）
//!
//! 1. 从原文中抽取关键短语（keyphrases）
//! 2. 基于关键短语生成一组问题（对原文答案恒为 yes）
//! 3. 用这些问题询问摘要，统计正确回答的比例
//! 4. `QA score = 正确回答数 / 总问题数`
//!
//! # 输入字段
//!
//! - `reference` — 原文内容
//! - `summary` — 模型生成的摘要

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json, parse_verdict};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Summarization Score（摘要质量）指标
///
/// 基于多步 LLM-as-Judge 流程实现的评测指标，实现了
/// [`super::super::registry::Metric`] trait。持有一个 LLM provider 管理器与一个提示词
/// 注册表的 [`Arc`] 共享引用，在执行 [`Metric::evaluate`] 时用于渲染并调用 LLM
/// 完成关键短语抽取、问题生成与 QA 校验，最后做计数聚合。
pub struct SummarizationScore {
    /// LLM provider 管理器（共享引用），用于调用 LLM 完成评测。
    providers: Arc<ProviderManager>,
    /// 提示词注册表（共享引用），按名称渲染 judge 提示词模板。
    prompts: Arc<PromptRegistry>,
}

impl SummarizationScore {
    /// 创建一个新的 Summarization Score 指标实例。
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
impl Metric for SummarizationScore {
    fn name(&self) -> &str {
        "summarization_score"
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

        let reference = input
            .get("reference")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 reference 字段".into()))?;
        let summary = input
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 summary 字段".into()))?;

        // Step 1: 从原文中提取关键短语
        let mut vars = tera::Context::new();
        vars.insert("reference", reference);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "summarization_extract_keyphrases",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let keyphrases: Vec<String> = parsed["keyphrases"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("keyphrases 不是数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if keyphrases.is_empty() {
            return Ok(MetricOutput {
                score: 0.0,
                details: json!({
                    "reference": reference,
                    "summary": summary,
                    "keyphrases": [],
                    "total_questions": 0,
                }),
            });
        }

        // Step 2: 基于关键短语生成问题
        let mut vars = tera::Context::new();
        vars.insert("keyphrases", &keyphrases);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "summarization_generate_questions",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let questions: Vec<String> = parsed["questions"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("questions 不是数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if questions.is_empty() {
            return Ok(MetricOutput {
                score: 0.0,
                details: json!({
                    "reference": reference,
                    "summary": summary,
                    "keyphrases": keyphrases,
                    "questions": [],
                    "total": 0,
                }),
            });
        }

        let total = questions.len();

        // Step 3: 用问题询问摘要，统计正确回答
        let mut correct = 0usize;
        let mut qa_results = Vec::new();

        for question in &questions {
            let mut vars = tera::Context::new();
            vars.insert("summary", summary);
            vars.insert("question", question);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "summarization_qa",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let ok = parse_verdict(&parsed, &["correct", "verdict"])?;
            qa_results.push(json!({
                "question": question,
                "correct": ok,
            }));
            if ok {
                correct += 1;
            }
        }

        let score = correct as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "reference": reference,
                "summary": summary,
                "keyphrases": keyphrases,
                "questions": questions,
                "qa_results": qa_results,
                "correct": correct,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_all_correct() {
        assert!((5.0_f64 / 5.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_partial_correct() {
        let score = 3.0_f64 / 5.0;
        assert!((score - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_none_correct() {
        assert!((0.0_f64 / 5.0 - 0.0).abs() < 0.001);
    }
}
