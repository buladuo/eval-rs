//! ROUGE 指标实现 — ROUGE-N 与 ROUGE-L
//!
//! ROUGE (Recall-Oriented Understudy for Gisting Evaluation) 是评估文本摘要质量的指标。
//!
//! - ROUGE-N: 基于 N-gram 共现的召回率
//! - ROUGE-L: 基于最长公共子序列 (LCS) 的 F-measure

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use serde_json::{json, Value};

use super::registry::{Metric, MetricOutput};
use crate::error::EvalError;

/// ROUGE 指标
pub struct RougeMetric;

#[async_trait]
impl Metric for RougeMetric {
    fn name(&self) -> &str {
        "rouge"
    }

    fn metric_type(&self) -> &str {
        "non_llm"
    }

    /// 参数 schema
    ///
    /// - `variant`: 变体，`"n"` (ROUGE-N) 或 `"l"` (ROUGE-L)，默认 `"n"`
    /// - `n`: N-gram 大小（仅 ROUGE-N 使用），默认 1
    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "variant".to_string(),
            json!({"type": "string", "description": "ROUGE 变体: n (ROUGE-N) 或 l (ROUGE-L)", "default": "n"}),
        );
        schema.insert(
            "n".to_string(),
            json!({"type": "integer", "description": "N-gram 大小（仅 ROUGE-N 使用）", "default": 1}),
        );
        schema
    }

    /// 执行 ROUGE 评测
    async fn evaluate(&self, params: &HashMap<String, Value>, input: &Value) -> Result<MetricOutput, EvalError> {
        let reference = input
            .get("reference")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 reference 字段".to_string()))?;
        let hypothesis = input
            .get("hypothesis")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 hypothesis 字段".to_string()))?;

        let variant = params
            .get("variant")
            .and_then(|v| v.as_str())
            .unwrap_or("n");

        match variant {
            "l" => Ok(compute_rouge_l(reference, hypothesis)),
            _ => {
                let n = params
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as usize;
                Ok(compute_rouge_n(reference, hypothesis, n))
            }
        }
    }
}

/// 计算 ROUGE-N（n-gram 召回率 + F1）
fn compute_rouge_n(reference: &str, hypothesis: &str, n: usize) -> MetricOutput {
    let ref_ngrams = get_ngrams(reference, n);
    let hyp_ngrams = get_ngrams(hypothesis, n);

    let overlap = ref_ngrams.intersection(&hyp_ngrams).count() as f64;

    let precision = if hyp_ngrams.is_empty() {
        0.0
    } else {
        overlap / hyp_ngrams.len() as f64
    };

    let recall = if ref_ngrams.is_empty() {
        0.0
    } else {
        overlap / ref_ngrams.len() as f64
    };

    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };

    MetricOutput {
        score: f1,
        details: json!({
            "precision": precision,
            "recall": recall,
            "f1": f1,
            "n": n,
        }),
    }
}

/// 计算 ROUGE-L（基于 LCS 的 F-measure）
fn compute_rouge_l(reference: &str, hypothesis: &str) -> MetricOutput {
    let ref_words: Vec<&str> = reference.split_whitespace().collect();
    let hyp_words: Vec<&str> = hypothesis.split_whitespace().collect();

    let lcs_len = lcs_length(&ref_words, &hyp_words) as f64;

    let precision = if hyp_words.is_empty() {
        0.0
    } else {
        lcs_len / hyp_words.len() as f64
    };

    let recall = if ref_words.is_empty() {
        0.0
    } else {
        lcs_len / ref_words.len() as f64
    };

    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };

    MetricOutput {
        score: f1,
        details: json!({
            "precision": precision,
            "recall": recall,
            "f1": f1,
            "lcs_length": lcs_len,
        }),
    }
}

/// 获取文本的 N-gram 集合
fn get_ngrams(text: &str, n: usize) -> HashSet<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < n {
        return HashSet::new();
    }
    words
        .windows(n)
        .map(|w| w.join(" "))
        .collect()
}

/// 计算最长公共子序列长度（动态规划，O(m*n)）
fn lcs_length(a: &[&str], b: &[&str]) -> usize {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    dp[m][n]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rouge_1_exact_match() {
        let metric = RougeMetric;
        let mut params = HashMap::new();
        params.insert("variant".to_string(), json!("n"));
        params.insert("n".to_string(), json!(1));
        let input = json!({
            "reference": "the cat sat on the mat",
            "hypothesis": "the cat sat on the mat",
        });
        let result = metric.evaluate(&params, &input).await.unwrap();
        assert!((result.score - 1.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_rouge_1_no_match() {
        let metric = RougeMetric;
        let mut params = HashMap::new();
        params.insert("variant".to_string(), json!("n"));
        params.insert("n".to_string(), json!(1));
        let input = json!({
            "reference": "the cat sat on the mat",
            "hypothesis": "dogs run fast",
        });
        let result = metric.evaluate(&params, &input).await.unwrap();
        assert!((result.score - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_rouge_l() {
        let metric = RougeMetric;
        let mut params = HashMap::new();
        params.insert("variant".to_string(), json!("l"));
        let input = json!({
            "reference": "the cat sat on the mat",
            "hypothesis": "the cat sat",
        });
        let result = metric.evaluate(&params, &input).await.unwrap();
        assert!(result.score > 0.0);
        assert!(result.score < 1.0);
    }
}