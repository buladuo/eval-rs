//! Provider 层 — LLM 调用封装
//!
//! 基于 `rig` crate 封装 LLM 调用，支持多 provider、重试、并发控制、速率限制。
//!
//! # 子模块
//!
//! - [`concurrency`] — 并发控制
//! - [`manager`] — Provider 注册与分发
//! - [`rate_limiter`] — RPM 限流
//! - [`retry`] — 指数退避重试
//! - [`rig_provider`] — rig 调用封装
//! - [`token_limiter`] — Token 限流

pub mod concurrency;
pub mod manager;
pub mod rate_limiter;
pub mod retry;
pub mod rig_provider;
pub mod token_limiter;

pub use manager::ProviderManager;

use async_trait::async_trait;

use crate::error::EvalError;

/// LLM Provider 抽象 trait
///
/// 所有 LLM provider 需实现此 trait，以便上层统一调用。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider 名称
    fn name(&self) -> &str;

    /// 发送完整请求并获取响应
    ///
    /// # Arguments
    ///
    /// * `prompt` - 用户消息
    /// * `model` - 模型名称
    ///
    /// # Errors
    ///
    /// 网络 / 认证 / 限流等错误返回 [`EvalError::LlmCallError`]。
    async fn complete(&self, prompt: &str, model: &str) -> Result<String, EvalError>;
}