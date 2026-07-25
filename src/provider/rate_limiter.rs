//! RPM 令牌桶限流器
//!
//! 基于令牌桶算法控制 LLM Provider 的每分钟请求数（RPM）。
//! 当令牌不足时，任务将异步等待令牌补充，直到超时返回错误。
//!
//! # 并发模型
//!
//! - 内部状态使用 [`tokio::sync::Mutex`] 保护，等待期间不阻塞线程。
//! - 桶以固定频率（每 100ms 补充 1 个令牌）异步轮询，而非忙等待。
//!
//! # Examples
//!
//! ```
//! use std::time::Duration;
//! use eval_rs::provider::rate_limiter::RateLimiter;
//!
//! #[tokio::main]
//! async fn main() {
//!     let limiter = RateLimiter::new(10);
//!     limiter.acquire(Duration::from_secs(5)).await.unwrap();
//! }
//! ```

use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 令牌桶限流器
///
/// 通过令牌桶算法控制每分钟请求速率，超时未获取令牌将返回 [`EvalError::RateLimited`]。
///
/// # 内部实现
///
/// - `max_tokens` — 桶容量，也等于每分钟允许的最大请求数。
/// - `refill_interval` — 令牌补充间隔，`1000ms / max_tokens`（至少 1ms）。
/// - `available` — 当前可用令牌数。
/// - `last_refill` — 上次补充时间戳，用于计算经过时间应补充的令牌数。
pub struct RateLimiter {
    refill_interval: Duration,
    inner: Mutex<RateLimiterInner>,
}

struct RateLimiterInner {
    max_tokens: u64,
    available: u64,
    last_refill: Instant,
}

impl RateLimiter {
    /// 创建一个新的 RPM 限流器
    ///
    /// # Arguments
    ///
    /// * `rpm` — 每分钟最大请求数（RPM）。`rpm = 0` 时所有请求将立即超时。
    ///
    /// # Panics
    ///
    /// 不会 panic。
    pub fn new(rpm: u64) -> Self {
        // 每 1000ms 补充 rpm 个令牌 → 每 1000/rpm ms 补充 1 个令牌
        // 使用 checked_div 避免除零，rpm 为 0 时回退到 1ms 间隔
        let interval_ms = 1000u64.checked_div(rpm).unwrap_or(1);
        Self {
            refill_interval: Duration::from_millis(interval_ms.max(1)),
            inner: Mutex::new(RateLimiterInner {
                max_tokens: rpm,
                available: rpm,
                last_refill: Instant::now(),
            }),
        }
    }

    /// 获取一个请求令牌
    ///
    /// 获取成功后令牌数减 1；令牌不足时异步等待补充，每 100ms 尝试一次，
    /// 超过 `timeout` 后返回错误。
    ///
    /// # Arguments
    ///
    /// * `timeout` — 最大等待时间。
    ///
    /// # Errors
    ///
    /// 返回 [`EvalError::RateLimited`] 当等待超时且仍无可用令牌。
    ///
    /// # Panics
    ///
    /// 不会 panic（Mutex 使用 `tokio::sync::Mutex`，不存在中毒问题）。
    pub async fn acquire(&self, timeout: Duration) -> Result<(), crate::error::EvalError> {
        let deadline = Instant::now() + timeout;

        loop {
            // 尝试获取令牌
            {
                let mut inner = self.inner.lock().await;
                inner.refill();

                if inner.available > 0 {
                    inner.available -= 1;
                    return Ok(());
                }
            } // MutexGuard 在此 drop，不阻塞其他任务

            // 令牌不足，检查超时
            if Instant::now() >= deadline {
                return Err(crate::error::EvalError::RateLimited {
                    retry_after: Some(self.refill_interval),
                });
            }

            // 等待一个补充间隔后重试
            tokio::time::sleep(self.refill_interval).await;
        }
    }
}

impl RateLimiterInner {
    /// 根据流逝时间补充令牌，不超过 `max_tokens`
    ///
    /// 令牌补充速率基于 `max_tokens / 60` tokens/second（即 RPM 转 TPS）。
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let rate = self.max_tokens as f64 / 60.0; // tokens per second
        let new_tokens = (elapsed * rate) as u64;
        if new_tokens > 0 {
            self.available = (self.available + new_tokens).min(self.max_tokens);
            self.last_refill = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_rate_limit_within_capacity() {
        let limiter = RateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.acquire(Duration::from_millis(10)).await.is_ok());
        }
        // 第6个应超时（5 RPM = 1 token / 12s，10ms 内无法补充）
        assert!(limiter.acquire(Duration::from_millis(50)).await.is_err());
    }

    #[tokio::test]
    async fn test_rate_limit_refill() {
        // 600 RPM = 10 TPS = 1 token / 100ms
        let limiter = RateLimiter::new(600);
        // 消耗完
        for _ in 0..600 {
            let _ = limiter.acquire(Duration::from_millis(1)).await;
        }
        // 等待令牌补充（100ms 后应至少补充 1 个）
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(limiter.acquire(Duration::from_millis(10)).await.is_ok());
    }
}
