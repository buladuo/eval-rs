//! 健康检查端点 — `GET /health`
//!
//! 返回服务健康状态，探测 SQLite 连通性和 LLM Provider 可用性。
//! 适用于 Kubernetes / 负载均衡器健康探测。

use axum::extract::State;
use axum::Json;

use crate::api::models::{DependencyHealth, HealthResponse};
use crate::api::routes::AppState;

/// `GET /health` 处理函数
///
/// 探测以下依赖项：
///
/// - **sqlite** — 执行 `SELECT 1` 验证数据库连通性；未启用存储时标记为 `disabled`
/// - **providers** — 检查是否至少注册了一个 LLM provider
///
/// # Arguments
///
/// * `state` - 通过 [`State`] 注入的 [`AppState`]
///
/// # Returns
///
/// 当所有启用的依赖均健康时返回 `200 { "status": "ok" }`，
/// 否则返回 `200 { "status": "degraded", "dependencies": {...} }`。
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let mut dependencies = std::collections::HashMap::new();
    let mut degraded = false;

    // 探测 SQLite
    match &state.storage {
        Some(store) => match store.health_check().await {
            Ok(()) => {
                dependencies.insert(
                    "sqlite".to_string(),
                    DependencyHealth {
                        status: "ok".to_string(),
                        error: None,
                    },
                );
            }
            Err(e) => {
                degraded = true;
                dependencies.insert(
                    "sqlite".to_string(),
                    DependencyHealth {
                        status: "error".to_string(),
                        error: Some(e.to_string()),
                    },
                );
            }
        },
        None => {
            dependencies.insert(
                "sqlite".to_string(),
                DependencyHealth {
                    status: "disabled".to_string(),
                    error: None,
                },
            );
        }
    }

    // 探测 LLM providers
    {
        let engine = &state.engine;
        // 通过 ProviderManager::has_any_provider 检查
        if engine.has_any_provider() {
            dependencies.insert(
                "providers".to_string(),
                DependencyHealth {
                    status: "ok".to_string(),
                    error: None,
                },
            );
        } else {
            degraded = true;
            dependencies.insert(
                "providers".to_string(),
                DependencyHealth {
                    status: "error".to_string(),
                    error: Some("no LLM providers configured".to_string()),
                },
            );
        }
    }

    let status = if degraded {
        "degraded".to_string()
    } else {
        "ok".to_string()
    };

    Json(HealthResponse {
        status,
        dependencies,
    })
}
