//! Answer Relevancy（答案相关性）指标
//!
//! 评估模型回答与用户问题意图的匹配程度（不评估事实正确性）。
//!
//! # 流程（2 步 LLM + 数学聚合）
//!
//! 1. 从 answer 反向生成 N 个可能的问题
//! 2. 逐对判断每个生成问题与原始问题是否语义等价
//! 3. `score = 匹配数 / N`
//!
//! 注：官方 RAGAS 使用 embedding 余弦相似度计算匹配度，
//! 此处使用 LLM 语义等同判断作为替代（无需 embedding 模型）。
//!
//! # 输入字段
//!
//! - `question` — 用户原始问题
//! - `answer` — 模型回答
//! - `n`（可选,默认 3） — 生成问题数量

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json, parse_verdict};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Answer Relevancy 指标
pub struct AnswerRelevancy {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl AnswerRelevancy {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for AnswerRelevancy {
    fn name(&self) -> &str {
        "answer_relevancy"
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
        schema.insert(
            "n".to_string(),
            json!({"type": "number", "description": "生成的问题数量，默认 3", "required": false}),
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
        let n: u32 = params.get("n").and_then(|v| v.as_u64()).unwrap_or(3) as u32;

        let question = input
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 question 字段".into()))?;
        let answer = input
            .get("answer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 answer 字段".into()))?;

        // Step 1: 从 answer 反向生成 N 个可能的问题
        let mut vars = tera::Context::new();
        vars.insert("answer", answer);
        vars.insert("n", &n);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "answer_relevancy_generate_questions",
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
                    "question": question,
                    "answer": answer,
                    "n": n,
                    "generated_questions": [],
                    "matched": 0,
                    "total": 0,
                }),
            });
        }

        let total = questions.len();

        // Step 2: 逐对判断原始问题与生成问题是否语义等价
        let mut matched = 0usize;
        let mut results = Vec::new();
        for gen_q in &questions {
            let mut vars = tera::Context::new();
            vars.insert("original", question);
            vars.insert("generated", gen_q);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "answer_relevancy_match",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let ok = parse_verdict(&parsed, &["match", "verdict"])?;
            results.push(json!({
                "generated_question": gen_q,
                "matched": ok,
            }));
            if ok {
                matched += 1;
            }
        }

        let score = matched as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "question": question,
                "answer": answer,
                "n": n,
                "generated_questions": questions,
                "pair_results": results,
                "matched": matched,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_all_matched() {
        assert!((3.0_f64 / 3.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_partial_matched() {
        let score = 2.0_f64 / 3.0;
        assert!((score - 0.667).abs() < 0.001);
    }
}
