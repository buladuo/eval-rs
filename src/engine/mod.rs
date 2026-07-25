//! 评测引擎 — 评测流程编排与生命周期管理
//!
//! 负责接收评测请求、路由到对应指标、编排执行流程并返回结果。
//!
//! # 模块结构
//!
//! - [`EvalEngine`] — 引擎主体，协调所有模块完成评测
//! - [`EvalRequest`] / [`EvalResult`] / [`MetricInfo`] / [`RetryConfig`] — 核心数据结构
//! - [`orchestrator`] — 单次评测编排
//! - [`router`] — 指标路由
//!
//! # 存储写入背压
//!
//! 启用存储后，评测结果通过有界 channel（容量 [`STORAGE_QUEUE_CAPACITY`]）异步写入 SQLite。
//! 当队列满时，新的写入任务将被丢弃并记录警告日志，防止内存无限增长。

pub mod orchestrator;
pub mod router;

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::EvalError;
use crate::metrics::registry::MetricRegistry;
use crate::preprocessor::PreprocessConfig;
use crate::prompts::registry::PromptRegistry;
use crate::provider::manager::ProviderManager;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

/// 存储写入队列容量（背压上限）
///
/// 当队列满时，新的写入任务将被丢弃并记录警告日志。
pub const STORAGE_QUEUE_CAPACITY: usize = 1024;

/// 评测引擎，协调所有模块完成评测任务
pub struct EvalEngine {
    /// 指标注册表
    pub metrics: Arc<MetricRegistry>,
    /// LLM provider 管理器
    pub providers: Arc<ProviderManager>,
    /// 提示词注册表
    pub prompts: Arc<PromptRegistry>,
    /// 评测超时时间（秒）
    pub eval_timeout_secs: u64,
    /// 评测结果存储（可选，未启用时为 None）
    pub storage: Option<Arc<crate::storage::SqliteStore>>,
    /// 存储写入队列发送端（启用存储后由 [`with_storage`] 初始化）
    storage_tx: Option<mpsc::Sender<StorageWriteTask>>,
}

/// 存储写入任务
struct StorageWriteTask {
    /// 评测请求（用于提取 provider/model/input/params）
    request: EvalRequest,
    /// 评测结果
    result: EvalResult,
}

impl EvalEngine {
    /// 创建新的评测引擎
    ///
    /// # Arguments
    ///
    /// * `metrics` - 指标注册表
    /// * `providers` - Provider 管理器
    /// * `prompts` - 提示词注册表
    /// * `eval_timeout_secs` - 评测超时（秒）
    pub fn new(
        metrics: Arc<MetricRegistry>,
        providers: Arc<ProviderManager>,
        prompts: Arc<PromptRegistry>,
        eval_timeout_secs: u64,
    ) -> Self {
        Self {
            metrics,
            providers,
            prompts,
            eval_timeout_secs,
            storage: None,
            storage_tx: None,
        }
    }

    /// 启用存储层
    ///
    /// 启动后台 worker 异步消费评测结果并写入 SQLite。
    /// 写入队列容量为 [`STORAGE_QUEUE_CAPACITY`]，队列满时丢弃新任务并记录警告。
    ///
    /// # Arguments
    ///
    /// * `store` - 已初始化的 SQLite 存储
    pub fn with_storage(mut self, store: Arc<crate::storage::SqliteStore>) -> Self {
        let (tx, mut rx) = mpsc::channel::<StorageWriteTask>(STORAGE_QUEUE_CAPACITY);

        // 启动后台 worker 消费写入任务
        let store_clone = store.clone();
        tokio::spawn(async move {
            tracing::info!(capacity = STORAGE_QUEUE_CAPACITY, "存储写入 worker 已启动");
            while let Some(task) = rx.recv().await {
                let provider_name = task.request.provider.as_deref();
                let model_name = task.request.params.get("model").and_then(|v| v.as_str());
                let params_json = serde_json::to_value(&task.request.params).unwrap_or(Value::Null);
                if let Err(e) = store_clone
                    .insert(
                        &task.result.request_id,
                        &task.result.metric,
                        task.result.score,
                        &task.result.details,
                        provider_name,
                        model_name,
                        &task.request.input,
                        &params_json,
                    )
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        request_id = %task.result.request_id,
                        "写入评测结果到存储失败"
                    );
                }
            }
            tracing::info!("存储写入 worker 已退出");
        });

        self.storage = Some(store);
        self.storage_tx = Some(tx);
        self
    }

    /// 执行单次评测
    ///
    /// 流程：参数校验 → 输入预处理 → 指标执行 → 可选存储写入（背压）。
    ///
    /// # Errors
    ///
    /// - 指标未找到、参数无效、评测超时等
    pub async fn evaluate(&self, request: EvalRequest) -> Result<EvalResult, EvalError> {
        let _span = tracing::info_span!("evaluate", metric = %request.metric, request_id = %request.request_id);
        let result = orchestrator::run_evaluation(self, request.clone()).await?;

        // 评测成功后将写入任务投递到有界 channel，由后台 worker 异步消费
        if let Some(tx) = &self.storage_tx {
            let task = StorageWriteTask {
                request,
                result: result.clone(),
            };
            if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(task) {
                tracing::warn!(
                    request_id = %result.request_id,
                    "存储写入队列已满（容量 {}），丢弃本次写入任务",
                    STORAGE_QUEUE_CAPACITY
                );
            }
        }

        Ok(result)
    }

    /// 获取所有已注册指标的元信息
    pub fn list_metrics(&self) -> Vec<MetricInfo> {
        self.metrics
            .list()
            .into_iter()
            .map(|m| MetricInfo {
                name: m.name().to_string(),
                metric_type: m.metric_type().to_string(),
                params_schema: m.params_schema().clone(),
            })
            .collect()
    }

    /// 检查是否注册了至少一个 LLM provider（用于健康检查）
    ///
    /// # Returns
    ///
    /// `true` 当且仅当 `ProviderManager` 中至少有一个 provider。
    pub fn has_any_provider(&self) -> bool {
        self.providers.has_any_provider()
    }
}

/// 评测请求
///
/// 由 HTTP 层解析后传入引擎。
#[derive(Debug, Clone, Deserialize)]
pub struct EvalRequest {
    /// 指标名称（如 `"rouge"`、`"llm_judge_accuracy"`）
    pub metric: String,
    /// 指标参数
    #[serde(default)]
    pub params: HashMap<String, Value>,
    /// 评测输入数据（结构由指标定义）
    pub input: Value,
    /// 输入预处理配置（可选），见 [`PreprocessConfig`]
    #[serde(default)]
    pub extract: Option<PreprocessConfig>,
    /// 重试配置（可选），覆盖全局默认
    #[serde(default)]
    pub retry: Option<RetryConfig>,
    /// 使用的 LLM provider 名称（可选，默认使用配置中的默认 provider）
    #[serde(default)]
    pub provider: Option<String>,
    /// 请求 ID（可选，未提供则自动生成 UUIDv4）
    #[serde(default = "default_request_id")]
    pub request_id: String,
}

/// 生成默认请求 ID（UUIDv4）
fn default_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 重试配置
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// 初始退避时间（毫秒），后续按 2^n 指数增长
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,
}

fn default_max_attempts() -> u32 {
    3
}

fn default_backoff_ms() -> u64 {
    1000
}

/// 评测结果
#[derive(Debug, Clone, Serialize)]
pub struct EvalResult {
    /// 指标名称
    pub metric: String,
    /// 评分
    pub score: f64,
    /// 详细信息（JSON，由指标自定义填充）
    pub details: Value,
    /// 请求 ID（透传）
    pub request_id: String,
}

/// 指标元信息
///
/// 用于 `GET /v1/metrics` 接口返回。
#[derive(Debug, Serialize)]
pub struct MetricInfo {
    /// 指标名称
    pub name: String,
    /// 指标类型（`"llm"` 或 `"non_llm"`）
    pub metric_type: String,
    /// 参数 schema（JSON 描述）
    pub params_schema: HashMap<String, Value>,
}
