//! 错误类型定义 — 统一错误处理
//!
//! 使用 `thiserror` 定义项目中所有模块可能产生的错误。
//!
//! 同时实现 [`axum::response::IntoResponse`]，使 [`EvalError`] 可直接作为
//! axum 处理器返回值，自动映射为 JSON 错误响应。
//!
//! # 错误分类
//!
//! | 分组               | 变体                          | HTTP 状态码             |
//! |--------------------|-------------------------------|-------------------------|
//! | 配置错误           | `ConfigError`                 | 500                     |
//! | Provider 错误      | `ProviderNotConfigured`、`LlmCallError`、`LlmRetryExhausted`、`RateLimitTimeout` | 400/502/429 |
//! | 指标错误           | `MetricNotFound`、`InvalidParams`、`MetricExecutionError` | 404/400/500 |
//! | 提示词错误         | `PromptNotFound`、`InvalidPromptVariables`、`PromptRenderError` | 404/400/500 |
//! | 预处理错误         | `JsonPathNotFound`、`RegexNoMatch` | 400 |
//! | 评测错误           | `EvalTimeout`、`JudgeParseFailed` | 504/422 |
//! | 内部错误           | `Internal`                    | 500                     |

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// 统一的评测错误类型
///
/// 涵盖配置、Provider 调用、指标执行、提示词、预处理与评测超时等环节。
/// 通过 `thiserror::Error` 派生 `Display` 与 `std::error::Error`。
#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    // ── 配置错误 ──
    /// 配置加载失败（文件不存在、TOML 解析错误等）
    #[error("配置加载失败: {0}")]
    ConfigError(String),

    // ── Provider 错误 ──
    /// 请求中指定的 provider 未在配置中注册
    #[error("Provider 未配置: {0}")]
    ProviderNotConfigured(String),

    /// LLM 调用失败（网络错误、认证失败、模型不可用等）
    #[error("LLM 调用失败: {0}")]
    LlmCallError(String),

    /// 所有重试均已耗尽，仍未成功
    #[error("LLM 调用重试耗尽: {0}")]
    LlmRetryExhausted(String),

    /// 限流等待超时（令牌桶长时间无可用令牌）
    #[error("限流等待超时")]
    RateLimitTimeout,

    /// 限流拒绝（令牌桶耗尽且超过等待超时）
    #[error("限流拒绝，建议 {retry_after:?} 后重试")]
    RateLimited {
        /// 建议重试等待时间
        retry_after: Option<std::time::Duration>,
    },

    /// 单次 LLM 调用超时（外部超时触发）
    #[error("LLM 调用超时（{0:?}）")]
    Timeout(std::time::Duration),

    // ── 指标错误 ──
    /// 请求中指定的指标名称未在注册表中找到
    #[error("指标未找到: {0}")]
    MetricNotFound(String),

    /// 指标参数不符合其 schema 约束
    #[error("指标参数无效: {0}")]
    InvalidParams(String),

    /// 指标内部执行失败（计算过程异常）
    #[error("指标执行失败: {0}")]
    MetricExecutionError(String),

    // ── 提示词错误 ──
    /// 指定的提示词模板名称未找到
    #[error("提示词未找到: {0}")]
    PromptNotFound(String),

    /// 渲染提示词时缺少必填变量
    #[error("提示词变量校验失败: {0}")]
    InvalidPromptVariables(String),

    /// tera 模板渲染失败
    #[error("提示词渲染失败: {0}")]
    PromptRenderError(String),

    // ── 预处理错误 ──
    /// JSON 路径在输入中不存在
    #[error("JSON 路径未找到: {0}")]
    JsonPathNotFound(String),

    /// 正则表达式在输入中无匹配
    #[error("正则无匹配")]
    RegexNoMatch,

    // ── 评测错误 ──
    /// 评测执行超时（超过配置的 timeout_secs）
    #[error("评测超时")]
    EvalTimeout,

    /// Judge 返回的评分无法解析为数字
    #[error("Judge 评分解析失败: {0}")]
    JudgeParseFailed(String),

    // ── 内部错误 ──
    /// 未分类的内部错误，指示 bug 或非预期状态
    #[error("内部错误: {0}")]
    Internal(String),
}

/// 将 [`EvalError`] 转换为 HTTP JSON 响应
///
/// 每个错误变体映射到合适的 HTTP 状态码与错误代码，
/// 响应体包含 `error`（代码）与 `details`（人类可读说明）。
impl IntoResponse for EvalError {
    fn into_response(self) -> Response {
        let (status, error_code, details) = match &self {
            EvalError::ConfigError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "config_error", msg.clone()),
            EvalError::ProviderNotConfigured(name) => (StatusCode::BAD_REQUEST, "provider_not_configured", name.clone()),
            EvalError::LlmCallError(msg) => (StatusCode::BAD_GATEWAY, "llm_call_error", msg.clone()),
            EvalError::LlmRetryExhausted(msg) => (StatusCode::BAD_GATEWAY, "llm_retry_exhausted", msg.clone()),
            EvalError::RateLimitTimeout => (StatusCode::TOO_MANY_REQUESTS, "rate_limit_timeout", String::new()),
            EvalError::RateLimited { retry_after } => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited", format!("建议 {retry_after:?} 后重试"))
            }
            EvalError::Timeout(d) => (StatusCode::GATEWAY_TIMEOUT, "timeout", format!("{d:?}")),
            EvalError::MetricNotFound(name) => (StatusCode::NOT_FOUND, "metric_not_found", name.clone()),
            EvalError::InvalidParams(msg) => (StatusCode::BAD_REQUEST, "invalid_params", msg.clone()),
            EvalError::MetricExecutionError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "metric_execution_error", msg.clone()),
            EvalError::PromptNotFound(name) => (StatusCode::NOT_FOUND, "prompt_not_found", name.clone()),
            EvalError::InvalidPromptVariables(msg) => (StatusCode::BAD_REQUEST, "invalid_prompt_variables", msg.clone()),
            EvalError::PromptRenderError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "prompt_render_error", msg.clone()),
            EvalError::JsonPathNotFound(path) => (StatusCode::BAD_REQUEST, "path_not_found", path.clone()),
            EvalError::RegexNoMatch => (StatusCode::BAD_REQUEST, "regex_no_match", String::new()),
            EvalError::EvalTimeout => (StatusCode::GATEWAY_TIMEOUT, "eval_timeout", String::new()),
            EvalError::JudgeParseFailed(msg) => (StatusCode::UNPROCESSABLE_ENTITY, "judge_parse_failed", msg.clone()),
            EvalError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg.clone()),
        };

        let body = json!({
            "error": error_code,
            "details": details,
        });

        (status, Json(body)).into_response()
    }
}