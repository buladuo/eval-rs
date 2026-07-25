//! Context Recall（上下文召回率）指标
//!
//! 评估检索到的上下文是否包含回答问题所需的全部信息。
//! 以 reference（参考答案）为代理真值，将 reference 拆分为声明后逐条验证。
//!
//! # 流程（2 步 LLM + 数学聚合）
//!
//! 1. 将 reference 分解为独立声明（claims）
//! 2. 逐条验证每个声明是否能被检索到的上下文（contexts）支持
//! 3. `score = 支持的声明数 / 声明总数`
//!
//! # 输入字段
//!
//! - `question` — 用户问题
//! - `reference` — 参考答案（ground truth）
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

/// Context Recall 指标
pub struct ContextRecall {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl ContextRecall {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for ContextRecall {
    fn name(&self) -> &str {
        "context_recall"
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

        let contexts_text = contexts.join("\n---\n");

        // Step 1: 从 reference 中分解出声明
        let mut vars = tera::Context::new();
        vars.insert("reference", reference);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "context_recall_extract_claims",
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
                score: 1.0,
                details: json!({
                    "question": question,
                    "reference": reference,
                    "contexts": contexts,
                    "claims": [],
                    "supported": 0,
                    "total": 0,
                }),
            });
        }

        let total = claims.len();
        let mut supported = 0usize;
        let mut verdicts = Vec::new();

        // Step 2: 逐条验证每个声明是否能被 contexts 支持
        for claim in &claims {
            let mut vars = tera::Context::new();
            vars.insert("contexts", &contexts_text);
            vars.insert("claim", claim);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "context_recall_verify_claim",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let ok = parse_verdict(&parsed, &["verdict", "supported"])?;
            verdicts.push(json!({
                "claim": claim,
                "supported": ok,
            }));
            if ok {
                supported += 1;
            }
        }

        let score = supported as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "question": question,
                "reference": reference,
                "contexts": contexts,
                "claims": claims,
                "verdicts": verdicts,
                "supported": supported,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_full_recall() {
        assert!((5.0_f64 / 5.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_partial_recall() {
        let score = 3.0_f64 / 5.0;
        assert!((score - 0.6).abs() < 0.001);
    }
}
