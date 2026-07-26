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

/// Context Relevancy（上下文相关性）指标
///
/// 基于多步 LLM-as-Judge 流程实现的评测指标，实现了
/// [`super::super::registry::Metric`] trait。持有一个 LLM provider 管理器与一个提示词
/// 注册表的 [`Arc`] 共享引用，在执行 [`Metric::evaluate`] 时用于渲染并调用 LLM
/// 完成逐 chunk 相关性判断，最后做简单平均聚合。
pub struct ContextRelevancy {
    /// LLM provider 管理器（共享引用），用于调用 LLM 完成评测。
    providers: Arc<ProviderManager>,
    /// 提示词注册表（共享引用），按名称渲染 judge 提示词模板。
    prompts: Arc<PromptRegistry>,
}

impl ContextRelevancy {
    /// 创建一个新的 Context Relevancy 指标实例。
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
