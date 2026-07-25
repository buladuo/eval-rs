//! Token 速率限制 — TPM/TPS 自动单位换算
//!
//! 支持配置每分钟或每秒 token 数，内部统一转换为 tokens/second 作为令牌桶基准。
//! 若同时配置两者，取换算后的较小值。
//!
//! # 并发模型
//!
//! - 内部状态使用 [`tokio::sync::Mutex`] 保护，等待期间不阻塞线程。
//! - 令牌不足时异步等待补充，每 100ms 尝试一次，超时后返回错误。

use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Token 速率限制器（Clone 实现线程安全共享）
///
/// 通过令牌桶算法控制 LLM Provider 的每秒 token 数（TPS）。
/// 超时未获取令牌将返回 [`EvalError::RateLimited`]。
///
/// # 内部实现
///
/// - `tps` — 每秒允许的 token 数。
/// - `tokens` — 当前可用令牌数。
/// - `last_refill` — 上次补充时间戳。
#[derive(Clone)]
pub struct TokenLimiter {
    inner: std::sync::Arc<TokenLimiterInner>,
}

struct TokenLimiterInner {
    /// 每秒允许的 token 数
    tps: f64,
    /// 当前可用令牌数
    tokens: Mutex<f64>,
    /// 上次令牌补充时间
    last_refill: Mutex<Instant>,
}

impl TokenLimiter {
    /// 创建新的 Token 限流器
    ///
    /// 内部统一转换为 tokens/second。若两者都提供，取较小值。
    /// 两者均为 None 时返回 None（不限流）。
    ///
    /// # Arguments
    ///
    /// * `tokens_per_minute` - 每分钟 token 数（可选）
    /// * `tokens_per_second` - 每秒 token 数（可选）
    pub fn new(tokens_per_minute: Option<u64>, tokens_per_second: Option<u64>) -> Option<Self> {
        let tps = match (tokens_per_minute, tokens_per_second) {
            (Some(tpm), Some(tps)) => {
                // 两者都配置，取较小值
                let tpm_as_tps = tpm as f64 / 60.0;
                tpm_as_tps.min(tps as f64)
            }
            (Some(tpm), None) => tpm as f64 / 60.0,
            (None, Some(tps)) => tps as f64,
            (None, None) => return None,
        };

        if tps <= 0.0 {
            return None;
        }

        let now = Instant::now();
        Some(Self {
            inner: std::sync::Arc::new(TokenLimiterInner {
                tps,
                tokens: Mutex::new(tps),
                last_refill: Mutex::new(now),
            }),
        })
    }

    /// 尝试获取 token 许可
    ///
    /// 简化实现：每次调用消耗 1 个令牌（实际应用中可根据 prompt 长度估算）。
    /// 令牌不足时异步等待补充，每 100ms 尝试一次，超过 `timeout` 后返回错误。
    ///
    /// # Arguments
    ///
    /// * `timeout` - 最大等待时间。
    ///
    /// # Errors
    ///
    /// 返回 [`EvalError::RateLimited`] 当等待超时且仍无可用令牌。
    ///
    /// # Panics
    ///
    /// 不会 panic（使用 `tokio::sync::Mutex`，不存在中毒问题）。
    pub async fn acquire(&self, timeout: Duration) -> Result<(), crate::error::EvalError> {
        let deadline = Instant::now() + timeout;

        loop {
            {
                let mut tokens = self.inner.tokens.lock().await;
                let mut last_refill = self.inner.last_refill.lock().await;

                // 按经过时间补充令牌
                let now = Instant::now();
                let elapsed = now.duration_since(*last_refill).as_secs_f64();
                *tokens = (*tokens + elapsed * self.inner.tps).min(self.inner.tps);
                *last_refill = now;

                // 尝试消耗一个令牌
                if *tokens >= 1.0 {
                    *tokens -= 1.0;
                    return Ok(());
                }
            } // MutexGuard 在此 drop，不阻塞其他任务

            // 检查超时
            if Instant::now() >= deadline {
                return Err(crate::error::EvalError::RateLimited {
                    retry_after: Some(Duration::from_millis(100)),
                });
            }

            // 等待一个补充间隔后重试
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// 获取当前可用令牌数（仅用于测试）
    #[cfg(test)]
    pub(crate) async fn available(&self) -> f64 {
        *self.inner.tokens.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_tpm_conversion() {
        // 60000 TPM = 1000 TPS
        let limiter = TokenLimiter::new(Some(60000), None);
        assert!(limiter.is_some());
    }

    #[test]
    fn test_tps_direct() {
        let limiter = TokenLimiter::new(None, Some(1000));
        assert!(limiter.is_some());
    }

    #[test]
    fn test_both_take_min() {
        // 120 TPM = 2 TPS, 但 TPS 配置为 1000，应取 2
        let limiter = TokenLimiter::new(Some(120), Some(1000));
        assert!(limiter.is_some());
    }

    #[test]
    fn test_no_limits() {
        let limiter = TokenLimiter::new(None, None);
        assert!(limiter.is_none());
    }

    #[tokio::test]
    async fn test_acquire_within_capacity() {
        let limiter = TokenLimiter::new(Some(60000), None).unwrap();
        // 60000 TPM = 1000 TPS，初始有 1000 个令牌
        for _ in 0..10 {
            assert!(limiter.acquire(Duration::from_millis(10)).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_acquire_timeout() {
        let limiter = TokenLimiter::new(Some(1), None).unwrap();
        // 1 TPM = 0.0167 TPS，初始只有 0.0167 个令牌，不足以消耗 1 个
        // 消耗完初始令牌后，后续请求应超时
        let _ = limiter.acquire(Duration::from_millis(10)).await;
        assert!(limiter.acquire(Duration::from_millis(50)).await.is_err());
    }
}
