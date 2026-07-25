//! 评测结果查询端点 — GET /v1/results
//!
//! 提供评测结果的查询、详情查看和聚合统计功能（罗盘）。

use axum::extract::{Path, Query, State};
use axum::http::header::{self, HeaderMap};
use axum::response::IntoResponse;
use axum::Json;

use crate::api::routes::AppState;
use crate::error::EvalError;
use crate::storage::models::{AggregationResult, QueryParams};

/// GET /v1/results — 列表查询（支持分页与多维度过滤）
///
/// Query 参数：
/// - `metric`: 指标名过滤
/// - `request_id`: 请求 ID 过滤
/// - `min_score` / `max_score`: 评分范围
/// - `start_time` / `end_time`: 时间范围（ISO 8601）
/// - `provider` / `model`: provider/模型过滤
/// - `offset` / `limit`: 分页（默认 0 / 20，最大 100）
pub async fn list_results(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<Vec<crate::storage::models::EvalResultRow>>, EvalError> {
    let store = state
        .storage
        .as_ref()
        .ok_or_else(|| EvalError::Internal("存储层未启用".to_string()))?;

    let rows = store.query(&params).await?;
    Ok(Json(rows))
}

/// GET /v1/results/:id — 查询单条记录详情
pub async fn get_result(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::storage::models::EvalResultRow>, EvalError> {
    let store = state
        .storage
        .as_ref()
        .ok_or_else(|| EvalError::Internal("存储层未启用".to_string()))?;

    match store.get_by_id(id).await? {
        Some(row) => Ok(Json(row)),
        None => Err(EvalError::Internal(format!("未找到 ID={id} 的记录"))),
    }
}

/// GET /v1/results/aggregate — 聚合统计
///
/// Query 参数：
/// - `metric`（必填）: 指标名
/// - `start_time` / `end_time`: 时间范围（ISO 8601）
pub async fn aggregate_results(
    State(state): State<AppState>,
    Query(params): Query<AggregateQuery>,
) -> Result<Json<AggregationResult>, EvalError> {
    let store = state
        .storage
        .as_ref()
        .ok_or_else(|| EvalError::Internal("存储层未启用".to_string()))?;

    let agg = store
        .aggregate(
            &params.metric,
            params.start_time.as_deref(),
            params.end_time.as_deref(),
        )
        .await?;
    Ok(Json(agg))
}

/// 聚合查询参数
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AggregateQuery {
    /// 指标名（必填）
    pub metric: String,
    /// 开始时间（ISO 8601）
    pub start_time: Option<String>,
    /// 结束时间（ISO 8601）
    pub end_time: Option<String>,
}

/// GET /v1/results/{id}/download — 按数据库主键下载单条记录为 JSON 文件
///
/// 返回的响应设置了 `Content-Disposition: attachment` 头，浏览器会自动触发文件下载。
/// 文件名为 `eval-result-{id}.json`。
pub async fn download_result_by_id(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, EvalError> {
    let store = state
        .storage
        .as_ref()
        .ok_or_else(|| EvalError::Internal("存储层未启用".to_string()))?;

    let row = match store.get_by_id(id).await? {
        Some(row) => row,
        None => return Err(EvalError::Internal(format!("未找到 ID={id} 的记录"))),
    };

    let filename = format!("eval-result-{}.json", id);
    let body = serde_json::to_string_pretty(&row)
        .map_err(|e| EvalError::Internal(format!("序列化失败: {e}")))?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
    );

    Ok((headers, body))
}

/// GET /v1/results/download — 按查询条件批量下载结果为 JSON 数组文件
///
/// 支持与 `GET /v1/results` 相同的过滤参数：
/// - `metric`、`request_id`、`min_score`、`max_score`
/// - `start_time`、`end_time`、`provider`、`model`
///
/// 返回 `Content-Disposition: attachment` 触发浏览器下载，文件名为
/// `eval-results-{metric}-{timestamp}.json`。
pub async fn download_results_by_query(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<impl IntoResponse, EvalError> {
    let store = state
        .storage
        .as_ref()
        .ok_or_else(|| EvalError::Internal("存储层未启用".to_string()))?;

    let rows = store.query(&params).await?;

    if rows.is_empty() {
        return Err(EvalError::Internal("查询结果为空，无法下载".to_string()));
    }

    // 用 metric 名和时间戳构造文件名
    let metric_part = params
        .metric
        .as_deref()
        .unwrap_or("all");
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("eval-results-{}-{}.json", metric_part, timestamp);

    let body = serde_json::to_string_pretty(&rows)
        .map_err(|e| EvalError::Internal(format!("序列化失败: {e}")))?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
    );

    Ok((headers, body))
}
