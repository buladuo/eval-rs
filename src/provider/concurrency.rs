//! 并发控制 — 最大并发请求数限制
//!
//! 基于 tokio `Semaphore` 实现，超过限制的请求排队等待。

use std::sync::Arc;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// 并发限制器
#[derive(Clone)]
pub struct ConcurrencyLimiter {
    semaphore: Arc<Semaphore>,
}

impl ConcurrencyLimiter {
    /// 创建新的并发限制器
    ///
    /// `max_concurrency` — 最大并发数
    pub fn new(max_concurrency: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrency)),
        }
    }

    /// 获取并发许可，等待直到有可用槽位
    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, crate::error::EvalError> {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| crate::error::EvalError::Internal(e.to_string()))
    }
}
