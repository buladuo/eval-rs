//! eval-rs 二进制入口
//!
//! 启动入口，组装所有模块并启动 HTTP 服务。
//!
//! 启动顺序：
//!
//! 1. 加载配置 [`settings::AppConfig`]
//! 2. 初始化日志 [`logging::init_logging`]
//! 3. 构造 [`provider::manager::ProviderManager`]、指标与提示词注册表
//! 4. 可选地打开 SQLite 存储 [`storage::SqliteStore`]
//! 5. 构造 [`engine::EvalEngine`] 并构建 axum [`Router`](axum::Router)
//! 6. 启动 HTTP 服务，注册 Ctrl+C / SIGTERM 优雅关闭

use std::sync::Arc;

use crate::engine::EvalEngine;
use crate::metrics::registry::MetricRegistry;
use crate::metrics::{bleu::BleuMetric, perplexity::PerplexityMetric, rouge::RougeMetric};
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;
use crate::settings::AppConfig;

pub mod api;
pub mod engine;
pub mod error;
pub mod logging;
pub mod metrics;
pub mod preprocessor;
pub mod prompts;
pub mod provider;
pub mod settings;
pub mod storage;

/// 二进制主入口
///
/// 异步启动 HTTP 服务并阻塞至收到关闭信号。
///
/// # Errors
///
/// 在以下情况返回错误：
///
/// - 配置加载失败（[`settings::load_config`]）
/// - 提示词模板加载失败
/// - SQLite 存储打开失败
/// - 监听端口绑定失败
///
/// # Panics
///
/// 当无法注册 Ctrl+C 或 SIGTERM 信号处理器时 panic（见 [`shutdown_signal`]）。
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 加载配置（环境变量 EVAL_CONFIG_PATH 可覆盖默认配置文件路径）
    let config: AppConfig = settings::load_config()?;

    // 2. 初始化全局日志（JSON / 文本，根据 LogConfig 选择）
    let log_broadcaster = logging::init_logging(&config.log);

    tracing::info!("eval-rs 服务启动中...");

    // 3. 初始化 Provider 管理器：按配置创建各 provider 实例，并装配限流 / 重试策略
    let provider_manager = Arc::new(ProviderManager::new(
        config.providers.clone(),
        &config.limits,
    ));

    // 4. 从配置目录加载所有提示词模板
    let mut prompt_registry = PromptRegistry::new();
    prompt_registry.load_from_dir(&config.prompts.dir)?;
    let prompt_registry = Arc::new(prompt_registry);

    // 5. 初始化指标注册表
    let mut metric_registry = MetricRegistry::new();

    // 5.1 注册非 LLM 指标（纯算法，无网络调用）
    metric_registry.register(Box::new(RougeMetric));
    metric_registry.register(Box::new(BleuMetric));

    // 5.2 注册 LLM 指标（Perplexity 通过 LLM 评估文本自然度）
    metric_registry.register(Box::new(PerplexityMetric::new(provider_manager.clone())));

    // 5.3 注册所有以 `llm_judge_` 开头的提示词为 LLM-as-Judge 指标
    for prompt_name in prompt_registry.list_names() {
        if prompt_name.starts_with("llm_judge_") {
            let metric_name = prompt_name.clone();
            metric_registry.register(Box::new(crate::metrics::llm_judge::LlmJudgeMetric::new(
                metric_name,
                prompt_name,
                None,
                provider_manager.clone(),
                prompt_registry.clone(),
            )));
        }
    }

    let metric_registry = Arc::new(metric_registry);

    // 6. 可选地初始化 SQLite 存储；禁用时所有历史查询接口将返回 500
    let storage = if config.storage.enabled {
        let db_path = &config.storage.db_path;
        tracing::info!(db_path = %db_path, "初始化 SQLite 存储");
        let store = Arc::new(crate::storage::SqliteStore::open(db_path).await?);
        Some(store)
    } else {
        tracing::info!("存储层已禁用");
        None
    };

    // 7. 构造评测引擎，按需启用存储
    let mut engine_builder = EvalEngine::new(
        metric_registry,
        provider_manager,
        prompt_registry,
        config.eval.timeout_secs,
    );
    if let Some(store) = &storage {
        engine_builder = engine_builder.with_storage(store.clone());
    }
    let engine = Arc::new(engine_builder);

    // 8. 构建 axum 路由并启动 HTTP 服务
    let app = api::build_router(engine, storage, log_broadcaster);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    tracing::info!("HTTP 服务监听在 {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// 等待并响应优雅关闭信号
///
/// 同时监听：
///
/// - **Ctrl+C**（所有平台）
/// - **SIGTERM**（仅 Unix）
///
/// 任一信号到达即返回，触发 [`axum::serve`] 的优雅关闭流程（停止接收新连接、
/// 等待进行中请求完成）。
///
/// # Panics
///
/// 当 [`tokio::signal::ctrl_c`] 或 [`tokio::signal::unix::signal`] 注册失败时 panic。
async fn shutdown_signal() {
    // Ctrl+C 处理（跨平台）
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("无法注册 Ctrl+C 信号处理器");
    };

    // SIGTERM 处理（仅 Unix；非 Unix 平台替换为永未完成的 future）
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("无法注册 SIGTERM 信号处理器")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("收到 Ctrl+C 信号，正在关闭...");
        }
        _ = terminate => {
            tracing::info!("收到 SIGTERM 信号，正在关闭...");
        }
    }
}
