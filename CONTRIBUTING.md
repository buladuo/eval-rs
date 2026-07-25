# 贡献指南

感谢你为 **eval-rs** 做出贡献！本文件说明如何搭建开发环境、遵循的代码规范，以及如何扩展核心能力（指标、Provider、提示词）。

## 行为准则

参与本项目即表示你同意遵守我们的行为准则。请保持尊重、友好和建设性的交流。

## 贡献方式

- 🐛 报告 Bug
- 💡 提出新功能建议
- 📝 改进文档
- 🔧 提交代码修复或新功能
- ✅ 编写测试用例

## 开发环境设置

### 前置要求

- **Rust 1.85+**（项目使用 `edition = "2024"`，请确保工具链足够新：`rustup update stable`）
- 一个可用的 LLM Provider（OpenAI / Anthropic 或任意 OpenAI 兼容端点），用于运行涉及 LLM 的测试 / 手动验证（纯 NLP 指标不依赖）
- Git

### 安装依赖

```bash
# 克隆仓库
git clone https://github.com/buladuo/eval-rs.git
cd eval-rs

# 安装 Rust 工具链
rustup update stable

# 安装开发工具
rustup component add rustfmt clippy

# 配置环境变量（API 密钥等，纯 NLP 指标可跳过）
cp .env.example .env
# 编辑 .env，填入 EVAL_PROVIDERS__<NAME>__API_KEY

# 构建项目
cargo build

# 运行测试
cargo test
```

## 项目结构

```
.
├── src/                    # 源代码（按职责分层，测试内联在各模块 #[cfg(test)] 中）
│   ├── api/                # HTTP 层（axum 路由、请求/响应模型、错误映射）
│   ├── engine/             # 评测引擎（编排单次评测、指标路由、存储背压）
│   ├── metrics/            # 指标注册表与 Metric trait；LLM-as-Judge 与 NLP 指标
│   ├── provider/           # LLM 调用封装（多 Provider、重试、并发、限流）
│   ├── prompts/            # 提示词注册、TOML 加载、tera 渲染
│   ├── preprocessor/       # 输入预处理（JSONPath / 正则提取）
│   ├── storage/            # SQLite 持久化与查询/聚合
│   ├── settings/           # 配置加载（TOML + 环境变量覆盖）
│   ├── error/              # 统一错误类型 EvalError
│   └── logging/            # tracing 日志初始化
├── prompts/                # 提示词模板（TOML + tera，如 llm_judge_accuracy.toml）
├── config/                 # 运行配置（default.toml）
├── docs/                   # 调研与文档（ragas_metrics.md、eval_frameworks.md）
└── .env.example            # 环境变量样例
```

> 集成测试目前以内联方式写在各模块 `#[cfg(test)]` 中；如需独立的端到端测试，可新建 `tests/` 目录。

## 开发流程

### 1. 创建分支

```bash
# 从 main 分支创建功能分支
git checkout -b feature/<username>/your-feature-name

# 或修复分支
git checkout -b fix/<username>/issue-description
```

**分支命名规范：**

格式：`<type>/<username>/<description>`

- `feature/<username>/xxx` - 新功能
- `fix/<username>/xxx` - Bug 修复
- `docs/<username>/xxx` - 文档更新
- `refactor/<username>/xxx` - 代码重构
- `test/<username>/xxx` - 测试相关
- `perf/<username>/xxx` - 性能优化

**示例：**
```bash
git checkout -b feature/weizhikong/add-hallucination-metric
git checkout -b fix/weizhikong/storage-backpressure-drop
git checkout -b docs/weizhikong/api-examples
git checkout -b refactor/weizhikong/simplify-error-handling
```

**说明：**
- `<username>` 使用你的 GitHub 用户名
- `<description>` 使用中划线分隔的简短描述
- 描述使用小写字母，避免使用下划线
- 保持描述简洁明了，一般不超过 5 个单词

### 2. 编写代码

**代码规范：**
- 遵循 Rust 官方编码风格
- 使用 `rustfmt` 格式化代码
- 通过 `clippy` 检查
- 公开文档注释与代码注释使用**简体中文**；代码标识符、命令保持英文
- 为公共 API（`pub` 项、trait、unsafe 代码）编写文档注释（`///` / `//!`），含必要的 `# Arguments` / `# Returns` / `# Errors` / `# Panics` / `# Safety` / `# Examples`
- `unsafe` 代码必须附 `// SAFETY:` 说明安全契约
- 统一使用 `error::EvalError`（`thiserror`），不在业务代码中裸用 `unwrap()` / `expect()`（测试与示例除外）
- 使用 `tracing` 的 `info!` / `warn!` / `error!` 宏记录日志，附结构化字段（如 `metric`、`request_id`）

### 3. 质量检查

在提交前，**必须**运行以下检查：

```bash
# 1. 代码格式化
cargo fmt --all

# 2. 编译检查
cargo check --all-targets

# 3. Clippy 静态分析
cargo clippy --all-targets -- -D warnings

# 4. 运行所有测试
cargo test

# 5. 检查文档
cargo doc --no-deps
```

**一键检查脚本：**

可以创建 `scripts/check.sh` 方便执行：

```bash
#!/bin/bash
set -e

echo "🔍 Running cargo fmt..."
cargo fmt --all -- --check

echo "🔍 Running cargo check..."
cargo check --all-targets

echo "🔍 Running cargo clippy..."
cargo clippy --all-targets -- -D warnings

echo "🔍 Running cargo test..."
cargo test

echo "✅ All checks passed!"
```

### 4. 提交代码

**Commit Message 规范：**

每个 commit message 必须包含以下部分：

```
<type>(<scope>): <subject>

[背景说明]
问题描述和产生原因

[处理方式]
采用的解决方案和实现思路

[改动清单]
- 修改文件1：具体改动
- 修改文件2：具体改动
- 新增文件3：功能说明

[结果结论]
修复效果和验证方式

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过
- cargo test: ✅ 通过 (新增 X 个测试)
```

**Type 类型：**
- `feat`: 新功能
- `fix`: Bug 修复
- `docs`: 文档更新
- `style`: 代码格式调整（不影响功能）
- `refactor`: 代码重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建/工具链相关

**Scope 范围：**
模块名或功能域，如：`metrics`、`provider`、`api`、`storage`、`engine`

**Subject 主题：**
- 使用祈使句，现在时态："添加指标" 而非 "添加了指标"
- 首字母小写
- 结尾不加句号
- 限制在 50 字符以内

**示例 Commit 1 - Bug 修复：**

```
fix(storage): 修复聚合统计缺失时间范围过滤

[背景说明]
store.rs 的 aggregate 标准差子查询未应用 start_time/end_time 过滤，
导致聚合结果与列表查询的时间范围不一致，统计值被历史数据污染。

[处理方式]
1. 在标准差子查询中复用与列表查询相同的时间范围条件
2. 新增回归测试 test_aggregate_with_time_range_filters_stddev 覆盖该场景

[改动清单]
- src/storage/store.rs: aggregate 标准差子查询添加时间范围绑定参数
- src/storage/store.rs: 新增 test_aggregate_with_time_range_filters_stddev

[结果结论]
聚合与列表查询在相同时间范围内结果一致；回归测试通过。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过
- cargo test: ✅ 通过 (新增 1 个测试)
```

**示例 Commit 2 - 新功能：**

```
feat(metrics): 新增 llm_judge_hallucination 指标

[背景说明]
现有 LLM-as-Judge 指标缺少幻觉检测维度，用户希望在 RAG 场景下评估
回答是否被检索上下文支撑。

[处理方式]
1. 在 prompts/ 新增 llm_judge_hallucination.toml（tera 模板，输出 JSON）
2. 复用 LlmJudgeMetric 注册该提示词对应的指标
3. 在引擎初始化处将其注册到 MetricRegistry

[改动清单]
- prompts/llm_judge_hallucination.toml: 新增提示词模板
- src/metrics/llm_judge.rs: 适配 hallucination 的输出解析
- src/main.rs: 注册新指标到 MetricRegistry
- src/metrics/llm_judge.rs: 新增单元测试

[结果结论]
通过 POST /v1/eval 以 metric=llm_judge_hallucination 调用可得 0~1 评分。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过
- cargo test: ✅ 通过 (新增 3 个测试)
```

**示例 Commit 3 - 重构：**

```
refactor(error): 统一错误类型到 EvalError

[背景说明]
原有错误散落在各模块，错误转换逻辑重复且缺乏上下文。
使用 thiserror 统一错误定义，并为 axum 实现 IntoResponse 以自动映射 HTTP 状态码。

[处理方式]
1. 将所有错误类型集中到 src/error.rs 的 EvalError（thiserror）
2. 为 EvalError 实现 axum::response::IntoResponse，按变体映射 HTTP 状态码
3. 移除各模块的本地错误定义

[改动清单]
- src/error.rs: 集中定义 EvalError 并实现 IntoResponse
- src/api/routes/*.rs: 统一返回 EvalError
- src/*: 移除冗余错误包装

[结果结论]
错误处理更统一，Handler 可直接返回 EvalError；编译无警告。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过
- cargo test: ✅ 通过
```

### 5. 提交前检查清单

在 `git commit` 之前，确认：

- [ ] 代码已通过 `cargo fmt`
- [ ] 代码已通过 `cargo check`
- [ ] 代码已通过 `cargo clippy` 且无警告
- [ ] 所有测试通过 `cargo test`
- [ ] 为新功能添加了测试
- [ ] 为公共 API 添加了文档注释
- [ ] 修改了配置 / API 时同步更新 `README.md` 与 `config/default.toml` / `.env.example`
- [ ] Commit message 符合规范并包含所有必需部分
- [ ] 分支命名符合 `<type>/<username>/<description>` 格式

### 6. 推送并创建 PR

```bash
# 推送到远程分支
git push -u origin feature/<username>/your-feature-name

# 使用 GitHub CLI 创建 PR（推荐）
gh pr create --title "feat(scope): 简短描述" --body-file .github/pull_request_template.md

# 或手动在 GitHub 网页创建 PR
```

## Pull Request 规范

### PR 标题格式

与 commit message 的 subject 格式一致：
```
<type>(<scope>): <description>
```

示例：
- `feat(metrics): 新增 llm_judge_hallucination 指标`
- `fix(storage): 修复聚合统计缺失时间范围过滤`
- `docs(readme): 更新 API 参考`

### PR 描述模板

```markdown
## 变更说明

简要描述本 PR 的目的和内容

## 变更类型

- [ ] 🐛 Bug 修复
- [ ] ✨ 新功能
- [ ] 📝 文档更新
- [ ] 🎨 代码格式/风格
- [ ] ♻️ 代码重构
- [ ] ⚡ 性能优化
- [ ] ✅ 测试相关
- [ ] 🔧 构建/工具链

## 背景说明

问题描述和产生原因

## 解决方案

采用的处理方式和实现思路

## 主要改动

- 改动点 1
- 改动点 2
- 改动点 3

## 相关 Issue

Closes #(issue number)
Refs #(related issue)

## 测试情况

- [ ] 单元测试（新增 X 个测试用例）
- [ ] 集成测试
- [ ] 手动测试
- [ ] 性能测试（如适用）

### 测试结果

描述测试覆盖的场景和验证结果

## 质量检查

- [ ] `cargo fmt` 通过
- [ ] `cargo check` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 全部通过
- [ ] `cargo doc` 生成正常
- [ ] 添加了必要的测试
- [ ] 更新了相关文档
- [ ] 检查了性能影响

## 破坏性变更

- [ ] 无破坏性变更
- [ ] 有破坏性变更（请在下方说明）

如有破坏性变更，请说明：
- 影响范围
- 迁移指南
- 弃用计划

## 截图/演示（如适用）

## 额外说明

任何需要审查者特别注意的地方

---

**Checklist for Reviewer:**
- [ ] 代码逻辑正确
- [ ] 测试覆盖充分
- [ ] 文档完整清晰
- [ ] 性能无明显回归
- [ ] 安全性考虑充分
- [ ] API 设计合理
```

## 扩展指南（eval-rs 专属）

### 新增一个指标

指标需实现 `metrics::registry::Metric` trait，并在引擎初始化时注册到 `MetricRegistry`。

**非 LLM 指标（纯函数，无提示词）：**

1. 在 `src/metrics/` 下新建模块（参考 `bleu.rs` / `rouge.rs` / `perplexity.rs`）。
2. 实现 `Metric`：`name()` 返回唯一名，`metric_type()` 返回 `"non_llm"`，`params_schema()` 描述参数，`evaluate()` 返回 `MetricOutput { score, details }`。
3. 在 `src/main.rs` 的注册函数中 `metric_registry.register(Box::new(MyMetric))`。
4. 补充单元测试。

**LLM-as-Judge 指标：**

1. 在 `prompts/` 下新增 TOML 提示词文件（参考 `llm_judge_accuracy.toml`），结构含 `name`、`version`、`description`、`template`（tera 语法）、`[variables.*]` 声明。
2. 复用通用的 `LlmJudgeMetric`（或参照 `src/metrics/llm_judge.rs`）在引擎初始化处注册该提示词对应的指标。
3. 提示词输出需为可解析的 JSON（如 `{"score": f64, "reason": str}`），`LlmJudgeMetric` 负责解析并映射为 `MetricOutput`。

### 新增一个 Provider

1. 实现 `provider::LlmProvider` trait（`complete(prompt, model)`）。
2. 在 `src/provider/rig_provider.rs` 中基于 `rig` 封装对应类型（已有 OpenAI / Anthropic 兼容实现）。
3. 在 `config/default.toml` 增加 `[providers.<name>]` 段落，设置 `provider_type` / `base_url` / `model` / `default`，密钥通过环境变量 `EVAL_PROVIDERS__<NAME>__API_KEY` 注入（请勿在配置文件中写入明文密钥）。

### 新增提示词模板

- 在 `prompts/` 目录新增 TOML，使用 tera 模板（`{{ variable }}`）。
- 在 `[variables.<name>]` 中声明类型与是否必填，供加载器校验。
- 提示词遵循「指令 + 输入变量 + 输出 JSON schema」的通用结构，便于与 Ragas 风格指标对齐。

### 配置与环境变量

配置优先级：**环境变量 `EVAL_*` > `config/default.toml` > Rust 默认值**。

环境变量使用 `EVAL_` 前缀，双下划线 `__` 表示嵌套字段，例如 `EVAL_SERVER__PORT=9090`、`EVAL_PROVIDERS__GLM__API_KEY=sk-xxx`。完整样例见 `.env.example`。

## 测试要求

### 测试覆盖率目标

- 核心逻辑代码：≥ 90%
- 整体代码：≥ 80%
- 关键路径：100%

### 单元测试

每个公共函数和方法都应有单元测试，覆盖：
- 正常情况
- 边界条件
- 错误情况
- 特殊输入

测试以内联方式写在各模块 `#[cfg(test)]` 中（参考 `src/metrics/bleu.rs`、`src/metrics/llm_judge.rs`）。涉及 LLM 的测试若需真实 Provider，可用 `mockall` 构造 mock，或用 `#[ignore]` 标记避免在无密钥的 CI 中失败。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_case() {
        let result = function_under_test(valid_input);
        assert_eq!(result, expected_output);
    }

    #[test]
    fn test_edge_case() {
        let result = function_under_test(edge_case_input);
        assert_eq!(result, expected_edge_output);
    }

    #[test]
    fn test_error_result() {
        let result = fallible_function(invalid_input);
        assert!(result.is_err());
    }
}
```

### 集成测试

可放在 `tests/` 目录，测试模块间的交互：

```rust
// tests/integration_test.rs
use eval_rs::engine::EvalEngine;
use eval_rs::metrics::registry::MetricRegistry;

#[tokio::test]
async fn test_end_to_end_eval() {
    // 完整的使用场景测试（构造引擎、注册指标、执行评测）
}
```

### 文档测试

在文档注释中编写可执行的示例代码（项目代码注释即大量使用此风格）：

```rust
/// 计算两个评测分数的均值
///
/// # Examples
///
/// ```
/// use eval_rs::metrics::registry::MetricOutput;
///
/// let out = MetricOutput { score: 0.8, details: serde_json::Value::Null };
/// assert!(out.score > 0.5);
/// ```
pub fn average_score(_a: f64, _b: f64) -> f64 {
    // 实现
    0.0
}
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 显示输出（包括 tracing 日志）
cargo test -- --nocapture

# 运行忽略的测试（如依赖真实 LLM Provider 的用例）
cargo test -- --ignored

# 并行度控制
cargo test -- --test-threads=1
```

## 文档规范

### API 文档

所有公共 API **必须**有文档注释（项目现有代码已遵循此规范）：

```rust
/// 简短的一句话描述（祈使句，现在时）
///
/// 更详细的说明，可以多行。解释函数的用途、设计决策、
/// 使用场景等。使用 Markdown 格式。
///
/// # Arguments
///
/// * `param1` - 第一个参数的说明
/// * `param2` - 第二个参数的说明，可以多行
///   继续说明
///
/// # Returns
///
/// 返回值的详细说明
///
/// # Errors
///
/// 列出可能返回的错误类型和触发条件：
/// - `EvalError::InvalidParams` - 当缺少必填参数时
///
/// # Examples
///
/// 基本使用：
///
/// ```
/// use eval_rs::metrics::registry::MetricRegistry;
///
/// let mut reg = MetricRegistry::new();
/// assert!(reg.get("rouge").is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// # Panics
///
/// 说明在什么情况下会 panic（如果会的话）
///
/// # Safety
///
/// 如果是 unsafe 函数，说明安全使用的前提条件
///
/// # See Also
///
/// - [`Metric`] - 指标 trait
pub fn function_name(param1: Type1, param2: Type2) {
    // 实现
}
```

### 模块文档

```rust
//! 模块级文档，使用 //! 而不是 ///
//!
//! 这个模块提供了 XXX 功能。
//!
//! # 主要组件
//!
//! - [`ComponentA`] - 组件 A 的说明
//! - [`ComponentB`] - 组件 B 的说明
//!
//! # 使用示例
//!
//! ```
//! use eval_rs::module_name::*;
//!
//! // 示例代码
//! ```

// 模块代码
```

### 文档最佳实践

1. **使用主动语态和祈使句**：
   - ✅ "计算评测分数"
   - ❌ "这个函数计算评测分数"

2. **提供可运行的示例**：
   - 所有示例代码都应该能通过 `cargo test`
   - 使用 `# Ok::<(), Error>(())` 处理 Result 返回

3. **使用内部链接**：
   - 使用 `[`Type`]` 链接到其他类型
   - 使用 `[`function_name`]` 链接到函数

4. **说明复杂度和性能特征**：
   - 对性能敏感的 API 说明时间/空间复杂度

### 生成和检查文档

```bash
# 生成文档
cargo doc --no-deps

# 在浏览器中打开文档
cargo doc --no-deps --open

# 检查文档链接
cargo doc --no-deps 2>&1 | grep warning

# 运行文档测试
cargo test --doc
```

## 性能要求

### 性能基准测试

项目当前未内置基准测试目录。如需对指标计算或 LLM 调用路径做基准测试，可引入 `criterion` 并在 `benches/` 下编写（需同步更新 `Cargo.toml`）：

```rust
// benches/metric_bench.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use eval_rs::metrics::bleu::BleuMetric;

fn benchmark_bleu(c: &mut Criterion) {
    c.bench_function("bleu", |b| {
        b.iter(|| {
            // 调用 BleuMetric 计算
        });
    });
}

criterion_group!(benches, benchmark_bleu);
criterion_main!(benches);
```

### 运行基准测试

```bash
# 运行所有基准测试（引入 criterion 后）
cargo bench
```

### 性能回归检测

PR 不应引入明显的性能回归：
- 关键路径性能下降 > 5% 需要说明原因
- 内存使用增加 > 10% 需要说明原因
- 如有性能提升，在 commit message 中说明

### 性能优化建议

- 使用 `cargo flamegraph` 分析性能热点
- 使用 `cargo bloat` 分析二进制大小
- 避免不必要的克隆和分配
- 合理使用并发与背压（评测结果写入已通过容量为 1024 的有界 channel 异步落库）

## 代码审查流程

### 提交 PR 后

1. **CI 自动检查**：GitHub Actions 会自动运行
   - 格式检查
   - 编译检查
   - Clippy 检查
   - 测试套件
   - 文档生成

2. **人工审查**：至少一位维护者会审查
   - 代码正确性
   - 架构设计
   - 测试覆盖
   - 文档质量
   - 性能影响

3. **反馈处理**：
   - 及时回复审查意见
   - 在同一分支上提交修改
   - 每次修改后更新 commit message
   - 通过 `git push` 更新 PR

4. **合并条件**：
   - ✅ 所有 CI 检查通过
   - ✅ 至少一个维护者批准
   - ✅ 没有未解决的讨论
   - ✅ 与 main 分支无冲突

### 审查关注点

**代码质量：**
- 逻辑正确且健壮
- 错误处理完善（统一使用 `EvalError`）
- 边界条件考虑周全
- 无明显的性能问题
- 代码可读性好

**设计质量：**
- API 设计符合 Rust 惯例
- 模块职责清晰（api / engine / metrics / provider / prompts / storage 分层）
- 抽象层次合理
- 向后兼容性

**测试质量：**
- 覆盖核心逻辑
- 包含错误场景
- 测试案例有代表性
- LLM 依赖用例使用 mock 或 `#[ignore]`

**文档质量：**
- 公共 API 有完整中文文档
- 示例代码可运行
- 复杂逻辑有注释
- 更新了相关文档（README / config / .env.example）

**安全性：**
- 输入验证充分
- 无明显的安全漏洞
- Unsafe 代码有充分说明
- API 密钥仅通过环境变量注入，不写入配置文件
- 依赖项安全可信

### 响应审查

**回复审查意见：**
```markdown
> 审查者意见：这里应该处理 None 的情况

已修改，添加了 None 处理逻辑。相关 commit: abc1234
```

**请求重新审查：**
```bash
# 修改代码后
git add .
git commit -m "fix(scope): 根据审查意见修复 XXX"
git push

# 在 PR 页面请求重新审查
gh pr review <pr-number> --request-changes
```

## Bug 报告

使用 GitHub Issues 报告 Bug，请提供完整信息：

### Bug 报告模板

```markdown
## Bug 描述

清晰简洁地描述问题

## 复现步骤

1. 执行命令 '...'
2. 调用接口 'POST /v1/eval' 参数 '...'
3. 观察结果 '...'

## 期望行为

应该发生什么

## 实际行为

实际发生了什么

## 最小复现代码

```rust
// 尽可能简化的复现代码
fn main() {
    // ...
}
```

## 错误信息

```
完整的错误输出或堆栈跟踪
```

## 环境信息

- OS: [e.g. Ubuntu 20.04]
- Rust 版本: [e.g. 1.85.0]
- 工具版本: [e.g. cargo 1.85.0]
