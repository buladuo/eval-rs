# 贡献指南（CONTRIBUTING）

感谢你为 **eval-rs** 做出贡献！本文件说明如何搭建开发环境、遵循的代码规范，以及如何扩展核心能力（指标、Provider、提示词）。

---

## 1. 开发环境

- **Rust 1.85+**（项目使用 `edition = "2024"`，请确保工具链足够新：`rustup update stable`）。
- 一个可用的 LLM Provider 用于运行涉及 LLM 的测试/手动验证（可选，纯 NLP 指标不依赖）。

```bash
git clone https://github.com/buladuo/eval-rs.git eval-rs && cd eval-rs
cp .env.example .env          # 填入 EVAL_PROVIDERS__<NAME>__API_KEY
cargo build
cargo test
```

---

## 2. 提交前检查清单

在提交 Pull Request 前，请确保：

- [ ] `cargo fmt --check` 通过（或运行 `cargo fmt` 自动格式化）
- [ ] `cargo clippy --all-targets -- -D warnings` 无警告
- [ ] `cargo test` 全部通过
- [ ] 新增公共 API（`pub` 项、trait、unsafe 代码）均带有符合 [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) 的文档注释（模块级 `//!`、项级 `///`，含 `# Examples`）
- [ ] 修改了配置/API 时，同步更新 `README.md` 与 `config/default.toml` / `.env.example`
- [ ] 新增指标/提示词时补充了测试

---

## 3. 代码规范

- **注释语言**：公开文档注释与代码注释使用**简体中文**；代码标识符、命令保持英文。
- **文档注释结构**：`pub` 项需包含简短描述 + 详细说明 + 必要的 `# Arguments` / `# Returns` / `# Errors` / `# Panics` / `# Safety`（unsafe 必需） / `# Examples`。
- **unsafe 代码**：必须附 `// SAFETY:` 说明安全契约。
- **错误处理**：统一使用 `error::EvalError`（`thiserror`），不在业务代码中裸用 `unwrap()`/`expect()`（测试与示例除外）。
- **日志**：使用 `tracing` 的 `info!`/`warn!`/`error!` 宏，并附带结构化字段（如 `metric`、`request_id`）。
- **特殊标记**：`// TODO:` / `// FIXME:` / `// HACK:` / `// NOTE:` / `// PERF:` / `// XXX:`。

---

## 4. 扩展指南

### 新增一个指标

指标需实现 `metrics::registry::Metric` trait，并在引擎初始化时注册到 `MetricRegistry`。

**非 LLM 指标（纯函数，无提示词）：**

1. 在 `src/metrics/` 下新建模块（参考 `bleu.rs` / `rouge.rs` / `perplexity.rs`）。
2. 实现 `Metric`：`name()` 返回唯一名，`metric_type()` 返回 `"non_llm"`，`params_schema()` 描述参数，`evaluate()` 返回 `MetricOutput { score, details }`。
3. 在 `metrics/mod.rs` 的注册函数中 `reg.register(Box::new(MyMetric))`。
4. 补充单元测试。

**LLM-as-Judge 指标：**

1. 在 `prompts/` 下新增 TOML 提示词文件（参考 `llm_judge_accuracy.toml`），结构包含 `name`、`version`、`description`、`template`（tera 语法）、`[variables.*]` 声明。
2. 复用一个通用的 `LlmJudgeMetric` 或在 `src/metrics/llm_judge.rs` 中注册该提示词对应的指标。
3. 提示词输出需为可解析的 JSON（`{"score": f64, "reason": str}`），指标负责解析并映射为 `MetricOutput`。

### 新增一个 Provider

1. 实现 `provider::LlmProvider` trait（`complete(prompt, model)`）。
2. 在 `provider/rig_provider.rs` 中基于 `rig` 封装对应类型（已有 OpenAI / Anthropic 兼容实现）。
3. 在 `config/default.toml` 增加 `[providers.<name>]` 段落，设置 `provider_type` / `base_url` / `model` / `default`，密钥通过环境变量注入。

### 新增提示词模板

- 在 `prompts/` 目录新增 TOML，使用 tera 模板（`{{ variable }}`）。
- 在 `[variables.<name>]` 中声明类型与是否必填，供加载器校验。
- 提示词遵循「指令 + 输入变量 + 输出 JSON schema」的通用结构，便于对齐 Ragas 风格指标。

---

## 5. 提交规范

- 分支命名：`<type>/<short-description>`，如 `feat/llm-judge-hallucination`、`fix/storage-backpressure`。
- 提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)：`feat:` / `fix:` / `docs:` / `refactor:` / `test:` / `chore:`。
- 一个 PR 聚焦一个主题；大型改动请先提 Issue 讨论设计。

---

## 6. 测试

```bash
cargo test                       # 全部测试
cargo test --lib metrics         # 指定模块
cargo test -- --nocapture        # 显示日志输出
```

涉及 LLM 的测试依赖外部 Provider；若未配置密钥，相关用例应标记为 `#[ignore]` 或跳过，避免 CI 失败。

---

## 7. 行为准则

请保持友好、专业的交流氛围。对设计有分歧时以 Issue 讨论为依据，聚焦技术方案本身。
