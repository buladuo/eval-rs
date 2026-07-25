# eval-rs

> 一个模块化、高性能的 LLM 评测服务（Evaluation-as-a-Service），使用 Rust 编写，通过 HTTP API 提供 **LLM-as-Judge** 与 **传统 NLP 指标** 两种评测能力。

`eval-rs` 将评测逻辑抽象为可插拔的「指标（Metric）」，支持基于 `rig` 的多种 LLM Provider 调用、基于 `tera` 的提示词模板渲染、输入预处理（JSONPath / 正则提取），并将每次评测结果持久化到 SQLite 以便查询与聚合分析（罗盘功能）。

---

## 特性

- **双引擎指标**：内置 LLM-as-Judge 指标（`llm_judge_accuracy` / `llm_judge_fluency` / `llm_judge_relevance`）与传统 NLP 指标（`bleu` / `rouge` / `perplexity`），指标通过统一 `Metric` trait 注册，可热插拔扩展。
- **HTTP API（axum）**：RESTful 接口，支持单次评测、批量评测（后台任务 + 状态查询）、指标列表、结果分页查询、聚合统计与 JSON 导出。
- **多 Provider 支持**：基于 `rig` 封装 OpenAI / Anthropic 及任意 OpenAI 兼容服务；支持重试（指数退避）、并发控制、RPM / Token 限流。
- **提示词模板化**：提示词以 TOML 文件管理，使用 `tera` 模板引擎渲染，变量经 schema 描述与校验。
- **输入预处理**：支持 `json_path` 与 `regex` 两种方式从复杂负载中抽取评测字段。
- **结果持久化**：结果异步（带背压的有界 channel）写入 SQLite，支持多维度过滤查询、时间范围聚合与文件下载。
- **可观测性**：基于 `tracing` 的结构化日志（文本 / JSON），健康检查端点探测依赖状态。
- **安全防护**：全局请求体大小限制（默认 1 MiB）与请求超时（默认 30s），防止资源耗尽与慢速攻击。

---

## 架构

```
                 ┌────────────────────────────────────────────┐
   HTTP Client ─▶│  api (axum)                                  │
                 │   /health  /v1/eval  /v1/metrics             │
                 │   /v1/results  (/aggregate, /download)       │
                 └───────────────┬────────────────────────────┘
                                 │
                                 ▼
                 ┌────────────────────────────────────────────┐
                 │  engine (EvalEngine / orchestrator / router) │
                 │   路由指标 → 编排执行 → 收集结果             │
                 └───────┬───────────────┬───────────────┬─────┘
                         │               │               │
              ┌──────────▼──┐   ┌─────────▼────┐  ┌───────▼────────┐
              │ metrics     │   │ provider     │  │ prompts        │
              │ (registry + │   │ (manager +   │  │ (registry +    │
              │  llm/nlp)   │   │  rig/retry/  │  │  loader/       │
              │             │   │  limits)     │  │  renderer)     │
              └──────────┬──┘   └─────────┬────┘  └───────┬────────┘
                         │               │               │
                         └───────────────┼───────────────┘
                                         ▼
                            ┌────────────────────────────┐
                            │  storage (SqliteStore)      │
                            │   SQLite + 有界队列背压     │
                            └────────────────────────────┘
```

| 模块 | 职责 |
|------|------|
| `api` | HTTP 端点、请求/响应模型、错误映射、路由组装与安全中间件 |
| `engine` | 评测引擎，编排单次评测、按指标名路由、生命周期管理 |
| `metrics` | 指标注册表与 `Metric` trait；LLM-as-Judge 与 NLP 指标实现 |
| `provider` | LLM 调用封装（多 Provider、重试、并发、限流） |
| `prompts` | 提示词注册、TOML 加载、tera 渲染 |
| `preprocessor` | 输入预处理（JSONPath / 正则提取 / 自动转换） |
| `storage` | SQLite 持久化与查询/聚合 |
| `settings` | 配置加载（TOML + 环境变量覆盖） |
| `error` | 统一错误类型 `EvalError` |
| `logging` | `tracing` 日志初始化（文本/JSON、文件滚动） |

---

## 快速开始

### 前置要求

- Rust **1.85+**（使用 `edition = "2024"`，需较新的 nightly/stable 工具链）
- 一个可用的 LLM Provider（OpenAI / Anthropic 或任意 OpenAI 兼容端点）

### 构建与运行

```bash
# 克隆仓库
git clone https://github.com/buladuo/eval-rs.git eval-rs && cd eval-rs

# 配置环境变量（API 密钥等）
cp .env.example .env
# 编辑 .env，填入 EVAL_PROVIDERS__GLM__API_KEY 等

# 编译并启动（默认监听 0.0.0.0:8080）
cargo run --release

# 或仅编译
cargo build --release
./target/release/eval-rs
```

### 第一次评测

```bash
curl -X POST http://localhost:8080/v1/eval \
  -H 'Content-Type: application/json' \
  -d '{
    "metric": "llm_judge_accuracy",
    "input": {
      "reference": "中国的首都是北京。",
      "hypothesis": "北京是中国的首都。"
    }
  }'
```

返回示例：

```json
{
  "metric": "llm_judge_accuracy",
  "score": 1.0,
  "details": { "reason": "回答与参考答案事实一致" },
  "request_id": "3f1a...c9"
}
```

---

## 配置

配置优先级：**环境变量 `EVAL_*` > `config/default.toml` > Rust 默认值**。

环境变量使用 `EVAL_` 前缀，双下划线 `__` 表示嵌套字段，例如 `EVAL_SERVER__PORT=9090`、`EVAL_PROVIDERS__GLM__API_KEY=sk-xxx`。

| 配置项 | 默认值 | 说明 |
|--------|--------|------|
| `server.host` | `0.0.0.0` | 监听地址 |
| `server.port` | `8080` | 监听端口 |
| `log.level` | `info` | 日志级别：`error`/`warn`/`info`/`debug`/`trace` |
| `log.format` | `text` | 日志格式：`text`(可读) / `json`(每行一条 JSON) |
| `limits.max_concurrency` | `10` | 最大并发 LLM 请求数 |
| `limits.requests_per_minute` | `60` | 每分钟最大请求数（RPM） |
| `limits.tokens_per_minute` | — | 每分钟最大 token 数（可选） |
| `limits.tokens_per_second` | — | 每秒最大 token 数（可选） |
| `prompts.dir` | `prompts` | 提示词 TOML 文件目录 |
| `eval.timeout_secs` | `120` | 单次评测超时（秒） |
| `storage.enabled` | `true` | 是否启用 SQLite 持久化 |
| `storage.db_path` | `data/eval.db` | SQLite 数据库文件路径 |
| `providers.<name>.provider_type` | — | `openai` 或 `anthropic` |
| `providers.<name>.base_url` | — | API 基础 URL（OpenAI 兼容服务） |
| `providers.<name>.api_key` | — | API 密钥（建议用环境变量设置） |
| `providers.<name>.model` | — | 模型名称 |
| `providers.<name>.default` | `false` | 是否为默认 Provider |

> **安全提示**：请勿在 `config/default.toml` 中写入明文密钥。使用 `EVAL_PROVIDERS__<NAME>__API_KEY` 注入，配置文件中保留 `api_key = ""` 即可。

完整示例见 [`config/default.toml`](config/default.toml) 与 [`.env.example`](.env.example)。

---

## API 参考

所有接口以 JSON 通信。全局保护：请求体上限 1 MiB，单请求超时 30s。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查（探测依赖状态） |
| POST | `/v1/eval` | 执行一次评测（同步返回） |
| GET | `/v1/metrics` | 列出所有已注册指标及其参数 schema |
| GET | `/v1/results` | 分页查询评测历史（多维度过滤） |
| GET | `/v1/results/{id}` | 查询单条评测记录 |
| GET | `/v1/results/aggregate` | 按指标聚合统计（均值/计数等） |
| GET | `/v1/results/download` | 按查询条件批量下载结果为 JSON 文件 |
| GET | `/v1/results/{id}/download` | 下载单条记录为 JSON 文件 |

### `POST /v1/eval`

请求体（`EvalRequestBody`）：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `metric` | string | 是 | 指标名称，如 `rouge`、`llm_judge_accuracy` |
| `input` | JSON | 是 | 评测输入，结构由指标定义（常见 `reference`/`hypothesis`/`text`） |
| `params` | object | 否 | 指标参数键值（schema 见 `GET /v1/metrics`） |
| `extract` | object | 否 | 输入预处理：`{"json_path":"..."}` 或 `{"regex":"..."}` |
| `retry` | object | 否 | 重试覆盖：`{"max_attempts":3,"backoff_ms":1000}` |
| `provider` | string | 否 | 指定 Provider，缺省使用默认 Provider |
| `request_id` | string | 否 | 透传/自动生成 UUIDv4 |

```bash
# 传统 NLP 指标：BLEU
curl -X POST http://localhost:8080/v1/eval \
  -H 'Content-Type: application/json' \
  -d '{
    "metric": "bleu",
    "input": {
      "reference": "the cat sat on the mat",
      "hypothesis": "the cat is on the mat"
    }
  }'

# 通过 json_path 预处理：从复杂负载抽取字段
curl -X POST http://localhost:8080/v1/eval \
  -H 'Content-Type: application/json' \
  -d '{
    "metric": "llm_judge_relevance",
    "input": {
      "data": { "ref": "北京是首都", "ans": "北京是中国的首都" }
    },
    "extract": { "json_path": "$.data" }
  }'
```

### `GET /v1/metrics`

```bash
curl http://localhost:8080/v1/metrics
```

### `GET /v1/results`

支持过滤参数：`metric`、`request_id`、`min_score`、`max_score`、`start_time`、`end_time`（ISO 8601）、`provider`、`model`、`offset`、`limit`（默认 0/20，最大 100）。

```bash
curl "http://localhost:8080/v1/results?metric=rouge&limit=10"
curl "http://localhost:8080/v1/results/aggregate?metric=rouge"
```

### `GET /health`

```bash
curl http://localhost:8080/health
# { "status": "ok", "dependencies": {} }
```

错误响应统一为 `{ "error": "<code>", "details": "<message>" }`，HTTP 状态码与 `EvalError` 语义对应（如 `MetricNotFound` → 404）。

---

## 指标

指标通过 `Metric` trait 实现并以名称注册到 `MetricRegistry`。

### LLM-as-Judge（类型 `llm`）

依赖 LLM Provider，提示词模板位于 [`prompts/`](prompts/)（TOML + tera）。

| 指标 | 输入 | 说明 |
|------|------|------|
| `llm_judge_accuracy` | `reference`, `hypothesis` | 评估回答与参考答案的事实一致性，返回 0~1 |
| `llm_judge_fluency` | `hypothesis` | 评估回答的语言流畅度，返回 0~1 |
| `llm_judge_relevance` | `reference`, `hypothesis` | 评估回答与问题的相关性，返回 0~1 |

### 传统 NLP（类型 `non_llm`）

| 指标 | 输入 | 说明 |
|------|------|------|
| `bleu` | `reference`, `hypothesis` | 机器翻译/生成质量（n-gram 重合度） |
| `rouge` | `reference`, `hypothesis` | 摘要/召回类指标（ROUGE-N/L） |
| `perplexity` | `text` | 语言模型困惑度 |

### 扩展自定义指标

实现 `metrics::registry::Metric` trait 并注册即可（详见 [`CONTRIBUTING.md`](CONTRIBUTING.md#新增一个指标)）。LLM 指标只需新增一个提示词 TOML 文件并在代码中注册 `LlmJudgeMetric`。

---

## 存储

启用后，评测结果通过**有界 channel（容量 1024）**异步写入 SQLite（`storage.db_path`）。队列满时新写入任务被丢弃并记录警告，防止内存无限增长。支持：

- 分页 / 多维过滤查询（`GET /v1/results`）
- 按指标聚合并返回统计（`GET /v1/results/aggregate`）
- 单条 / 批量导出为 JSON 文件

---

## 开发

```bash
cargo build            # 编译
cargo test             # 运行单元测试与集成测试
cargo clippy --all-targets -- -D warnings   # Lint（建议零警告）
cargo fmt --check      # 代码格式检查
```

项目结构见 [`src/`](src/)，调研资料见 [`docs/`](docs/)（`ragas_metrics.md`、`eval_frameworks.md`）。

---

## 许可证

[MIT](LICENSE)
