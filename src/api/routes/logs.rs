//! 日志流端点 — `GET /v1/logs/stream`
//!
//! 通过 Server-Sent Events (SSE) 实时推送日志事件给前端。
//!
//! # 协议
//!
//! - 响应 Content-Type: `text/event-stream`
//! - 每条日志以 `data: <json>\n\n` 格式发送
//! - 空闲时每 15 秒发送一次 `: ping\n\n` 心跳注释，保持连接活跃
//!
//! # Examples
//!
//! 使用 `curl` 订阅日志流：
//!
//! ```sh
//! curl -N http://localhost:8080/v1/logs/stream
//! ```
//!
//! 使用浏览器 EventSource：
//!
//! ```js
//! const es = new EventSource('/v1/logs/stream');
//! es.onmessage = (e) => console.log(JSON.parse(e.data));
//! ```

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::api::routes::AppState;

/// `GET /v1/logs/stream` 处理函数
///
/// 订阅全局日志广播通道，将日志事件以 SSE 形式实时推送给客户端。
///
/// # Arguments
///
/// * `state` - 通过 [`State`] 注入的 [`AppState`]，包含 [`LogBroadcaster`]
///
/// # Returns
///
/// 返回 [`Sse`] 流，Content-Type 为 `text/event-stream`。
/// 每条日志事件序列化为 JSON 并以 `data:` 字段发送。
///
/// # Notes
///
/// - 客户端断开连接时流自动结束（axum 检测到客户端消失）。
/// - 当广播通道中事件积压超过容量时，部分事件会被丢弃（[`BroadcastStream`]
///   会产生 `Err(Lagged)`，此处被过滤跳过）。
/// - 心跳由 [`KeepAlive`] 自动维护，间隔 15 秒。
pub async fn log_stream_handler(State(state): State<AppState>) -> Response {
    let rx = state.log_broadcaster.subscribe();

    // 将 broadcast::Receiver 转换为 Stream，并过滤 Lagged 错误
    let log_stream = BroadcastStream::new(rx).filter_map(|result| async move { result.ok() });

    // 将每个 LogEntry 序列化为 SSE data 事件
    let sse_stream = log_stream.map(|entry| {
        let json = serde_json::to_string(&entry).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, Infallible>(Event::default().data(json))
    });

    let sse = Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    );

    sse.into_response()
}
