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

/// Faithfulness（忠实度）指标
///
/// 基于多步 LLM-as-Judge 流程实现的评测指标，实现了
/// [`super::super::registry::Metric`] trait。持有一个 LLM provider 管理器与一个提示词
/// 注册表的 [`Arc`] 共享引用，在执行 [`Metric::evaluate`] 时用于渲染并调用 LLM
/// 完成逐声明验证，最后做计数聚合。
pub struct Faithfulness {
    /// LLM provider 管理器（共享引用），用于调用 LLM 完成评测。
    providers: Arc<ProviderManager>,
    /// 提示词注册表（共享引用），按名称渲染 judge 提示词模板。
    prompts: Arc<PromptRegistry>,
}

impl Faithfulness {
    /// 创建一个新的 Faithfulness 指标实例。
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
