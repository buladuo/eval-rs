//! `eval-rs` — 模块化 LLM 评测服务
//!
//! 该 crate 提供基于 HTTP API 的 LLM 评测能力，支持 LLM-as-Judge 与传统 NLP 指标，
//! 可作为独立服务部署，也可作为库嵌入其他 Rust 项目。
//!
//! # 特性
//!
//! - **多指标**：内置 BLEU、ROUGE、Perplexity，并支持通过提示词模板定义自定义 LLM-as-Judge 指标。
//! - **多 Provider**：通过 [`rig`](https://docs.rs/rig) 封装 OpenAI（含 OpenAI 兼容服务）
//!   与 Anthropic 调用，可灵活切换模型。
//! - **可靠性**：内置指数退避重试、RPM / Token 限流、并发控制与评测超时。
//! - **可观测性**：基于 `tracing` 的结构化日志（JSON / 文本），支持请求 ID 关联。
//! - **持久化**：基于 SQLite 的评测结果存储与聚合统计。
//!
//! # 架构分层
//!
//! - **[`api`]** — HTTP 端点层，基于 axum
//! - **[`engine`]** — 评测引擎层，编排评测流程
//! - **[`provider`]** — LLM 调用层，基于 rig，含重试 / 限流 / 并发控制
//! - **[`metrics`]** — 指标计算层，LLM 与非 LLM 指标
//! - **[`prompts`]** — 提示词管理，模板化 / 版本化
//! - **[`preprocessor`]** — 输入预处理，JSON 路径 / 正则提取
//! - **[`settings`]** — 配置加载与校验
//! - **[`error`]** — 统一错误类型
//! - **[`logging`]** — 结构化日志
//! - **[`storage`]** — SQLite 评测结果持久化
//!
//! # 快速开始
//!
//! 一般情况下无需直接以库形式调用，可通过 [`bin` 模块](../main_rs/index.html)
//! 启动 HTTP 服务。如下展示以库形式运行一次评测的核心流程：
//!
//! ```no_run
//! use std::sync::Arc;
//! use eval_rs::engine::EvalEngine;
//! use eval_rs::metrics::registry::MetricRegistry;
//! use eval_rs::provider::manager::ProviderManager;
//! use eval_rs::prompts::registry::PromptRegistry;
//! use eval_rs::settings::AppConfig;
//!
//! # async fn run() -> Result<(), eval_rs::error::EvalError> {
//! let config: AppConfig = eval_rs::settings::load_config()?;
//! let providers = Arc::new(ProviderManager::new(config.providers.clone(), &config.limits));
//! let mut prompts = PromptRegistry::new();
//! prompts.load_from_dir(&config.prompts.dir)?;
//! let prompts = Arc::new(prompts);
//! let metrics = Arc::new(MetricRegistry::new());
//!
//! let engine = EvalEngine::new(metrics, providers, prompts, config.eval.timeout_secs);
//! # Ok(()) }
//! ```
//!
//! # Safety
//!
//! 本 crate 未使用 `unsafe`，所有公开 API 均为安全 Rust。

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
