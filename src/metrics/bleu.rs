//! BLEU 指标实现 — BLEU-N (N=1..4)
//!
//! BLEU (Bilingual Evaluation Understudy) 是评估机器翻译质量的指标。
//! 基于 N-gram 精确度与简短惩罚 (Brevity Penalty)。
//!
//! # 算法概要
//!
//! 对 N=1..=max_n 分别计算 N-gram 精度，取几何平均后乘以长度惩罚因子。

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::registry::{Metric, MetricOutput};
use crate::error::EvalError;

/// BLEU 指标
pub struct BleuMetric;

#[async_trait]
impl Metric for BleuMetric {
    fn name(&self) -> &str {
        "bleu"
    }

    fn metric_type(&self) -> &str {
        "non_llm"
    }

    /// 参数 schema
    ///
    /// - `n`: 最大 N-gram 大小（1-4），默认 4
    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "n".to_string(),
            json!({"type": "integer", "description": "最大 N-gram 大小 (1-4)", "default": 4}),
        );
        schema
    }

    /// 执行 BLEU 评测。
    ///
    /// 从 `input` 中取出 `reference` 与 `hypothesis` 文本，按参数 `n` 计算 BLEU-N 分数。
    ///
    /// # Arguments
    ///
    /// * `params` - 参数键值，支持 `n`（最大 N-gram 大小，1-4，默认 4）。
    /// * `input` - 评测输入，需含 `reference` 与 `hypothesis` 字符串字段。
    ///
    /// # Returns
    ///
    /// 返回 [`MetricOutput`]，其中 `score` 为 BLEU 分数（0.0~1.0）。
    ///
    /// # Errors
    ///
    /// 当 `reference` 或 `hypothesis` 字段缺失时返回 [`EvalError::InvalidParams`]。
    async fn evaluate(
        &self,
        params: &HashMap<String, Value>,
        input: &Value,
    ) -> Result<MetricOutput, EvalError> {
        let reference = input
            .get("reference")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 reference 字段".to_string()))?;
        let hypothesis = input
            .get("hypothesis")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 hypothesis 字段".to_string()))?;

        let max_n = params.get("n").and_then(|v| v.as_u64()).unwrap_or(4) as usize;

        let max_n = max_n.clamp(1, 4);

        Ok(compute_bleu(reference, hypothesis, max_n))
    }
}

/// 计算 BLEU 分数
///
/// 对 N=1..=max_n 逐阶计算精度，取几何平均后乘以 brevity penalty。
fn compute_bleu(reference: &str, hypothesis: &str, max_n: usize) -> MetricOutput {
    let ref_tokens: Vec<&str> = reference.split_whitespace().collect();
    let hyp_tokens: Vec<&str> = hypothesis.split_whitespace().collect();

    if hyp_tokens.is_empty() {
        return MetricOutput {
            score: 0.0,
            details: json!({"error": "hypothesis 为空"}),
        };
    }

    let mut log_avg_precision = 0.0;
    let mut ngram_details = Vec::new();

    for n in 1..=max_n {
        let (precision, clipped_count, total_count) = ngram_precision(&ref_tokens, &hyp_tokens, n);

        if precision > 0.0 {
            log_avg_precision += precision.ln();
        } else {
            // 如果某个 N-gram 精度为 0，BLEU 为 0
            log_avg_precision = f64::NEG_INFINITY;
            ngram_details.push(json!({
                "n": n,
                "precision": 0.0,
                "clipped": 0,
                "total": total_count,
            }));
            break;
        }

        ngram_details.push(json!({
            "n": n,
            "precision": precision,
            "clipped": clipped_count,
            "total": total_count,
        }));
    }

    // 简短惩罚 (Brevity Penalty)
    let bp = if hyp_tokens.len() < ref_tokens.len() {
        let ratio = hyp_tokens.len() as f64 / ref_tokens.len() as f64;
        (1.0 - ratio).exp()
    } else {
        1.0
    };

    let bleu_score = if log_avg_precision == f64::NEG_INFINITY {
        0.0
    } else {
        bp * (log_avg_precision / max_n as f64).exp()
    };

    MetricOutput {
        score: bleu_score,
        details: json!({
            "bleu": bleu_score,
            "brevity_penalty": bp,
            "ngrams": ngram_details,
            "n": max_n,
        }),
    }
}

/// 计算 N-gram 精确度（带 clipping）
///
/// 返回 `(precision, clipped_count, total_count)`。
fn ngram_precision(reference: &[&str], hypothesis: &[&str], n: usize) -> (f64, usize, usize) {
    use std::collections::HashMap as StdHashMap;

    if hypothesis.len() < n {
        return (0.0, 0, 0);
    }

    let mut ref_counts: StdHashMap<Vec<&str>, usize> = StdHashMap::new();
    let mut hyp_counts: StdHashMap<Vec<&str>, usize> = StdHashMap::new();

    for window in reference.windows(n) {
        *ref_counts.entry(window.to_vec()).or_insert(0) += 1;
    }

    for window in hypothesis.windows(n) {
        *hyp_counts.entry(window.to_vec()).or_insert(0) += 1;
    }

    let mut clipped_count = 0;
    let mut total_count = 0;

    for (ngram, &hyp_count) in &hyp_counts {
        let ref_count = ref_counts.get(ngram).copied().unwrap_or(0);
        clipped_count += hyp_count.min(ref_count);
        total_count += hyp_count;
    }

    let precision = if total_count == 0 {
        0.0
    } else {
        clipped_count as f64 / total_count as f64
    };

    (precision, clipped_count, total_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bleu_exact_match() {
        let metric = BleuMetric;
        let mut params = HashMap::new();
        params.insert("n".to_string(), json!(4));
        let input = json!({
            "reference": "the cat sat on the mat",
            "hypothesis": "the cat sat on the mat",
        });
        let result = metric.evaluate(&params, &input).await.unwrap();
        assert!((result.score - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_bleu_no_match() {
        let metric = BleuMetric;
        let mut params = HashMap::new();
        params.insert("n".to_string(), json!(4));
        let input = json!({
            "reference": "the cat sat on the mat",
            "hypothesis": "dogs run fast everywhere",
        });
        let result = metric.evaluate(&params, &input).await.unwrap();
        assert!(result.score < 0.1);
    }
}
