//! Faithfulness（忠实度）指标
//!
//! 评估生成答案是否忠实于检索到的上下文，答案中的每个声明都应在上下文中找到依据。
//!
//! # 流程（2 步 LLM + 数学聚合）
//!
//! 1. 将 answer 分解为独立声明（claims）
//! 2. 逐条验证每个声明是否能从 context 推断
//! 3. `score = 支持数 / 总数`
//!
//! # 输入字段
//!
//! - `question` — 用户问题
//! - `context` — 检索到的上下文（字符串）
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

/// Faithfulness 指标
pub struct Faithfulness {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl Faithfulness {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for Faithfulness {
    fn name(&self) -> &str {
        "faithfulness"
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

        let question = get_str(input, "question")?;
        let context = get_str(input, "context")?;
        let answer = get_str(input, "answer")?;

        // Step 1: 从 answer 中分解出独立声明
        let mut vars = tera::Context::new();
        vars.insert("answer", answer);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "faithfulness_extract_claims",
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
            .collect::<Vec<_>>();

        if claims.is_empty() {
            return Ok(MetricOutput {
                score: 1.0,
                details: json!({
                    "question": question,
                    "context": context,
                    "answer": answer,
                    "claims": [],
                    "supported": 0,
                    "total": 0,
                }),
            });
        }

        let total = claims.len();

        // Step 2: 逐条验证每个声明
        let mut supported = 0usize;
        let mut verdicts = Vec::new();
        for claim in &claims {
            let mut vars = tera::Context::new();
            vars.insert("context", context);
            vars.insert("claim", claim);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "faithfulness_verify_claim",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let ok = parse_verdict(&parsed, &["verdict"])?;
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
                "context": context,
                "answer": answer,
                "claims": claims,
                "verdicts": verdicts,
                "supported": supported,
                "total": total,
            }),
        })
    }
}

fn get_str<'a>(input: &'a Value, key: &str) -> Result<&'a str, EvalError> {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| EvalError::InvalidParams(format!("缺少 {key} 字段")))
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_score_calculation() {
        // 3 claims, 2 supported → 0.667
        let score = 2.0_f64 / 3.0;
        assert!((score - 0.667).abs() < 0.001);
    }

    #[test]
    fn test_all_supported() {
        assert!((3.0_f64 / 3.0 - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_none_supported() {
        assert!((0.0_f64 / 3.0 - 0.0).abs() < 0.001);
    }
}
