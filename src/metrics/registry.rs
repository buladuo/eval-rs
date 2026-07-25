//! 指标注册表 — Metric trait 定义与注册管理
//!
//! 定义 `Metric` trait，所有指标需实现该 trait 并通过 `MetricRegistry` 注册。
//!
//! # Examples
//!
//! ```
//! use eval_rs::metrics::registry::{Metric, MetricRegistry, MetricOutput};
//! use std::collections::HashMap;
//! use serde_json::Value;
//! use async_trait::async_trait;
//!
//! struct MyMetric;
//!
//! #[async_trait]
//! impl Metric for MyMetric {
//!     fn name(&self) -> &str { "my_metric" }
//!     fn metric_type(&self) -> &str { "non_llm" }
//!     fn params_schema(&self) -> HashMap<String, Value> { HashMap::new() }
//!     async fn evaluate(&self, _p: &HashMap<String, Value>, _i: &Value) -> Result<MetricOutput, eval_rs::error::EvalError> {
//!         Ok(MetricOutput { score: 1.0, details: Value::Null })
//!     }
//! }
//!
//! let mut reg = MetricRegistry::new();
//! reg.register(Box::new(MyMetric));
//! assert!(reg.get("my_metric").is_some());
//! ```

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::EvalError;

/// 指标 trait
///
/// 所有指标（LLM 与非 LLM）均需实现此 trait。
#[async_trait]
pub trait Metric: Send + Sync {
    /// 指标名称（全局唯一）
    fn name(&self) -> &str;

    /// 指标类型：`"llm"` 或 `"non_llm"`
    fn metric_type(&self) -> &str;

    /// 参数 schema，描述该指标接受的参数
    fn params_schema(&self) -> HashMap<String, Value>;

    /// 执行评测
    ///
    /// # Arguments
    ///
    /// * `params` - 参数键值
    /// * `input` - 预处理后的输入
    ///
    /// # Errors
    ///
    /// 参数校验失败、计算异常等。
    async fn evaluate(&self, params: &HashMap<String, Value>, input: &Value) -> Result<MetricOutput, EvalError>;
}

/// 指标输出
#[derive(Debug, Clone)]
pub struct MetricOutput {
    /// 评分（0.0 ~ 1.0）
    pub score: f64,
    /// 详细信息（JSON，由指标自定义填充）
    pub details: Value,
}

/// 指标注册表
///
/// 按名称存储并查找 [`Metric`] 实现。
pub struct MetricRegistry {
    metrics: HashMap<String, Box<dyn Metric>>,
}

impl Default for MetricRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricRegistry {
    /// 创建新的空注册表
    pub fn new() -> Self {
        Self {
            metrics: HashMap::new(),
        }
    }

    /// 注册指标
    ///
    /// 同名指标将被覆盖（覆盖时打印 warning 日志）。
    ///
    /// # Arguments
    ///
    /// * `metric` - 任意 [`Metric`] 实现
    pub fn register(&mut self, metric: Box<dyn Metric>) {
        let name = metric.name().to_string();
        tracing::info!(metric_name = %name, metric_type = %metric.metric_type(), "注册指标");
        self.metrics.insert(name, metric);
    }

    /// 按名称获取指标
    ///
    /// # Returns
    ///
    /// 返回 `&dyn Metric`；未找到返回 `None`。
    pub fn get(&self, name: &str) -> Option<&dyn Metric> {
        self.metrics.get(name).map(|m| m.as_ref())
    }

    /// 列出所有已注册指标
    pub fn list(&self) -> Vec<&dyn Metric> {
        self.metrics.values().map(|m| m.as_ref()).collect()
    }
}