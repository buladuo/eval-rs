//! Perplexity 指标 — 基于 LLM 计算文本困惑度
//!
//! Perplexity 是评估语言模型对文本建模能力的指标，越低越好。
//! 此实现通过 LLM provider 获取对数概率，再计算困惑度。
//!
//! # 原理
//!
//! 构造提示词请求 LLM 评估文本自然度与流畅度，
//! 将返回的评分（1-100）转换为 perplexity 表示。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::registry::{Metric, MetricOutput};
use crate::error::EvalError;
use crate::provider::manager::ProviderManager;

/// Perplexity 指标
pub struct PerplexityMetric {
    /// Provider 管理器引用
    providers: std::sync::Arc<ProviderManager>,
}

impl PerplexityMetric {
    /// 创建新的 Perplexity 指标
    ///
    /// # Arguments
    ///
    /// * `providers` - Provider 管理器（Arc 共享引用）
    pub fn new(providers: std::sync::Arc<ProviderManager>) -> Self {
        Self { providers }
    }
}

#[async_trait]
impl Metric for PerplexityMetric {
    fn name(&self) -> &str {
        "perplexity"
    }

    fn metric_type(&self) -> &str {
        "llm"
    }

    /// 参数 schema
    ///
    /// - `provider`: LLM provider 名称（可选）
    /// - `model`: 模型名称（可选）
    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "provider".to_string(),
            json!({"type": "string", "description": "使用的 LLM provider 名称", "required": false}),
        );
        schema.insert(
            "model".to_string(),
            json!({"type": "string", "description": "使用的模型名称", "required": false}),
        );
        schema
    }

    /// 执行 perplexity 评测
    async fn evaluate(
        &self,
        params: &HashMap<String, Value>,
        input: &Value,
    ) -> Result<MetricOutput, EvalError> {
        let text = input
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 text 字段".to_string()))?;

        let provider = params.get("provider").and_then(|v| v.as_str());
        let model = params.get("model").and_then(|v| v.as_str());

        // 构造提示词，请求 LLM 评估文本困惑度
        let prompt = format!(
            "请评估以下文本的自然度与流畅度，返回一个 1-100 的整数评分（越低表示越困惑、越不自然）：\n\n{text}\n\n请仅返回数字。"
        );

        let response = self
            .providers
            .complete(provider, &prompt, model, None)
            .await?;

        // 解析 LLM 返回的评分
        let score = response
            .trim()
            .parse::<f64>()
            .map_err(|_| EvalError::JudgeParseFailed(response.clone()))?;

        // 将评分转换为 perplexity 表示（评分越高，perplexity 越低）
        let perplexity = if score > 0.0 { 100.0 / score } else { f64::MAX };

        Ok(MetricOutput {
            score: perplexity,
            details: json!({
                "raw_score": score,
                "perplexity": perplexity,
                "text_length": text.len(),
            }),
        })
    }
}
