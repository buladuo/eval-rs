//! 评测路由器 — 按指标名路由到 LLM 或非 LLM 执行路径
//!
//! 通过指标注册表查找指标，根据指标类型决定执行路径。

use crate::engine::EvalEngine;
use crate::error::EvalError;
use crate::metrics::registry::Metric;

/// 路由评测请求到对应的指标执行器
///
/// # Arguments
///
/// * `engine` - 评测引擎
/// * `metric_name` - 指标名称
///
/// # Errors
///
/// 指标未注册时返回 [`EvalError::MetricNotFound`]。
pub fn route_metric<'a>(
    engine: &'a EvalEngine,
    metric_name: &str,
) -> Result<&'a dyn Metric, EvalError> {
    engine
        .metrics
        .get(metric_name)
        .ok_or_else(|| EvalError::MetricNotFound(metric_name.to_string()))
}

/// 判断指标是否为 LLM 指标
pub fn is_llm_metric(metric: &dyn Metric) -> bool {
    metric.metric_type() == "llm"
}
