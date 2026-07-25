//! 重试策略 — 指数退避重试
//!
//! 支持区分可重试与不可重试错误，按指数退避策略重试。

use std::time::Duration;

use crate::error::EvalError;

/// 重试策略配置
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 初始退避时间（毫秒）
    pub backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_ms: 1000,
        }
    }
}

impl RetryPolicy {
    /// 创建新的重试策略
    pub fn new(max_attempts: u32, backoff_ms: u64) -> Self {
        Self {
            max_attempts,
            backoff_ms,
        }
    }

    /// 执行带重试的异步操作
    ///
    /// `operation` 返回 `Result<T, EvalError>`，其中可重试的错误会被重试。
    pub async fn execute<F, Fut, T>(&self, mut operation: F) -> Result<T, EvalError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, EvalError>>,
    {
        let mut last_error = None;

        for attempt in 1..=self.max_attempts {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(err) => {
                    if !is_retryable(&err) {
                        return Err(err);
                    }

                    if attempt < self.max_attempts {
                        let delay = self.backoff_ms * (1u64 << (attempt - 1)); // 指数退避
                        tracing::warn!(
                            attempt,
                            max_attempts = self.max_attempts,
                            delay_ms = delay,
                            error = %err,
                            "LLM 调用失败，即将重试"
                        );
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }

                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| EvalError::LlmRetryExhausted("重试耗尽".to_string())))
    }
}

/// 判断错误是否可重试
///
/// 可重试错误：429（限流）、500/502/503（服务端错误）
/// 不可重试错误：400/401/403/404（客户端错误）
fn is_retryable(err: &EvalError) -> bool {
    matches!(
        err,
        EvalError::LlmCallError(_)
            | EvalError::RateLimitTimeout
            | EvalError::RateLimited { .. }
            | EvalError::Timeout(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_retry_success_on_first_try() {
        let policy = RetryPolicy::new(3, 10);
        let result = policy
            .execute(|| async { Ok::<_, EvalError>(42) })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let policy = RetryPolicy::new(2, 10);
        let call_count = std::cell::Cell::new(0);
        let result: Result<i32, EvalError> = policy
            .execute(|| async {
                call_count.set(call_count.get() + 1);
                Err(EvalError::LlmCallError("server error".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert_eq!(call_count.get(), 2);
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let policy = RetryPolicy::new(3, 10);
        let call_count = std::cell::Cell::new(0);
        let result: Result<i32, EvalError> = policy
            .execute(|| async {
                call_count.set(call_count.get() + 1);
                Err(EvalError::InvalidParams("bad request".to_string()))
            })
            .await;
        assert!(result.is_err());
        assert_eq!(call_count.get(), 1);
    }
}