//! Provider 管理器 — 注册与按名称路由
//!
//! 整合重试、限流、并发控制，提供统一的 LLM 调用入口。
//!
//! # 调用流程
//!
//! ```text
//! complete() → 并发控制 → RPM限流 → Token限流 → 外部超时 → 重试 → rig_provider
//! ```

use std::collections::HashMap;
use std::time::Duration;

use super::concurrency::ConcurrencyLimiter;
use super::rate_limiter::RateLimiter;
use super::retry::RetryPolicy;
use super::rig_provider::RigProvider;
use super::token_limiter::TokenLimiter;
use super::LlmProvider;
use crate::error::EvalError;
use crate::settings::{LimitsConfig, ProviderConfig};

/// 默认单次 LLM 调用超时（60 秒）
pub const DEFAULT_PROVIDER_TIMEOUT: Duration = Duration::from_secs(60);

/// 限流等待超时（10 秒）
const RATE_LIMIT_WAIT_TIMEOUT: Duration = Duration::from_secs(10);

/// Provider 管理器
pub struct ProviderManager {
    /// 已注册的 provider 列表（名称 → 实例）
    providers: HashMap<String, Box<dyn LlmProvider>>,
    /// 默认 provider 名称
    default_provider: Option<String>,
    /// 重试策略
    retry_policy: RetryPolicy,
    /// RPM 限流器
    rate_limiter: RateLimiter,
    /// Token 限流器（可选）
    token_limiter: Option<TokenLimiter>,
    /// 并发限制器
    concurrency_limiter: ConcurrencyLimiter,
}

impl ProviderManager {
    /// 创建新的 Provider 管理器
    ///
    /// 根据配置构造各 provider 实例，装配限流与重试策略。
    /// 创建失败的 provider 被跳过（打印 error 日志）。
    ///
    /// # Arguments
    ///
    /// * `provider_configs` - provider 配置映射
    /// * `limits` - 全局限流配置
    pub fn new(
        provider_configs: HashMap<String, ProviderConfig>,
        limits: &LimitsConfig,
    ) -> Self {
        let mut providers: HashMap<String, Box<dyn LlmProvider>> = HashMap::new();
        let mut default_provider = None;

        for (name, config) in provider_configs {
            let provider: Option<Box<dyn LlmProvider>> = match config.provider_type.as_str() {
                "openai" => match RigProvider::new_openai(
                    config.api_key.unwrap_or_default(),
                    config.model,
                    config.base_url,
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        tracing::error!(error = %e, provider = %name, "创建 OpenAI provider 失败，跳过");
                        continue;
                    }
                },
                "anthropic" => match RigProvider::new_anthropic(
                    config.api_key.unwrap_or_default(),
                    config.model,
                ) {
                    Ok(p) => Some(Box::new(p)),
                    Err(e) => {
                        tracing::error!(error = %e, provider = %name, "创建 Anthropic provider 失败，跳过");
                        continue;
                    }
                },
                other => {
                    tracing::warn!(provider_type = other, "不支持的 provider 类型，跳过");
                    continue;
                }
            };

            if let Some(provider) = provider {
                if config.default {
                    default_provider = Some(name.clone());
                }
                providers.insert(name, provider);
            }
        }

        Self {
            providers,
            default_provider,
            retry_policy: RetryPolicy::default(),
            rate_limiter: RateLimiter::new(limits.requests_per_minute),
            token_limiter: TokenLimiter::new(limits.tokens_per_minute, limits.tokens_per_second),
            concurrency_limiter: ConcurrencyLimiter::new(limits.max_concurrency),
        }
    }

    /// 调用 LLM，整合重试、限流、并发控制与外部超时
    ///
    /// # Arguments
    ///
    /// * `provider_name` - 指定 provider；为 None 使用默认
    /// * `prompt` - 用户消息
    /// * `model` - 模型名；为 None 使用 provider 默认
    /// * `retry` - 可选重写重试策略
    ///
    /// # Errors
    ///
    /// provider 不存在、调用失败、重试耗尽、超时等情况返回对应错误。
    ///
    /// # Panics
    ///
    /// 不会 panic。
    pub async fn complete(
        &self,
        provider_name: Option<&str>,
        prompt: &str,
        model: Option<&str>,
        retry: Option<&super::super::engine::RetryConfig>,
    ) -> Result<String, EvalError> {
        self.complete_with_timeout(
            provider_name,
            prompt,
            model,
            retry,
            DEFAULT_PROVIDER_TIMEOUT,
        )
        .await
    }

    /// 调用 LLM（自定义超时）
    ///
    /// 与 [`complete`] 相同，但允许调用方指定单次 LLM 调用（含重试）的总超时。
    ///
    /// # Arguments
    ///
    /// * `provider_name` - 指定 provider；为 None 使用默认
    /// * `prompt` - 用户消息
    /// * `model` - 模型名；为 None 使用 provider 默认
    /// * `retry` - 可选重写重试策略
    /// * `timeout` - 单次 LLM 调用（含重试）的总超时
    ///
    /// # Errors
    ///
    /// provider 不存在、调用失败、重试耗尽、超时等情况返回对应错误。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::time::Duration;
    /// # use eval_rs::provider::ProviderManager;
    /// # async fn _example(mgr: &ProviderManager) -> Result<(), Box<dyn std::error::Error>> {
    /// let result = mgr
    ///     .complete_with_timeout(
    ///         Some("glm"),
    ///         "你好",
    ///         None,
    ///         None,
    ///         Duration::from_secs(30),
    ///     )
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn complete_with_timeout(
        &self,
        provider_name: Option<&str>,
        prompt: &str,
        model: Option<&str>,
        retry: Option<&super::super::engine::RetryConfig>,
        timeout: Duration,
    ) -> Result<String, EvalError> {
        let provider_name = provider_name
            .or(self.default_provider.as_deref())
            .ok_or_else(|| EvalError::ProviderNotConfigured("未配置默认 provider".to_string()))?;

        let provider = self
            .providers
            .get(provider_name)
            .ok_or_else(|| EvalError::ProviderNotConfigured(provider_name.to_string()))?;

        let model = model.unwrap_or("default");

        // 并发控制
        let _permit = self.concurrency_limiter.acquire().await?;

        // RPM 限流（带等待超时）
        self.rate_limiter
            .acquire(RATE_LIMIT_WAIT_TIMEOUT)
            .await?;

        // Token 限流（可选，带等待超时）
        if let Some(ref token_limiter) = self.token_limiter {
            token_limiter.acquire(RATE_LIMIT_WAIT_TIMEOUT).await?;
        }

        // 重试策略（请求级 > 全局）
        let retry_policy = match retry {
            Some(r) => RetryPolicy::new(r.max_attempts, r.backoff_ms),
            None => self.retry_policy.clone(),
        };

        tracing::debug!(
            provider = provider_name,
            model = model,
            timeout_secs = timeout.as_secs(),
            "开始 LLM 调用"
        );

        // 外部超时包裹整个重试执行
        let result = tokio::time::timeout(
            timeout,
            retry_policy.execute(|| async { provider.complete(prompt, model).await }),
        )
        .await
        .map_err(|_| {
            tracing::warn!(
                provider = provider_name,
                timeout_secs = timeout.as_secs(),
                "LLM 调用超时"
            );
            EvalError::Timeout(timeout)
        });

        match &result {
            Ok(Ok(_)) => tracing::debug!(provider = provider_name, "LLM 调用成功"),
            Ok(Err(e)) => tracing::error!(provider = provider_name, error = %e, "LLM 调用失败"),
            Err(_) => {}
        }

        result?
    }

    /// 检查 provider 是否存在
    pub fn has_provider(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// 列出所有已注册 provider 名称（用于健康检查与可观测性）
    pub fn list_providers(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// 是否有任何 provider 已注册
    pub fn has_any_provider(&self) -> bool {
        !self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{LimitsConfig, ProviderConfig};

    fn test_limits() -> LimitsConfig {
        LimitsConfig {
            max_concurrency: 10,
            requests_per_minute: 60,
            tokens_per_minute: None,
            tokens_per_second: None,
        }
    }

    #[test]
    fn test_no_providers_returns_empty() {
        let mgr = ProviderManager::new(HashMap::new(), &test_limits());
        assert!(!mgr.has_any_provider());
        assert!(mgr.list_providers().is_empty());
        assert!(!mgr.has_provider("any"));
    }

    #[test]
    fn test_unsupported_provider_type_skipped() {
        let mut configs = HashMap::new();
        configs.insert(
            "bad".to_string(),
            ProviderConfig {
                provider_type: "unsupported".to_string(),
                api_key: None,
                model: "m".to_string(),
                base_url: None,
                default: true,
            },
        );
        let mgr = ProviderManager::new(configs, &test_limits());
        assert!(!mgr.has_any_provider());
    }
}
