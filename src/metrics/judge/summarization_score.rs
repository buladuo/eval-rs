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

/// Summarization Score 指标
pub struct SummarizationScore {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl SummarizationScore {
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
