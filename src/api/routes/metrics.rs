//! 指标列表端点 — `GET /v1/metrics`
//!
//! 返回所有已注册指标的元信息（名称、类型、参数 schema）。

use axum::extract::State;
use axum::Json;

use crate::api::models::MetricsListResponse;
use crate::api::routes::AppState;

/// `GET /v1/metrics` 处理函数
///
/// # Returns
///
/// 返回 [`MetricsListResponse`]，包含每个指标的名称、类型（`llm` / `non_llm`）
/// 与参数 schema，可用于前端动态构建评测表单。
pub async fn list_metrics_handler(State(state): State<AppState>) -> Json<MetricsListResponse> {
    let metrics = state.engine.list_metrics();
    Json(MetricsListResponse { metrics })
}