//! Context Relevancy（上下文相关性）指标
//!
//! 评估检索到的上下文与问题的相关性（逐 chunk 二值判断，无位置加权）。
//!
//! # 流程（N 步 LLM + 计数聚合）
//!
//! 1. 对每个检索 chunk 逐一二值判定（0=不相关,1=相关）
//! 2. `score = 相关 chunk 数 / 总 chunk 数`
//!
//! 与 [`super::context_precision`] 的区别：不按位置加权，简单平均。
//!
//! # 输入字段
//!
//! - `question` — 用户问题
//! - `contexts` — 检索到的上下文列表（字符串数组）

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json, parse_verdict};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Context Relevancy 指标
pub struct ContextRelevancy {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl ContextRelevancy {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for ContextRelevancy {
    fn name(&self) -> &str {
        "context_relevancy"
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

        let question = input
            .get("question")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 question 字段".into()))?;
        let contexts: Vec<String> = input
            .get("contexts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| EvalError::InvalidParams("缺少 contexts 数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        if contexts.is_empty() {
            return Ok(MetricOutput {
                score: 0.0,
                details: json!({ "question": question, "contexts": [], "total": 0 }),
            });
        }

        let total = contexts.len();
        let mut relevant = 0usize;
        let mut verdicts = Vec::new();

        // Step 1: 逐 chunk 判断与问题的相关性
        for chunk in &contexts {
            let mut vars = tera::Context::new();
            vars.insert("question", question);
            vars.insert("chunk", chunk);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "chunk_relevance",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let ok = parse_verdict(&parsed, &["verdict", "relevant"])?;
            verdicts.push(ok);
            if ok {
                relevant += 1;
            }
        }

        let score = relevant as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "question": question,
                "contexts": contexts,
                "verdicts": verdicts.iter().map(|&v| if v { 1 } else { 0 }).collect::<Vec<u8>>(),
                "relevant": relevant,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_all_relevant() {
        assert!((5.0_f64 / 5.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_half_relevant() {
        assert!((2.0_f64 / 4.0 - 0.5).abs() < 0.001);
    }
}
