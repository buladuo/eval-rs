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
//! - [`llm_judge::LlmJudgeMetric`] — LLM-as-Judge

pub mod bleu;
pub mod llm_judge;
pub mod perplexity;
pub mod registry;
pub mod rouge;