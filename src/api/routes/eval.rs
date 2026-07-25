//! 评测端点 — `POST /v1/eval`
//!
//! 接收评测请求，调用引擎执行评测，返回结果。

use axum::Json;
use axum::extract::State;

use crate::api::models::{EvalRequestBody, EvalResponse};
use crate::api::routes::AppState;
use crate::error::EvalError;

/// `POST /v1/eval` 处理函数
///
/// # Arguments
///
/// * `state` - 通过 [`State`] 注入的 [`AppState`]
/// * `body` - 通过 [`Json`] 解析的 [`EvalRequestBody`]
///
/// # Returns
///
/// 成功时返回 [`EvalResponse`]；失败时由 [`EvalError`] 自动映射为 JSON 错误响应。
///
/// # Errors
///
/// 可能抛出的错误包括：
///
/// - [`EvalError::MetricNotFound`] — 指标未注册
/// - [`EvalError::InvalidParams`] — 缺少必填参数
/// - [`EvalError::MetricExecutionError`] — 指标执行失败
/// - [`EvalError::EvalTimeout`] — 评测超时
pub async fn eval_handler(
    State(state): State<AppState>,
    Json(body): Json<EvalRequestBody>,
) -> Result<Json<EvalResponse>, EvalError> {
    // 提取或生成请求 ID，便于日志追踪
    let request_id = body
        .request_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    tracing::info!(
        metric = %body.metric,
        request_id = %request_id,
        "收到评测请求"
    );

    // 将 HTTP 请求体转换为引擎层请求，并执行评测
    let request = body.to_engine_request();
    let result = state.engine.evaluate(request).await?;

    Ok(Json(EvalResponse::from(result)))
}
