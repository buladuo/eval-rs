//! Context Entities Recall（上下文实体召回率）指标
//!
//! 评估检索到的上下文是否覆盖了参考答案中的关键实体（人名、地名、组织名、术语等）。
//!
//! # 流程（2 次 LLM 调用 + 集合运算）
//!
//! 1. 从 reference 中抽取实体集合 RE
//! 2. 从 retrieved_contexts 中抽取实体集合 RCE
//! 3. `score = |RCE ∩ RE| / |RE|`（纯数学计算，不涉及 LLM）
//!
//! # 输入字段
//!
//! - `reference` — 参考答案
//! - `contexts` — 检索到的上下文列表（字符串数组）

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Context Entities Recall 指标
pub struct ContextEntitiesRecall {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl ContextEntitiesRecall {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for ContextEntitiesRecall {
    fn name(&self) -> &str {
        "context_entities_recall"
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
        let contexts: Vec<String> = input
            .get("contexts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| EvalError::InvalidParams("缺少 contexts 数组".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        let contexts_text = contexts.join("\n---\n");

        // Step 1: 从 reference 抽取实体集合 RE
        let mut vars = tera::Context::new();
        vars.insert("text", reference);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "entity_extraction",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let re: Vec<String> = parsed["entities"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("entities 不是数组 (reference)".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
            .collect();

        // Step 2: 从 contexts 抽取实体集合 RCE
        let mut vars = tera::Context::new();
        vars.insert("text", &contexts_text);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "entity_extraction",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let rce: Vec<String> = parsed["entities"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("entities 不是数组 (contexts)".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
            .collect();

        if re.is_empty() {
            return Ok(MetricOutput {
                score: 1.0,
                details: json!({
                    "reference": reference,
                    "contexts": contexts,
                    "entities_reference": [],
                    "entities_contexts": rce,
                    "found": [],
                    "missing": [],
                    "score": 1.0,
                }),
            });
        }

        // Step 3: 集合交集运算（纯数学，不涉及 LLM）
        let total = re.len();
        let mut found = Vec::new();
        let mut missing = Vec::new();

        for entity in &re {
            if rce.contains(entity) {
                found.push(entity.clone());
            } else {
                missing.push(entity.clone());
            }
        }

        let score = found.len() as f64 / total as f64;

        Ok(MetricOutput {
            score,
            details: json!({
                "reference": reference,
                "contexts": contexts,
                "entities_reference": re,
                "entities_contexts": rce,
                "found": found,
                "missing": missing,
                "total": total,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_all_found() {
        let re = ["a", "b", "c"];
        let rce = ["a", "b", "c", "d"];
        let intersection = re.iter().filter(|e| rce.contains(e)).count();
        assert!((intersection as f64 / re.len() as f64 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_partial_found() {
        let re = ["a", "b", "c", "d"];
        let rce = ["a", "c"];
        let intersection = re.iter().filter(|e| rce.contains(e)).count();
        assert!((intersection as f64 / re.len() as f64 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_none_found() {
        let re = ["a", "b"];
        let rce: Vec<&str> = vec![];
        let intersection = re.iter().filter(|e| rce.contains(e)).count();
        assert!((intersection as f64 / re.len() as f64 - 0.0).abs() < 0.001);
    }
}
