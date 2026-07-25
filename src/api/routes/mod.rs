//! API 路由模块
//!
//! 按业务域拆分路由：
//!
//! - [`eval`] — `POST /v1/eval`
//! - [`metrics`] — `GET /v1/metrics`
//! - [`results`] — 评测历史查询（罗盘）
//! - [`health`] — `GET /health`
//!
//! 路由组装由 [`v1_routes`] 完成，共享状态由 [`AppState`] 承载。

pub mod eval;
pub mod health;
pub mod metrics;
pub mod results;

use axum::Router;
use std::sync::Arc;

use crate::engine::EvalEngine;
use crate::storage::SqliteStore;

/// v1 路由共享状态
///
/// 通过 axum 的 [`State`](axum::extract::State) 注入到各处理器。
/// `Clone` 是廉价 clone：内部为 `Arc`。
#[derive(Clone)]
pub struct AppState {
    /// 全局共享的评测引擎
    pub engine: Arc<EvalEngine>,
    /// 可选的 SQLite 存储；未启用时返回错误
    pub storage: Option<Arc<SqliteStore>>,
}

/// 构建 v1 版本的路由组
///
/// # Arguments
///
/// * `engine` - 评测引擎
/// * `storage` - 可选存储
///
/// # Returns
///
/// 挂载在 `/v1` 下的 [`Router`]。  
/// 注意：路由注册顺序敏感——`/results/aggregate` 必须早于 `/results/{id}`，
/// 否则会被路径参数匹配吞掉。
pub fn v1_routes(engine: Arc<EvalEngine>, storage: Option<Arc<SqliteStore>>) -> Router {
    let state = AppState { engine, storage };

    Router::new()
        .route("/eval", axum::routing::post(eval::eval_handler))
        .route("/metrics", axum::routing::get(metrics::list_metrics_handler))
        // 评测结果查询（罗盘功能）
        // 注意：路径参数 {id} 会吞掉同层级的字面量路径，因此
        // /results/aggregate、/results/download 必须先于 /results/{id} 注册。
        .route("/results", axum::routing::get(results::list_results))
        .route(
            "/results/aggregate",
            axum::routing::get(results::aggregate_results),
        )
        .route(
            "/results/download",
            axum::routing::get(results::download_results_by_query),
        )
        .route("/results/{id}", axum::routing::get(results::get_result))
        .route(
            "/results/{id}/download",
            axum::routing::get(results::download_result_by_id),
        )
        .with_state(state)
}