//! SQLite 存储实现 — 基于 sqlx 的评测结果持久化

use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::error::EvalError;

use super::models::{AggregationResult, EvalResultRow, QueryParams};

/// SQLite 存储句柄
#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// 创建并打开数据库，自动建表
    pub async fn open(db_path: &str) -> Result<Self, EvalError> {
        // 确保父目录存在
        if let Some(parent) = std::path::Path::new(db_path).parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| EvalError::Internal(format!("创建数据库目录失败: {e}")))?;
        }

        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{db_path}"))
            .map_err(|e| EvalError::Internal(format!("SQLite 连接参数错误: {e}")))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await
            .map_err(|e| EvalError::Internal(format!("SQLite 连接失败: {e}")))?;

        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    /// 健康检查 — 验证数据库连通性
    ///
    /// 执行 `SELECT 1` 验证连接池可用。失败时返回错误。
    ///
    /// # Errors
    ///
    /// 返回 [`EvalError::Internal`] 当数据库连接失败。
    pub async fn health_check(&self) -> Result<(), EvalError> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| EvalError::Internal(format!("SQLite 健康检查失败: {e}")))?;
        Ok(())
    }

    /// 数据库迁移（建表）
    async fn migrate(&self) -> Result<(), EvalError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS eval_results (
                id              INTEGER PRIMARY KEY AUTOINCREMENT,
                request_id      TEXT    NOT NULL,
                metric          TEXT    NOT NULL,
                score           REAL    NOT NULL,
                details         TEXT    NOT NULL,
                provider        TEXT,
                model           TEXT,
                input           TEXT    NOT NULL,
                params          TEXT    NOT NULL,
                created_at      TEXT    NOT NULL
            );
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| EvalError::Internal(format!("建表失败: {e}")))?;

        // 索引：加速常用查询维度
        for sql in [
            "CREATE INDEX IF NOT EXISTS idx_eval_metric     ON eval_results(metric);",
            "CREATE INDEX IF NOT EXISTS idx_eval_request_id ON eval_results(request_id);",
            "CREATE INDEX IF NOT EXISTS idx_eval_created_at ON eval_results(created_at);",
            "CREATE INDEX IF NOT EXISTS idx_eval_provider   ON eval_results(provider);",
            "CREATE INDEX IF NOT EXISTS idx_eval_model      ON eval_results(model);",
            "CREATE INDEX IF NOT EXISTS idx_eval_score      ON eval_results(score);",
        ] {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| EvalError::Internal(format!("创建索引失败: {e}")))?;
        }
        Ok(())
    }

    /// 写入一条评测结果
    ///
    /// # Arguments
    ///
    /// * `request_id` - 请求 ID
    /// * `metric` - 指标名称
    /// * `score` - 评分
    /// * `details` - 评分详情（JSON）
    /// * `provider` - 提供方名称（可选）
    /// * `model` - 模型名称（可选）
    /// * `input` - 输入数据（JSON）
    /// * `params` - 评测参数（JSON）
    #[allow(
        clippy::too_many_arguments,
        reason = "数据库写入接口，参数均为必需字段"
    )]
    pub async fn insert(
        &self,
        request_id: &str,
        metric: &str,
        score: f64,
        details: &Value,
        provider: Option<&str>,
        model: Option<&str>,
        input: &Value,
        params: &Value,
    ) -> Result<i64, EvalError> {
        let now = Utc::now().to_rfc3339();
        let details_str = serde_json::to_string(details)
            .map_err(|e| EvalError::Internal(format!("序列化 details 失败: {e}")))?;
        let input_str = serde_json::to_string(input)
            .map_err(|e| EvalError::Internal(format!("序列化 input 失败: {e}")))?;
        let params_str = serde_json::to_string(params)
            .map_err(|e| EvalError::Internal(format!("序列化 params 失败: {e}")))?;

        let result = sqlx::query(
            r#"
            INSERT INTO eval_results
                (request_id, metric, score, details, provider, model, input, params, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?);
            "#,
        )
        .bind(request_id)
        .bind(metric)
        .bind(score)
        .bind(&details_str)
        .bind(provider)
        .bind(model)
        .bind(&input_str)
        .bind(&params_str)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| EvalError::Internal(format!("写入评测结果失败: {e}")))?;

        Ok(result.last_insert_rowid())
    }

    /// 按条件查询评测结果
    pub async fn query(&self, params: &QueryParams) -> Result<Vec<EvalResultRow>, EvalError> {
        let limit = params.limit.clamp(1, 100);
        let offset = params.offset;

        let mut sql = String::from(
            r#"
            SELECT id, request_id, metric, score, details, provider, model,
                   input, params, created_at
            FROM eval_results
            WHERE 1=1
            "#,
        );

        if params.metric.is_some() {
            sql.push_str(" AND metric = ?");
        }
        if params.request_id.is_some() {
            sql.push_str(" AND request_id = ?");
        }
        if params.min_score.is_some() {
            sql.push_str(" AND score >= ?");
        }
        if params.max_score.is_some() {
            sql.push_str(" AND score <= ?");
        }
        if params.start_time.is_some() {
            sql.push_str(" AND created_at >= ?");
        }
        if params.end_time.is_some() {
            sql.push_str(" AND created_at <= ?");
        }
        if params.provider.is_some() {
            sql.push_str(" AND provider = ?");
        }
        if params.model.is_some() {
            sql.push_str(" AND model = ?");
        }
        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?;");

        let mut q = sqlx::query(&sql);

        if let Some(v) = &params.metric {
            q = q.bind(v);
        }
        if let Some(v) = &params.request_id {
            q = q.bind(v);
        }
        if let Some(v) = params.min_score {
            q = q.bind(v);
        }
        if let Some(v) = params.max_score {
            q = q.bind(v);
        }
        if let Some(v) = &params.start_time {
            q = q.bind(v);
        }
        if let Some(v) = &params.end_time {
            q = q.bind(v);
        }
        if let Some(v) = &params.provider {
            q = q.bind(v);
        }
        if let Some(v) = &params.model {
            q = q.bind(v);
        }
        q = q.bind(limit as i64).bind(offset as i64);

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| EvalError::Internal(format!("查询评测结果失败: {e}")))?;

        let results = rows
            .into_iter()
            .map(|row| -> Result<EvalResultRow, EvalError> {
                let details_str: String = row
                    .try_get("details")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let input_str: String = row
                    .try_get("input")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let params_str: String = row
                    .try_get("params")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let created_at_str: String = row
                    .try_get("created_at")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;

                let details: Value = serde_json::from_str(&details_str).unwrap_or(Value::Null);
                let input: Value = serde_json::from_str(&input_str).unwrap_or(Value::Null);
                let params: Value = serde_json::from_str(&params_str).unwrap_or(Value::Null);
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(EvalResultRow {
                    id: row
                        .try_get("id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    request_id: row
                        .try_get("request_id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    metric: row
                        .try_get("metric")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    score: row
                        .try_get("score")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    details,
                    provider: row
                        .try_get("provider")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    model: row
                        .try_get("model")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    input,
                    params,
                    created_at,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// 按 ID 查询单条记录
    pub async fn get_by_id(&self, id: i64) -> Result<Option<EvalResultRow>, EvalError> {
        let row = sqlx::query(
            r#"
            SELECT id, request_id, metric, score, details, provider, model,
                   input, params, created_at
            FROM eval_results
            WHERE id = ?;
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EvalError::Internal(format!("查询失败: {e}")))?;

        match row {
            Some(row) => {
                let details_str: String = row
                    .try_get("details")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let input_str: String = row
                    .try_get("input")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let params_str: String = row
                    .try_get("params")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let created_at_str: String = row
                    .try_get("created_at")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;

                let details: Value = serde_json::from_str(&details_str).unwrap_or(Value::Null);
                let input: Value = serde_json::from_str(&input_str).unwrap_or(Value::Null);
                let params: Value = serde_json::from_str(&params_str).unwrap_or(Value::Null);
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(EvalResultRow {
                    id: row
                        .try_get("id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    request_id: row
                        .try_get("request_id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    metric: row
                        .try_get("metric")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    score: row
                        .try_get("score")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    details,
                    provider: row
                        .try_get("provider")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    model: row
                        .try_get("model")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    input,
                    params,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// 按 request_id 查询单条记录
    ///
    /// # Arguments
    ///
    /// * `request_id` - 请求 ID
    ///
    /// # Returns
    ///
    /// 找到则返回 `Some(EvalResultRow)`，否则返回 `None`。
    pub async fn get_by_request_id(
        &self,
        request_id: &str,
    ) -> Result<Option<EvalResultRow>, EvalError> {
        let row = sqlx::query(
            r#"
            SELECT id, request_id, metric, score, details, provider, model,
                   input, params, created_at
            FROM eval_results
            WHERE request_id = ?
            ORDER BY created_at DESC
            LIMIT 1;
            "#,
        )
        .bind(request_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| EvalError::Internal(format!("查询失败: {e}")))?;

        match row {
            Some(row) => {
                let details_str: String = row
                    .try_get("details")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let input_str: String = row
                    .try_get("input")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let params_str: String = row
                    .try_get("params")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;
                let created_at_str: String = row
                    .try_get("created_at")
                    .map_err(|e| EvalError::Internal(e.to_string()))?;

                let details: Value = serde_json::from_str(&details_str).unwrap_or(Value::Null);
                let input: Value = serde_json::from_str(&input_str).unwrap_or(Value::Null);
                let params: Value = serde_json::from_str(&params_str).unwrap_or(Value::Null);
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(EvalResultRow {
                    id: row
                        .try_get("id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    request_id: row
                        .try_get("request_id")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    metric: row
                        .try_get("metric")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    score: row
                        .try_get("score")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    details,
                    provider: row
                        .try_get("provider")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    model: row
                        .try_get("model")
                        .map_err(|e| EvalError::Internal(e.to_string()))?,
                    input,
                    params,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// 按指标聚合统计
    pub async fn aggregate(
        &self,
        metric: &str,
        start_time: Option<&str>,
        end_time: Option<&str>,
    ) -> Result<AggregationResult, EvalError> {
        let mut sql = String::from(
            r#"
            SELECT
                COUNT(*)   AS count,
                AVG(score) AS avg_score,
                MAX(score) AS max_score,
                MIN(score) AS min_score
            FROM eval_results
            WHERE metric = ?
            "#,
        );
        if start_time.is_some() {
            sql.push_str(" AND created_at >= ?");
        }
        if end_time.is_some() {
            sql.push_str(" AND created_at <= ?");
        }
        sql.push(';');

        let mut q = sqlx::query(&sql).bind(metric);
        if let Some(v) = start_time {
            q = q.bind(v);
        }
        if let Some(v) = end_time {
            q = q.bind(v);
        }

        let row = q
            .fetch_one(&self.pool)
            .await
            .map_err(|e| EvalError::Internal(format!("聚合查询失败: {e}")))?;

        let count: i64 = row
            .try_get("count")
            .map_err(|e| EvalError::Internal(e.to_string()))?;
        let avg_score: Option<f64> = row
            .try_get("avg_score")
            .map_err(|e| EvalError::Internal(e.to_string()))?;
        let max_score: Option<f64> = row
            .try_get("max_score")
            .map_err(|e| EvalError::Internal(e.to_string()))?;
        let min_score: Option<f64> = row
            .try_get("min_score")
            .map_err(|e| EvalError::Internal(e.to_string()))?;

        // 标准差（单独查询，SQLite 不直接支持 STDDEV）
        let stddev: Option<f64> = if count > 0 {
            // 动态构建 WHERE 条件片段（不含 WHERE 关键字），确保子查询与外层查询一致
            let mut where_conditions = String::from("metric = ?");
            if start_time.is_some() {
                where_conditions.push_str(" AND created_at >= ?");
            }
            if end_time.is_some() {
                where_conditions.push_str(" AND created_at <= ?");
            }

            // SQL 模板：两个子查询和外层 WHERE 共享相同的条件片段
            let var_sql = format!(
                r#"
                SELECT AVG(
                    (score - (SELECT AVG(score) FROM eval_results WHERE {cond})) *
                    (score - (SELECT AVG(score) FROM eval_results WHERE {cond}))
                ) AS variance
                FROM eval_results
                WHERE {cond}
                "#,
                cond = where_conditions,
            );

            // 参数绑定顺序：子查询1(metric,[start],[end]) + 子查询2(metric,[start],[end]) + 外层(metric,[start],[end])
            let mut vq = sqlx::query(&var_sql).bind(metric);
            if let Some(v) = start_time {
                vq = vq.bind(v);
            }
            if let Some(v) = end_time {
                vq = vq.bind(v);
            }
            // 第二个子查询的参数
            vq = vq.bind(metric);
            if let Some(v) = start_time {
                vq = vq.bind(v);
            }
            if let Some(v) = end_time {
                vq = vq.bind(v);
            }
            // 外层 WHERE 的参数
            vq = vq.bind(metric);
            if let Some(v) = start_time {
                vq = vq.bind(v);
            }
            if let Some(v) = end_time {
                vq = vq.bind(v);
            }

            let var_row = vq
                .fetch_one(&self.pool)
                .await
                .map_err(|e| EvalError::Internal(format!("方差查询失败: {e}")))?;
            let variance: Option<f64> = var_row
                .try_get("variance")
                .map_err(|e| EvalError::Internal(e.to_string()))?;
            variance.map(|v| v.sqrt())
        } else {
            None
        };

        Ok(AggregationResult {
            metric: metric.to_string(),
            count,
            avg_score,
            max_score,
            min_score,
            stddev,
        })
    }

    /// 删除指定 ID 的记录
    pub async fn delete_by_id(&self, id: i64) -> Result<bool, EvalError> {
        let result = sqlx::query("DELETE FROM eval_results WHERE id = ?;")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| EvalError::Internal(format!("删除失败: {e}")))?;
        Ok(result.rows_affected() > 0)
    }

    /// 关闭连接池（优雅关闭）
    pub async fn close(&self) {
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn open_test_db() -> SqliteStore {
        // 用内存数据库测试（共享缓存模式 + 单连接，确保测试间隔离）
        let path = format!(
            "file:eval_test_{}?mode=memory&cache=shared",
            uuid::Uuid::new_v4().simple()
        );
        let opts = SqliteConnectOptions::from_str(&path)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        let store = SqliteStore { pool };
        store.migrate().await.unwrap();
        store
    }

    #[tokio::test]
    async fn test_insert_and_query() {
        let store = open_test_db().await;

        let id = store
            .insert(
                "req-1",
                "rouge",
                0.85,
                &json!({"source": "test"}),
                Some("openai"),
                Some("gpt-4"),
                &json!({"reference": "abc", "hypothesis": "abd"}),
                &json!({"variant": "n"}),
            )
            .await
            .unwrap();
        assert!(id > 0);

        let rows = store
            .query(&QueryParams {
                metric: Some("rouge".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].request_id, "req-1");
        assert!((rows[0].score - 0.85).abs() < 1e-9);
        assert_eq!(rows[0].provider.as_deref(), Some("openai"));
    }

    #[tokio::test]
    async fn test_query_by_score_range() {
        let store = open_test_db().await;
        for (i, score) in [0.2, 0.5, 0.8, 0.95].iter().enumerate() {
            store
                .insert(
                    &format!("req-{i}"),
                    "bleu",
                    *score,
                    &json!({}),
                    None,
                    None,
                    &json!({}),
                    &json!({}),
                )
                .await
                .unwrap();
        }

        let rows = store
            .query(&QueryParams {
                metric: Some("bleu".to_string()),
                min_score: Some(0.5),
                max_score: Some(0.9),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        for r in &rows {
            assert!(r.score >= 0.5 && r.score <= 0.9);
        }
    }

    #[tokio::test]
    async fn test_get_by_id() {
        let store = open_test_db().await;
        let id = store
            .insert(
                "req-x",
                "rouge",
                0.7,
                &json!({}),
                None,
                None,
                &json!({}),
                &json!({}),
            )
            .await
            .unwrap();

        let row = store.get_by_id(id).await.unwrap().unwrap();
        assert_eq!(row.id, id);
        assert_eq!(row.request_id, "req-x");

        let none = store.get_by_id(99999).await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn test_aggregate() {
        let store = open_test_db().await;
        for s in [0.5, 0.6, 0.7, 0.8, 0.9] {
            store
                .insert(
                    "req",
                    "rouge",
                    s,
                    &json!({}),
                    None,
                    None,
                    &json!({}),
                    &json!({}),
                )
                .await
                .unwrap();
        }

        let agg = store.aggregate("rouge", None, None).await.unwrap();
        assert_eq!(agg.count, 5);
        assert!((agg.avg_score.unwrap() - 0.7).abs() < 1e-9);
        assert!((agg.max_score.unwrap() - 0.9).abs() < 1e-9);
        assert!((agg.min_score.unwrap() - 0.5).abs() < 1e-9);
        assert!(agg.stddev.unwrap() > 0.0);
    }

    /// 回归测试：带时间范围的 stddev 应仅基于时间范围内的数据计算
    #[tokio::test]
    async fn test_aggregate_with_time_range_filters_stddev() {
        let store = open_test_db().await;

        // 早期数据：3 条，均值 0.5
        let early_time = "2020-01-01T00:00:00+00:00";
        for s in [0.4, 0.5, 0.6] {
            store
                .insert(
                    "req-early",
                    "rouge",
                    s,
                    &json!({}),
                    None,
                    None,
                    &json!({}),
                    &json!({}),
                )
                .await
                .unwrap();
            // 直接 UPDATE 已插入行的 created_at，模拟早期时间
            sqlx::query("UPDATE eval_results SET created_at = ? WHERE request_id = ?")
                .bind(early_time)
                .bind("req-early")
                .execute(&store.pool)
                .await
                .unwrap();
        }

        // 近期数据：2 条，均值 0.9
        for s in [0.85, 0.95] {
            store
                .insert(
                    "req-late",
                    "rouge",
                    s,
                    &json!({}),
                    None,
                    None,
                    &json!({}),
                    &json!({}),
                )
                .await
                .unwrap();
        }

        // 全量聚合：5 条数据
        let agg_all = store.aggregate("rouge", None, None).await.unwrap();
        assert_eq!(agg_all.count, 5);

        // 仅查询早期数据：count=3，stddev 应基于早期 3 条数据
        let agg_early = store
            .aggregate("rouge", Some(early_time), Some("2020-12-31T23:59:59+00:00"))
            .await
            .unwrap();
        assert_eq!(agg_early.count, 3);
        assert!((agg_early.avg_score.unwrap() - 0.5).abs() < 1e-9);
        // 早期数据 stddev = sqrt(((0.4-0.5)^2 + (0.5-0.5)^2 + (0.6-0.5)^2) / 3) = sqrt(0.02/3) ≈ 0.0816
        let expected_stddev = ((0.4_f64 - 0.5).powi(2) + 0.0 + (0.6_f64 - 0.5).powi(2)) / 3.0;
        let expected_stddev = expected_stddev.sqrt();
        assert!(
            (agg_early.stddev.unwrap() - expected_stddev).abs() < 1e-6,
            "stddev 应基于时间范围内的数据，实际: {}, 期望: {}",
            agg_early.stddev.unwrap(),
            expected_stddev
        );

        // 仅查询近期数据：count=2，均值 0.9
        let agg_late = store
            .aggregate("rouge", Some("2025-01-01T00:00:00+00:00"), None)
            .await
            .unwrap();
        assert_eq!(agg_late.count, 2);
        assert!((agg_late.avg_score.unwrap() - 0.9).abs() < 1e-9);
    }

    #[tokio::test]
    async fn test_delete() {
        let store = open_test_db().await;
        let id = store
            .insert(
                "req-d",
                "rouge",
                0.5,
                &json!({}),
                None,
                None,
                &json!({}),
                &json!({}),
            )
            .await
            .unwrap();
        assert!(store.delete_by_id(id).await.unwrap());
        assert!(store.get_by_id(id).await.unwrap().is_none());
        assert!(!store.delete_by_id(id).await.unwrap());
    }

    #[tokio::test]
    async fn test_pagination() {
        let store = open_test_db().await;
        for i in 0..25 {
            store
                .insert(
                    &format!("req-{i}"),
                    "rouge",
                    0.5,
                    &json!({}),
                    None,
                    None,
                    &json!({}),
                    &json!({}),
                )
                .await
                .unwrap();
        }

        let page1 = store
            .query(&QueryParams {
                metric: Some("rouge".to_string()),
                limit: 10,
                offset: 0,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page1.len(), 10);

        let page3 = store
            .query(&QueryParams {
                metric: Some("rouge".to_string()),
                limit: 10,
                offset: 20,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(page3.len(), 5);
    }
}
