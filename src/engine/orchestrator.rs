//! 评测编排器 — 编排评测的完整流程
//!
//! 流程：参数校验 → 输入预处理 → 指标执行 → 结果封装

use std::collections::HashMap;

use serde_json::Value;
use tokio::time::timeout;
use tracing::Instrument;

use crate::engine::{EvalEngine, EvalRequest, EvalResult};
use crate::error::EvalError;
use crate::metrics::registry::MetricOutput;

/// 执行单次评测的完整流程
///
/// 1. 查找指标
/// 2. 输入预处理（可选）
/// 3. 执行评测（带超时）
/// 4. 封装结果
pub async fn run_evaluation(
    engine: &EvalEngine,
    request: EvalRequest,
) -> Result<EvalResult, EvalError> {
    let request_id = request.request_id.clone();
    let metric_name = request.metric.clone();

    let span = tracing::info_span!(
        "run_evaluation",
        metric = %metric_name,
        request_id = %request_id,
    );

    async move {
        tracing::info!(metric = %metric_name, "开始评测");

        // 1. 查找指标
        let metric = engine
            .metrics
            .get(&metric_name)
            .ok_or_else(|| EvalError::MetricNotFound(metric_name.clone()))?;

        tracing::debug!(metric = %metric_name, metric_type = %metric.metric_type(), "找到指标");

        // 2. 输入预处理
        let processed_input = match &request.extract {
            Some(config) => {
                tracing::debug!("执行输入预处理");
                crate::preprocessor::preprocess(&request.input, config)?
            }
            None => request.input.clone(),
        };

        // 3. 执行评测（带超时）
        let eval_timeout = std::time::Duration::from_secs(engine.eval_timeout_secs);
        let eval_future = metric.evaluate(&request.params, &processed_input);

        let metric_output = match timeout(eval_timeout, eval_future).await {
            Ok(result) => result?,
            Err(_) => {
                tracing::error!(metric = %metric_name, "评测超时");
                return Err(EvalError::EvalTimeout);
            }
        };

        // 4. 封装结果
        let result = to_eval_result(&metric_output, &metric_name, &request_id);

        tracing::info!(metric = %metric_name, score = metric_output.score, "评测完成");

        Ok(result)
    }
    .instrument(span)
    .await
}

/// 将指标输出转换为评测结果
fn to_eval_result(output: &MetricOutput, metric: &str, request_id: &str) -> EvalResult {
    EvalResult {
        metric: metric.to_string(),
        score: output.score,
        details: output.details.clone(),
        request_id: request_id.to_string(),
    }
}

/// 校验指标参数
///
/// 简单的参数存在性校验，详细的 schema 校验由各指标自行实现。
pub fn validate_params(
    params: &HashMap<String, Value>,
    schema: &HashMap<String, Value>,
) -> Result<(), EvalError> {
    for (name, schema_value) in schema {
        let required = schema_value
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if required && !params.contains_key(name) {
            return Err(EvalError::InvalidParams(format!("缺少必填参数: {name}")));
        }
    }
    Ok(())
}