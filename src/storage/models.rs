//! 存储数据模型 — 评测结果行与查询参数

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 评测结果在数据库中的行结构
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EvalResultRow {
    /// 自增主键
    pub id: i64,
    /// 请求 ID
    pub request_id: String,
    /// 指标名称
    pub metric: String,
    /// 评分
    pub score: f64,
    /// 详细信息（JSON）
    pub details: Value,
    /// 使用的 provider 名称
    pub provider: Option<String>,
    /// 使用的模型名称
    pub model: Option<String>,
    /// 评测输入（JSON）
    pub input: Value,
    /// 指标参数（JSON）
    pub params: Value,
    /// 创建时间（UTC）
    pub created_at: DateTime<Utc>,
}

/// 查询参数
#[derive(Debug, Clone, Deserialize)]
pub struct QueryParams {
    /// 指标名称过滤（精确匹配）
    pub metric: Option<String>,
    /// 请求 ID 过滤（精确匹配）
    pub request_id: Option<String>,
    /// 最小评分
    pub min_score: Option<f64>,
    /// 最大评分
    pub max_score: Option<f64>,
    /// 开始时间（ISO 8601）
    pub start_time: Option<String>,
    /// 结束时间（ISO 8601）
    pub end_time: Option<String>,
    /// 使用的 provider 过滤
    pub provider: Option<String>,
    /// 使用的模型过滤
    pub model: Option<String>,
    /// 分页偏移量
    #[serde(default)]
    pub offset: u64,
    /// 分页大小（默认 20，最大 100）
    #[serde(default = "default_limit")]
    pub limit: u64,
}

impl Default for QueryParams {
    fn default() -> Self {
        Self {
            metric: None,
            request_id: None,
            min_score: None,
            max_score: None,
            start_time: None,
            end_time: None,
            provider: None,
            model: None,
            offset: 0,
            limit: default_limit(),
        }
    }
}

fn default_limit() -> u64 {
    20
}

/// 聚合统计结果
#[derive(Debug, Clone, Serialize)]
pub struct AggregationResult {
    /// 指标名称
    pub metric: String,
    /// 记录数
    pub count: i64,
    /// 平均分
    pub avg_score: Option<f64>,
    /// 最高分
    pub max_score: Option<f64>,
    /// 最低分
    pub min_score: Option<f64>,
    /// 标准差
    pub stddev: Option<f64>,
}
