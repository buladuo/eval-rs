//! 日志广播模块 — 将 tracing 事件通过 SSE 推送给前端
//!
//! 通过自定义 [`tracing_subscriber::Layer`] 将每个日志事件广播到
//! [`tokio::sync::broadcast`] 通道，前端通过 `GET /v1/logs/stream`
//! 建立 SSE 连接即可实时接收日志流。
//!
//! # Architecture
//!
//! ```text
//! tracing 事件
//!     │
//!     ▼
//! ┌─────────────────┐
//! │ BroadcastLayer  │  (tracing_subscriber::Layer)
//! │  捕获 event →    │
//! │  序列化为 JSON   │
//! └────────┬────────┘
//!          │ tokio::sync::broadcast::Sender
//!          ▼
//! ┌─────────────────┐
//! │ broadcast 通道   │  capacity = 1024
//! └────────┬────────┘
//!          │ broadcast::Receiver（每个 SSE 客户端一个）
//!          ▼
//!     SSE 响应流
//! ```

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;

/// 广播通道容量 — 超过此数量的未消费事件会被丢弃
const BROADCAST_CAPACITY: usize = 1024;

/// 日志条目 — 广播到 SSE 客户端的结构化日志数据
///
/// 每个条目包含日志的时间戳、级别、消息和可选的源位置信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// ISO 8601 格式的事件时间戳
    pub timestamp: String,
    /// 日志级别（`TRACE`、`DEBUG`、`INFO`、`WARN`、`ERROR`）
    pub level: String,
    /// 日志消息文本
    pub message: String,
    /// tracing target（通常为模块路径）
    pub target: Option<String>,
    /// 源文件名
    pub file: Option<String>,
    /// 源文件行号
    pub line: Option<u32>,
}

/// 日志广播器
///
/// 管理日志事件的广播通道，供 SSE 端点使用。
///
/// 通过 [`LogBroadcaster::new`] 创建后，将 [`BroadcastLayer`] 附加到
/// tracing subscriber 即可自动广播所有日志事件。SSE 客户端通过
/// [`LogBroadcaster::subscribe`] 获取接收端。
///
/// # Examples
///
/// ```no_run
/// # use std::sync::Arc;
/// # use eval_rs::logging::broadcaster::LogBroadcaster;
/// let broadcaster = Arc::new(LogBroadcaster::new());
/// let mut rx = broadcaster.subscribe();
/// // rx 在 SSE handler 中使用
/// ```
#[derive(Debug, Clone)]
pub struct LogBroadcaster {
    sender: broadcast::Sender<LogEntry>,
}

impl Default for LogBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

impl LogBroadcaster {
    /// 创建新的日志广播器
    ///
    /// 初始化一个具有固定容量的 [`broadcast`] 通道。当消费者落后于
    /// 生产者超过通道容量时，最旧的未消费事件将被丢弃。
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self { sender }
    }

    /// 订阅日志广播流
    ///
    /// 返回一个新的 [`broadcast::Receiver`]，仅接收调用后产生的事件。
    /// 每个 SSE 客户端应获得独立的 receiver。
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }
}

/// tracing 层 — 捕获日志事件并广播
///
/// 作为 [`tracing_subscriber::Layer`] 的实现，拦截所有经过 filter
/// 的 tracing 事件，将结构化字段序列化为 [`LogEntry`] 后发送到广播通道。
///
/// 线程安全：内部通过 `Arc<broadcast::Sender>` 共享，满足 `Send + Sync`。
pub struct BroadcastLayer {
    sender: Arc<broadcast::Sender<LogEntry>>,
}

impl BroadcastLayer {
    /// 创建新的广播层
    ///
    /// # Arguments
    ///
    /// * `broadcaster` - 共享的 [`LogBroadcaster`] 实例
    pub fn new(broadcaster: &LogBroadcaster) -> Self {
        Self {
            sender: Arc::new(broadcaster.sender.clone()),
        }
    }
}

impl<S> Layer<S> for BroadcastLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();

        let mut visitor = EventFieldVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);

        let entry = LogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: metadata.level().to_string(),
            message: visitor.message,
            target: Some(metadata.target().to_string()),
            file: metadata.file().map(|s| s.to_string()),
            line: metadata.line(),
        };

        // 丢弃发送错误（通道满或无消费者）
        let _ = self.sender.send(entry);
    }
}

/// 字段访问器 — 从 tracing 事件中提取 message 字段
///
/// tracing 宏（`info!`、`error!` 等）的第一个参数始终为 `message`，
/// 通过 `record_str` 被记录为字符串值。
struct EventFieldVisitor {
    message: String,
}

impl tracing::field::Visit for EventFieldVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, val: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{val:?}");
        }
    }
}
