//! 指标层 — 指标 trait 定义与实现
//!
//! 定义 `Metric` trait，所有指标（LLM 与非 LLM）均实现该 trait。
//! 通过 `MetricRegistry` 注册表按名称路由。
//!
//! # 内置指标
//!
//! - [`bleu::BleuMetric`] — BLEU 评测
//! - [`rouge::RougeMetric`] — ROUGE-N / ROUGE-L
//! - [`perplexity::PerplexityMetric`] — 基于 LLM 的困惑度
//! - [`judge`] — 多步 LLM-as-Judge 指标套件（多 prompt 调用 + 数学聚合）

pub mod bleu;
pub mod judge;
pub mod perplexity;
pub mod registry;
pub mod rouge;
