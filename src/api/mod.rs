//! API 层 — HTTP 端点定义与请求 / 响应模型
//!
//! 基于 [axum](https://docs.rs/axum) 框架，提供 RESTful 接口暴露评测能力。
//!
//! # 模块组成
//!
//! - [`models`] — 请求 / 响应 / 错误响应数据类型
//! - [`routes`] — 各路由处理函数及路由组装
//!
//! # 路由概览
//!
//! | 方法   | 路径                       | 说明                       |
//! |--------|----------------------------|----------------------------|
//! | GET    | `/health`                  | 健康检查                   |
//! | POST   | `/v1/eval`                 | 执行一次评测               |
//! | GET    | `/v1/metrics`              | 列出所有已注册指标         |
//! | GET    | `/v1/results`              | 分页查询评测历史           |
//! | GET    | `/v1/results/{id}`         | 查询单条评测记录           |
//! | GET    | `/v1/results/aggregate`    | 按指标聚合统计             |
//! | GET    | `/v1/logs/stream`          | SSE 实时日志流             |
//!
//! # 安全防护
//!
//! 路由器在构建时附加了两层全局防护：
//!
//! - **请求体大小限制**：默认 1 MiB，防止超大 JSON 耗尽内存。
//! - **全局请求超时**：默认 30 秒，防止慢速攻击。
//!
//! 上述限制可通过 [`build_router_with_limits`] 自定义。

pub mod models;
pub mod routes;

use axum::Router;
use std::sync::Arc;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

use crate::engine::EvalEngine;
use crate::logging::broadcaster::LogBroadcaster;
use crate::storage::SqliteStore;

/// 默认请求体大小限制（1 MiB）
pub const DEFAULT_BODY_LIMIT_BYTES: usize = 1024 * 1024;

/// 默认全局请求超时（30 秒）
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// 构建应用根路由（使用默认防护参数）
///
/// 等价于 [`build_router_with_limits`]`(engine, storage, broadcaster,
/// DEFAULT_BODY_LIMIT_BYTES, Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))`。
///
/// # Arguments
///
/// * `engine` - 已初始化的评测引擎 [`EvalEngine`]
/// * `storage` - 可选的 SQLite 存储；当 `None` 时历史查询类接口将返回 500
/// * `log_broadcaster` - 日志广播器，用于 SSE 日志流端点
///
/// # Returns
///
/// 可直接通过 [`axum::serve`] 启动的 [`Router`]。
pub fn build_router(
    engine: Arc<EvalEngine>,
    storage: Option<Arc<SqliteStore>>,
    log_broadcaster: Arc<LogBroadcaster>,
) -> Router {
    build_router_with_limits(
        engine,
        storage,
        log_broadcaster,
        DEFAULT_BODY_LIMIT_BYTES,
        Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
    )
}

/// 构建应用根路由（自定义防护参数）
///
/// 将 v1 路由组嵌套到 `/v1`，并将健康检查端点暴露在 `/health`。
/// 同时附加请求体大小限制和全局请求超时中间件，防止慢速攻击和资源耗尽。
///
/// # Arguments
///
/// * `engine` - 已初始化的评测引擎 [`EvalEngine`]
/// * `storage` - 可选的 SQLite 存储；当 `None` 时历史查询类接口将返回 500
/// * `log_broadcaster` - 日志广播器，用于 SSE 日志流端点
/// * `body_limit_bytes` - 请求体最大字节数
/// * `request_timeout` - 单个请求的全局超时
///
/// # Returns
///
/// 可直接通过 [`axum::serve`] 启动的 [`Router`]。
///
/// # Examples
///
/// ```no_run
/// # use std::sync::Arc;
/// # use std::time::Duration;
/// # use eval_rs::api::build_router_with_limits;
/// # use eval_rs::engine::EvalEngine;
/// # use eval_rs::metrics::registry::MetricRegistry;
/// # use eval_rs::provider::ProviderManager;
/// # use eval_rs::prompts::registry::PromptRegistry;
/// # use eval_rs::logging::broadcaster::LogBroadcaster;
/// # use std::collections::HashMap;
/// # use eval_rs::settings::LimitsConfig;
/// # async fn _example() {
/// let metrics = Arc::new(MetricRegistry::new());
/// let providers = Arc::new(ProviderManager::new(HashMap::new(), &LimitsConfig::default()));
/// let prompts = Arc::new(PromptRegistry::new());
/// let engine = Arc::new(EvalEngine::new(metrics, providers, prompts, 120));
/// let broadcaster = Arc::new(LogBroadcaster::new());
/// let router = build_router_with_limits(
///     engine,
///     None,
///     broadcaster,
///     2 * 1024 * 1024,
///     Duration::from_secs(60),
/// );
/// # }
/// ```
pub fn build_router_with_limits(
    engine: Arc<EvalEngine>,
    storage: Option<Arc<SqliteStore>>,
    log_broadcaster: Arc<LogBroadcaster>,
    body_limit_bytes: usize,
    request_timeout: Duration,
) -> Router {
    // 健康检查路由需要共享 AppState 以探测依赖
    let health_state = routes::AppState {
        engine: engine.clone(),
        storage: storage.clone(),
        log_broadcaster: log_broadcaster.clone(),
    };

    Router::new()
        .nest("/v1", routes::v1_routes(engine, storage, log_broadcaster))
        .route(
            "/health",
            axum::routing::get(routes::health::health_check).with_state(health_state),
        )
        .layer(RequestBodyLimitLayer::new(body_limit_bytes))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::GATEWAY_TIMEOUT,
            request_timeout,
        ))
}
