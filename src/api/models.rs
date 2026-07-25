//! API 请求 / 响应模型定义
//!
//! 定义 HTTP 接口的请求与响应数据结构。所有结构体均实现 [`serde::Serialize`]
//! 或 [`serde::Deserialize`]，使用 `serde_json::Value` 兼容灵活字段。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{EvalRequest, EvalResult, MetricInfo};
use crate::preprocessor::PreprocessConfig;

/// HTTP 评测请求体
///
/// 对应 `POST /v1/eval` 的 JSON Body。
///
/// # Examples
///
/// ```json
/// {
///   "metric": "rouge",
///   "params": { "variant": "n", "n": 1 },
///   "input": { "reference": "...", "hypothesis": "..." },
///   "extract": null,
///   "retry": { "max_attempts": 3, "backoff_ms": 1000 },
///   "provider": null,
///   "request_id": null
/// }
/// ```
#[derive(Debug, Deserialize)]
pub struct EvalRequestBody {
    /// 指标名称（如 `"rouge"`、`"llm_judge_accuracy"`）
    pub metric: String,
    /// 指标参数。键值集合，具体 schema 由指标自身定义。
    #[serde(default)]
    pub params: HashMap<String, Value>,
    /// 评测输入。结构由指标定义，常见字段如 `reference` / `hypothesis` / `text`。
    pub input: Value,
    /// 输入预处理配置（可选）。详见 [`PreprocessConfig`]。
    #[serde(default)]
    pub extract: Option<PreprocessConfig>,
    /// 重试配置（可选），覆盖全局默认。
    #[serde(default)]
    pub retry: Option<RetryConfigBody>,
    /// 指定使用的 LLM provider 名称；为 `None` 时使用配置中的默认 provider。
    #[serde(default)]
    pub provider: Option<String>,
    /// 请求 ID（可选）。未提供时在 [`Self::to_engine_request`] 内自动生成 UUIDv4。
    #[serde(default)]
    pub request_id: Option<String>,
}

/// HTTP 重试配置（请求体内嵌）
#[derive(Debug, Deserialize)]
pub struct RetryConfigBody {
    /// 最大重试次数（含首次调用）
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// 初始退避时间（毫秒）。每次重试按 2^n 指数退避。
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,
}

fn default_max_attempts() -> u32 {
    3
}

fn default_backoff_ms() -> u64 {
    1000
}

impl EvalRequestBody {
    /// 将 HTTP 请求体转换为引擎层 [`EvalRequest`]
    ///
    /// 主要完成：
    ///
    /// - 字段透传；
    /// - 将 [`RetryConfigBody`] 映射为 [`crate::engine::RetryConfig`]；
    /// - 当未提供 `request_id` 时生成 UUIDv4。
    ///
    /// # Returns
    ///
    /// 引擎层可直接消费的 [`EvalRequest`]。
    pub fn to_engine_request(self) -> EvalRequest {
        EvalRequest {
            metric: self.metric,
            params: self.params,
            input: self.input,
            extract: self.extract,
            retry: self.retry.map(|r| crate::engine::RetryConfig {
                max_attempts: r.max_attempts,
                backoff_ms: r.backoff_ms,
            }),
            provider: self.provider,
            request_id: self
                .request_id
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        }
    }
}

/// HTTP 评测响应体
#[derive(Debug, Serialize)]
pub struct EvalResponse {
    /// 指标名称
    pub metric: String,
    /// 评分
    pub score: f64,
    /// 详细信息（JSON），由各指标自定义填充。
    pub details: Value,
    /// 请求 ID。透传请求中的 `request_id`，或由服务端生成。
    pub request_id: String,
}

impl From<EvalResult> for EvalResponse {
    /// 从引擎层 [`EvalResult`] 直接转换。
    fn from(result: EvalResult) -> Self {
        Self {
            metric: result.metric,
            score: result.score,
            details: result.details,
            request_id: result.request_id,
        }
    }
}

/// 指标列表响应（`GET /v1/metrics`）
#[derive(Debug, Serialize)]
pub struct MetricsListResponse {
    /// 已注册指标的元信息列表
    pub metrics: Vec<MetricInfo>,
}

/// 健康检查响应（`GET /health`）
///
/// 当所有依赖探测均通过时 `status = "ok"`，否则 `status = "degraded"`，
/// 并在 `dependencies` 中列出各项依赖的状态与错误信息。
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// 总体状态：`"ok"` 或 `"degraded"`
    pub status: String,
    /// 各依赖项的探测结果
    #[serde(default)]
    pub dependencies: HashMap<String, DependencyHealth>,
}

/// 单个依赖的健康状态
#[derive(Debug, Serialize)]
pub struct DependencyHealth {
    /// 依赖状态：`"ok"` / `"error"` / `"disabled"`
    pub status: String,
    /// 错误信息（仅在 `status = "error"` 时存在）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Default for HealthResponse {
    /// 返回 `{ "status": "ok", "dependencies": {} }`
    fn default() -> Self {
        Self {
            status: "ok".to_string(),
            dependencies: HashMap::new(),
        }
    }
}

/// 错误响应体
///
/// 当任意处理器返回 [`crate::error::EvalError`] 时，
/// 由 [`IntoResponse`](axum::response::IntoResponse) 转换为此结构。
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// 错误代码（与 HTTP 状态码语义对应）
    pub error: String,
    /// 人类可读的错误详情
    pub details: String,
}
