//! Answer Accuracy（答案准确性）指标
//!
//! 评估模型生成答案与参考答案之间的事实一致性，
//! 基于 TP/FP/FN 三分类 + F1 加权语义相似度。
//!
//! # 流程（3 步 LLM + 数学聚合）
//!
//! 1. 将 answer 分解为独立事实声明（claims）
//! 2. 将 reference 分解为独立事实声明（reference_claims）
//! 3. 逐对匹配：answer 的每个 claim 是否在 reference_claims 中有对应
//!    - TP（真阳性）：answer 中有、reference 中也有的声明
//!    - FP（假阳性）：answer 中有、reference 中没有的声明（幻觉/过度生成）
//!    - FN（假阴性）：reference 中有、answer 中没有的声明（遗漏）
//! 4. 计算 F1 分数：F1 = |TP| / (|TP| + 0.5 × (|FP| + |FN|))
//!
//! # 输入字段
//!
//! - `question` — 用户问题（可选，提供上下文帮助 LLM 判断）
//! - `reference` — 参考答案（ground truth）
//! - `answer` — 模型生成的答案
//!
//! # 说明
//!
//! 与 [`super::faithfulness`] 的区别：faithfulness 检查 answer 是否忠实于 context，
//! answer_accuracy 检查 answer 是否与 reference（正确答案）一致。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::super::registry::{Metric, MetricOutput};
use super::{llm_complete, parse_json, parse_verdict};
use crate::error::EvalError;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;

/// Answer Accuracy 指标
pub struct AnswerAccuracy {
    providers: Arc<ProviderManager>,
    prompts: Arc<PromptRegistry>,
}

impl AnswerAccuracy {
    pub fn new(providers: Arc<ProviderManager>, prompts: Arc<PromptRegistry>) -> Self {
        Self { providers, prompts }
    }
}

#[async_trait]
impl Metric for AnswerAccuracy {
    fn name(&self) -> &str {
        "answer_accuracy"
    }

    fn metric_type(&self) -> &str {
        "llm"
    }

    fn params_schema(&self) -> HashMap<String, Value> {
        let mut schema = HashMap::new();
        schema.insert(
            "provider".to_string(),
            json!({"type": "string", "description": "LLM provider 名称", "required": false}),
        );
        schema.insert(
            "model".to_string(),
            json!({"type": "string", "description": "模型名称", "required": false}),
        );
        schema.insert(
            "prompt_version".to_string(),
            json!({"type": "string", "description": "提示词版本", "required": false}),
        );
        schema
    }

    async fn evaluate(
        &self,
        params: &HashMap<String, Value>,
        input: &Value,
    ) -> Result<MetricOutput, EvalError> {
        let provider = params.get("provider").and_then(|v| v.as_str());
        let model = params.get("model").and_then(|v| v.as_str());
        let version = params.get("prompt_version").and_then(|v| v.as_str());

        let question = input.get("question").and_then(|v| v.as_str()).unwrap_or("");
        let reference = input
            .get("reference")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 reference 字段".into()))?;
        let answer = input
            .get("answer")
            .and_then(|v| v.as_str())
            .ok_or_else(|| EvalError::InvalidParams("缺少 answer 字段".into()))?;

        // Step 1: 从 answer 中分解出声明
        let mut vars = tera::Context::new();
        vars.insert("answer", answer);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "answer_accuracy_extract_claims",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let answer_claims: Vec<String> = parsed["claims"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("claims 不是数组 (answer)".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        // Step 2: 从 reference 中分解出声明
        let mut vars = tera::Context::new();
        vars.insert("reference", reference);
        let resp = llm_complete(
            &self.providers,
            &self.prompts,
            "answer_accuracy_extract_reference_claims",
            vars,
            provider,
            model,
            version,
        )
        .await?;
        let parsed = parse_json(&resp)?;
        let ref_claims: Vec<String> = parsed["claims"]
            .as_array()
            .ok_or_else(|| EvalError::JudgeParseFailed("claims 不是数组 (reference)".into()))?
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();

        // Step 3: 逐对匹配 answer_claims → reference_claims，计算 TP/FP/FN
        let ref_text = ref_claims.join("\n");
        let mut tp = 0usize;
        let mut fp = 0usize;
        let mut matched_pairs = Vec::new();

        for claim in &answer_claims {
            let mut vars = tera::Context::new();
            vars.insert("reference_claims", &ref_text);
            vars.insert("question", question);
            vars.insert("claim", claim);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "answer_accuracy_match_claim",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let matched = parse_verdict(&parsed, &["match", "verdict"])?;
            if matched {
                tp += 1;
            } else {
                fp += 1;
            }
            matched_pairs.push(json!({
                "claim": claim,
                "in_reference": matched,
            }));
        }

        // FN：reference 中有但 answer 中没有的声明
        // 让 LLM 判断每个 ref_claim 是否被 answer 覆盖
        let answer_text = answer_claims.join("\n");
        let mut fn_count = 0usize;
        let mut fn_pairs = Vec::new();

        for ref_claim in &ref_claims {
            let mut vars = tera::Context::new();
            vars.insert("answer_claims", &answer_text);
            vars.insert("claim", ref_claim);
            let resp = llm_complete(
                &self.providers,
                &self.prompts,
                "answer_accuracy_match_ref_claim",
                vars,
                provider,
                model,
                version,
            )
            .await?;
            let parsed = parse_json(&resp)?;
            let covered = parse_verdict(&parsed, &["covered", "verdict"])?;
            if !covered {
                fn_count += 1;
            }
            fn_pairs.push(json!({
                "reference_claim": ref_claim,
                "covered_by_answer": covered,
            }));
        }

        // Step 4: 计算 F1 分数
        // F1 = |TP| / (|TP| + 0.5 × (|FP| + |FN|))
        let denominator = tp as f64 + 0.5 * ((fp + fn_count) as f64);
        let score = if denominator > 0.0 {
            tp as f64 / denominator
        } else {
            0.0
        };

        Ok(MetricOutput {
            score,
            details: json!({
                "question": question,
                "reference": reference,
                "answer": answer,
                "answer_claims": answer_claims,
                "reference_claims": ref_claims,
                "tp": tp,
                "fp": fp,
                "fn": fn_count,
                "matched_pairs": matched_pairs,
                "fn_pairs": fn_pairs,
                "f1_score": score,
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_perfect_match() {
        // TP=3, FP=0, FN=0 → F1 = 3/(3+0) = 1.0
        let tp: f64 = 3.0;
        let fp: f64 = 0.0;
        let f_n: f64 = 0.0;
        let score = tp / (tp + 0.5 * (fp + f_n));
        assert!((score - 1.0_f64).abs() < 0.001);
    }

    #[test]
    fn test_partial_match() {
        // TP=2, FP=1, FN=1 → F1 = 2/(2+0.5*2) = 2/3 ≈ 0.667
        let tp: f64 = 2.0;
        let fp: f64 = 1.0;
        let f_n: f64 = 1.0;
        let score = tp / (tp + 0.5 * (fp + f_n));
        assert!((score - 0.667_f64).abs() < 0.01);
    }

    #[test]
    fn test_all_wrong() {
        // TP=0, FP=3, FN=3 → F1 = 0/(0+0.5*6) = 0.0
        let tp: f64 = 0.0;
        let fp: f64 = 3.0;
        let f_n: f64 = 3.0;
        let score = tp / (tp + 0.5 * (fp + f_n));
        assert!((score - 0.0_f64).abs() < 0.001);
    }

    #[test]
    fn test_hallucination_penalty() {
        // TP=1, FP=2, FN=0 → F1 = 1/(1+0.5*2) = 1/2 = 0.5
        let tp: f64 = 1.0;
        let fp: f64 = 2.0;
        let f_n: f64 = 0.0;
        let score = tp / (tp + 0.5 * (fp + f_n));
        assert!((score - 0.5_f64).abs() < 0.001);
    }

    #[test]
    fn test_missing_penalty() {
        // TP=1, FP=0, FN=2 → F1 = 1/(1+0.5*2) = 1/2 = 0.5
        let tp: f64 = 1.0;
        let fp: f64 = 0.0;
        let f_n: f64 = 2.0;
        let score = tp / (tp + 0.5 * (fp + f_n));
        assert!((score - 0.5_f64).abs() < 0.001);
    }
}
