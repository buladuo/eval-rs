//! 多步 LLM-as-Judge 指标演示
//!
//! 展示 8 个 judge 指标的输入格式、多步评估流程与输出结构。
//!
//! 运行（需要配置 LLM provider 并设置 API Key）：
//! ```sh
//! EVAL_PROVIDERS__GLM__API_KEY=your-key \
//!   cargo run --example judge_metrics
//! ```
//!
//! 如果未配置 provider，示例将跳过实际的 LLM 调用，仅打印指标定义与输入格式。

use std::collections::HashMap;
use std::sync::Arc;

use eval_rs::error::EvalError;
use eval_rs::metrics::judge::{
    AnswerAccuracy, AnswerRelevancy, ContextEntitiesRecall, ContextPrecision, ContextRecall,
    ContextRelevancy, Faithfulness, NoiseSensitivity, SummarizationScore,
};
use eval_rs::metrics::registry::MetricRegistry;
use eval_rs::prompts::registry::PromptRegistry;
use eval_rs::provider::manager::ProviderManager;
use eval_rs::settings::{AppConfig, load_config};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // ── 加载配置与初始化 ──
    let config = load_config().unwrap_or_else(|_| AppConfig {
        server: Default::default(),
        log: Default::default(),
        providers: HashMap::new(),
        limits: Default::default(),
        prompts: eval_rs::settings::PromptsConfig {
            dir: "prompts".into(),
        },
        eval: Default::default(),
        storage: Default::default(),
    });

    let provider_manager = Arc::new(ProviderManager::new(
        config.providers.clone(),
        &config.limits,
    ));

    let mut prompts = PromptRegistry::new();
    prompts.load_from_dir(&config.prompts.dir)?;
    let prompts = Arc::new(prompts);

    let mut registry = MetricRegistry::new();
    registry.register(Box::new(Faithfulness::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(AnswerRelevancy::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(ContextPrecision::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(ContextRecall::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(ContextRelevancy::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(ContextEntitiesRecall::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(NoiseSensitivity::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(AnswerAccuracy::new(
        provider_manager.clone(),
        prompts.clone(),
    )));
    registry.register(Box::new(SummarizationScore::new(
        provider_manager.clone(),
        prompts.clone(),
    )));

    let has_provider = config
        .providers
        .values()
        .any(|p| p.api_key.as_deref().is_some_and(|k| !k.is_empty()));

    if has_provider {
        run_all_metrics(&registry).await;
    } else {
        print_metrics_info(&registry);
    }

    Ok(())
}

async fn run_all_metrics(registry: &MetricRegistry) {
    let params = HashMap::new();

    println!("=== 多步 LLM-as-Judge 指标演示 ===\n");

    // ── 1. Faithfulness ──
    println!("── 1. faithfulness ──");
    println!("流程: ①分解answer为claims → ②逐claim验证能否从context推断 → ③支持数/总数");
    let input = json!({
        "question": "爱因斯坦是哪年在哪里出生的？",
        "context": "Albert Einstein (born 14 March 1879) was a German-born theoretical physicist.",
        "answer": "Einstein was born in Germany on 14th March 1879."
    });
    match evaluate(registry, "faithfulness", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 2. Answer Relevancy ──
    println!("── 2. answer_relevancy ──");
    println!("流程: ①从answer反向生成N个问题 → ②LLM判断每对是否语义等价 → ③匹配数/N");
    let input = json!({
        "question": "法国的首都是哪里？",
        "answer": "法国的首都是巴黎，它是欧洲重要的文化和政治中心。",
        "n": 3
    });
    match evaluate(registry, "answer_relevancy", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 3. Context Precision ──
    println!("── 3. context_precision ──");
    println!("流程: ①逐chunk二值判定相关 → ②位置加权Precision@K");
    let input = json!({
        "question": "什么是机器学习？",
        "contexts": [
            "机器学习是人工智能的一个分支，专注于从数据中学习模式。",
            "苹果是一种常见的水果，富含维生素C。",
            "深度学习是机器学习的一个子领域，使用多层神经网络。",
            "巴黎是法国的首都。"
        ],
        "reference": "机器学习是人工智能的一个分支，使用算法从数据中学习模式。深度学习是机器学习的子领域。"
    });
    match evaluate(registry, "context_precision", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 4. Context Recall ──
    println!("── 4. context_recall ──");
    println!("流程: ①reference拆claims → ②逐claim验证能否从contexts支持 → ③支持数/总数");
    let input = json!({
        "question": "法国的首都是哪里？",
        "reference": "法国的首都是巴黎，巴黎以埃菲尔铁塔和卢浮宫闻名。",
        "contexts": [
            "巴黎是法国的首都，位于塞纳河畔。",
            "埃菲尔铁塔是巴黎的地标建筑。"
        ]
    });
    match evaluate(registry, "context_recall", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 5. Context Relevancy ──
    println!("── 5. context_relevancy ──");
    println!("流程: ①逐chunk二值判定 → ②相关数/总数（无位置加权）");
    let input = json!({
        "question": "什么是检索增强生成？",
        "contexts": [
            "RAG（检索增强生成）结合了信息检索和文本生成。",
            "今天天气晴朗，适合户外活动。",
            "RAG可以有效减少大语言模型的幻觉问题。"
        ]
    });
    match evaluate(registry, "context_relevancy", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 6. Context Entities Recall ──
    println!("── 6. context_entities_recall ──");
    println!("流程: ①从reference抽实体RE → ②从contexts抽实体RCE → ③|RCE∩RE|/|RE|");
    let input = json!({
        "reference": "阿尔伯特·爱因斯坦于1879年出生在德国乌尔姆，以相对论闻名。",
        "contexts": [
            "爱因斯坦提出了相对论，并于1921年获得诺贝尔物理学奖。",
            "他于1879年3月14日出生。"
        ]
    });
    match evaluate(registry, "context_entities_recall", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 7. Noise Sensitivity ──
    println!("── 7. noise_sensitivity ──");
    println!(
        "流程: ①response拆claims → ②逐claim验证是否与ground_truth一致 → ③错误数/总数（越低越好）"
    );
    let input = json!({
        "question": "印度人寿保险公司以什么闻名？",
        "reference": "印度人寿保险公司是印度最大的保险公司，成立于1956年，以管理庞大的投资组合而闻名。",
        "contexts": [
            "印度人寿保险公司成立于1956年，是在印度保险业国有化之后成立的。",
            "LIC是印度最大的保险公司，拥有庞大的投保人网络。",
            "作为印度最大的机构投资者，LIC管理着庞大的基金。",
            "印度经济是世界上增长最快的主要经济体之一。"
        ],
        "answer": "印度人寿保险公司是印度最大的保险公司，以其庞大的投资组合而闻名。LIC为国家的金融稳定做出了贡献。"
    });
    match evaluate(registry, "noise_sensitivity", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }

    // ── 8. Summarization Score ──
    println!("── 8. summarization_score ──");
    println!("流程: ①抽keyphrases → ②生成问题 → ③用问题询问summary → ④正确数/总数");
    let input = json!({
        "reference": "一家公司正在推出一款新的智能手机应用，该应用旨在帮助用户追踪他们的健身目标。这款应用支持记录步数、卡路里消耗和心率监测，并将在下个月上线。",
        "summary": "一家公司即将推出健身追踪应用，可以记录运动数据。"
    });
    match evaluate(registry, "summarization_score", &params, &input).await {
        Ok(out) => {
            println!("score: {:.4}", out.score);
            println!(
                "details: {}\n",
                serde_json::to_string_pretty(&out.details).unwrap()
            );
        }
        Err(e) => println!("错误: {e}\n"),
    }
}

fn print_metrics_info(registry: &MetricRegistry) {
    println!("=== LLM-as-Judge 指标定义（未配置 Provider，仅展示信息）===\n");

    for m in registry.list() {
        println!("── {} ──", m.name());
        println!("类型: {}", m.metric_type());
        println!("参数: {:#?}", m.params_schema());
        println!("输入示例: {}", example_input_for(m.name()));
        println!();
    }
}

fn example_input_for(name: &str) -> String {
    match name {
        "faithfulness" => json!({
            "question": "爱因斯坦是哪年出生的？",
            "context": "Albert Einstein was born on 14 March 1879 in Germany.",
            "answer": "Einstein was born in Germany on 14th March 1879."
        })
        .to_string(),
        "answer_relevancy" => json!({
            "question": "法国的首都是哪里？",
            "answer": "法国的首都是巴黎。",
            "n": 3
        })
        .to_string(),
        "context_precision" | "context_relevancy" => json!({
            "question": "什么是机器学习？",
            "contexts": ["机器学习是AI的分支。", "苹果是一种水果。"],
            "reference": "机器学习是人工智能的一个分支。"
        })
        .to_string(),
        "context_recall" => json!({
            "question": "...",
            "reference": "法国的首都是巴黎。",
            "contexts": ["巴黎是法国的首都。"]
        })
        .to_string(),
        "context_entities_recall" => json!({
            "reference": "爱因斯坦于1879年出生在德国。",
            "contexts": ["爱因斯坦提出了相对论。"]
        })
        .to_string(),
        "noise_sensitivity" => json!({
            "question": "...",
            "reference": "正确答案...",
            "contexts": ["相关上下文1", "噪声上下文"],
            "answer": "模型回答..."
        })
        .to_string(),
        "summarization_score" => json!({
            "reference": "原文内容...",
            "summary": "摘要内容..."
        })
        .to_string(),
        "answer_accuracy" => json!({
            "question": "Rust的特点是什么？",
            "reference": "Rust是一种注重内存安全和并发安全的系统编程语言，支持零成本抽象。",
            "answer": "Rust是一种安全的编程语言，性能优异。"
        })
        .to_string(),
        _ => "{}".to_string(),
    }
}

async fn evaluate(
    registry: &MetricRegistry,
    name: &str,
    params: &HashMap<String, serde_json::Value>,
    input: &serde_json::Value,
) -> Result<eval_rs::metrics::registry::MetricOutput, EvalError> {
    let metric = registry
        .get(name)
        .ok_or_else(|| EvalError::MetricNotFound(name.into()))?;
    metric.evaluate(params, input).await
}
