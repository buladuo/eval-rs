//! 配置模块 — 配置加载与校验
//!
//! 支持从 `config/default.toml` 加载配置，支持环境变量覆盖（`EVAL_` 前缀）。
//!
//! # 环境变量覆盖
//!
//! 例如 `EVAL_SERVER__PORT=9090` 可覆盖 `server.port`。使用双重下划线 `__`
//! 分隔嵌套字段。

use serde::Deserialize;
use std::collections::HashMap;

use crate::error::EvalError;

/// 全局应用配置
///
/// 从 TOML 文件加载，支持环境变量覆盖。所有子配置均有默认值。
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// HTTP 服务配置
    #[serde(default)]
    pub server: ServerConfig,
    /// 日志配置
    #[serde(default)]
    pub log: LogConfig,
    /// LLM provider 配置（key=provider名）
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    /// 限流配置
    #[serde(default)]
    pub limits: LimitsConfig,
    /// 提示词配置
    #[serde(default)]
    pub prompts: PromptsConfig,
    /// 评测配置
    #[serde(default)]
    pub eval: EvalConfig,
    /// 存储配置（SQLite 持久化）
    #[serde(default)]
    pub storage: StorageConfig,
}

/// HTTP 服务配置
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// 监听地址，默认 `"0.0.0.0"`
    #[serde(default = "default_host")]
    pub host: String,
    /// 监听端口，默认 `8080`
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

/// 日志配置
#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    /// 日志级别: `"error"` | `"warn"` | `"info"` | `"debug"` | `"trace"`
    #[serde(default = "default_log_level")]
    pub level: String,
    /// 日志格式: `"text"` / `"pretty"`（可读文本）或 `"json"` / `"jsonl"`（每行一条 JSON）
    #[serde(default = "default_log_format")]
    pub format: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_log_format() -> String {
    "text".to_string()
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            format: default_log_format(),
        }
    }
}

/// LLM Provider 配置
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Provider 类型: `"openai"` | `"anthropic"`
    pub provider_type: String,
    /// API 基础 URL（OpenAI 兼容服务使用）
    #[serde(default)]
    pub base_url: Option<String>,
    /// API 密钥（建议通过环境变量设置 `EVAL_PROVIDERS__XX__API_KEY`）
    #[serde(default)]
    pub api_key: Option<String>,
    /// 模型名称
    pub model: String,
    /// 是否为默认 provider
    #[serde(default)]
    pub default: bool,
}

/// 限流配置
#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    /// 最大并发请求数，默认 10
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// 每分钟最大请求数，默认 60
    #[serde(default = "default_rpm")]
    pub requests_per_minute: u64,
    /// 每分钟最大 token 数（可选）
    #[serde(default)]
    pub tokens_per_minute: Option<u64>,
    /// 每秒最大 token 数（可选）
    #[serde(default)]
    pub tokens_per_second: Option<u64>,
}

fn default_max_concurrency() -> usize {
    10
}

fn default_rpm() -> u64 {
    60
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            max_concurrency: default_max_concurrency(),
            requests_per_minute: default_rpm(),
            tokens_per_minute: None,
            tokens_per_second: None,
        }
    }
}

/// 提示词配置
#[derive(Debug, Clone, Deserialize)]
pub struct PromptsConfig {
    /// 提示词文件目录，默认 `"prompts"`
    #[serde(default = "default_prompts_dir")]
    pub dir: String,
}

fn default_prompts_dir() -> String {
    "prompts".to_string()
}

impl Default for PromptsConfig {
    fn default() -> Self {
        Self {
            dir: default_prompts_dir(),
        }
    }
}

/// 评测配置
#[derive(Debug, Clone, Deserialize)]
pub struct EvalConfig {
    /// 评测超时时间（秒），默认 120
    #[serde(default = "default_eval_timeout")]
    pub timeout_secs: u64,
}

fn default_eval_timeout() -> u64 {
    120
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_eval_timeout(),
        }
    }
}

/// 存储配置（SQLite 持久化）
#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    /// 是否启用存储，默认 true
    #[serde(default = "default_storage_enabled")]
    pub enabled: bool,
    /// SQLite 数据库文件路径，默认 `"data/eval.db"`
    #[serde(default = "default_db_path")]
    pub db_path: String,
}

fn default_storage_enabled() -> bool {
    true
}

fn default_db_path() -> String {
    "data/eval.db".to_string()
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            enabled: default_storage_enabled(),
            db_path: default_db_path(),
        }
    }
}

/// 加载配置
///
/// 优先级：`.env` 文件 > `config/default.toml` > Rust 默认值。
///
/// `.env` 文件中的环境变量格式：`EVAL__<section>__<field>=<value>`，
/// 双下划线 `__` 表示嵌套层级。
///
/// # Errors
///
/// 配置文件解析失败返回 [`EvalError::ConfigError`]。
pub fn load_config() -> Result<AppConfig, EvalError> {
    // 加载 .env 文件（不存在不报错，已存在的环境变量不会被覆盖）
    let _ = dotenvy::dotenv();

    let config_path =
        std::env::var("EVAL_CONFIG_PATH").unwrap_or_else(|_| "config/default.toml".to_string());

    let mut cfg = ::config::Config::builder();

    // 加载默认配置文件（不存在时不报错）
    cfg = cfg
        .add_source(::config::File::with_name(&config_path.replace(".toml", "")).required(false));

    // 环境变量覆盖（.env 通过 dotenvy 加载后会出现在系统环境变量中）
    cfg = cfg.add_source(::config::Environment::with_prefix("EVAL_").separator("__"));

    let settings = cfg
        .build()
        .map_err(|e| EvalError::ConfigError(e.to_string()))?;

    let app_config: AppConfig = settings
        .try_deserialize()
        .map_err(|e| EvalError::ConfigError(e.to_string()))?;

    Ok(app_config)
}
