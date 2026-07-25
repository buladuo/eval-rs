//! 存储模块 — 评测结果持久化与查询
//!
//! 使用 SQLite + sqlx 实现评测结果的持久化存储，支持多种查询维度：
//! - 按请求 ID 查询
//! - 按指标名查询
//! - 按时间范围查询
//! - 按分数范围过滤
//! - 聚合统计（平均分、最大/最小、计数等）

pub mod models;
pub mod store;

pub use models::{AggregationResult, EvalResultRow, QueryParams};
pub use store::SqliteStore;
