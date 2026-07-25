//! 请求 ID 生成与日志关联
//!
//! 为每个评测请求生成唯一 ID，并通过 tracing span 在日志中关联。

use tracing::Span;
use uuid::Uuid;

/// 生成新的请求 ID
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

/// 在当前的 tracing span 中注入 request_id
///
/// 调用后，该 span 及其子 span 中的所有日志都会自动携带 request_id 字段。
pub fn inject_request_id(request_id: &str) {
    Span::current().record("request_id", request_id);
}

/// 创建一个带有 request_id 的根 span
///
/// # 示例
///
/// ```ignore
/// let span = request_id::new_root_span("evaluate");
/// let _guard = span.enter();
/// // 此后的日志都会携带该 request_id
/// ```
pub fn new_root_span(span_name: &str, request_id: &str) -> Span {
    tracing::info_span!("request_id_span", name = span_name, request_id = %request_id)
}