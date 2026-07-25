//! 日志模块 — 结构化日志初始化与配置
//!
//! 基于 `tracing` 生态，支持 JSON 与普通文本格式输出，支持分级配置。

pub mod request_id;

use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::settings::LogConfig;

/// 初始化全局日志系统
///
/// 根据配置选择 JSON 或文本格式，设置日志级别。
pub fn init_logging(config: &LogConfig) {
    let env_filter = EnvFilter::try_new(&config.level)
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry()
        .with(env_filter);

    match config.format.as_str() {
        "json" | "jsonl" => {
            // json 与 jsonl 同义：每行一条独立 JSON 对象，无外层数组包裹
            registry
                .with(
                    fmt::layer()
                        .json()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        }
        "text" | "pretty" => {
            registry
                .with(
                    fmt::layer()
                        .pretty()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        }
        other => {
            // 未知格式回退到 text，并输出警告
            eprintln!("未知的日志格式 '{other}'，回退到 'text'。支持: json, jsonl, text, pretty");
            registry
                .with(
                    fmt::layer()
                        .pretty()
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true),
                )
                .init();
        }
    }

    tracing::info!(log_format = %config.format, log_level = %config.level, "日志系统初始化完成");
}