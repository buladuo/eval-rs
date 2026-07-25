# 贡献指南

感谢你对本项目的关注！我们欢迎所有形式的贡献。

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

- Rust 1.70+ （推荐使用 rustup）
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

# 构建项目
cargo build

# 运行测试
cargo test
```

## 项目结构

```
.
├── src/           # 源代码
├── tests/         # 集成测试
├── benches/       # 性能测试
├── examples/      # 示例代码
└── docs/          # 文档
```

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
git checkout -b feature/weizhikong/add-json-parser
git checkout -b fix/weizhikong/unicode-escape-bug
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
- 添加必要的注释和文档
- 为公共 API 编写文档注释（`///`）

### 3. 质量检查

在提交前，**必须**运行以下检查：

```bash
# 1. 代码格式化
cargo fmt --all

# 2. 编译检查
cargo check --all-targets --all-features

# 3. Clippy 静态分析
cargo clippy --all-targets --all-features -- -D warnings

# 4. 运行所有测试
cargo test --all-features

# 5. 检查文档
cargo doc --no-deps --all-features
```

**一键检查脚本：**

可以创建 `scripts/check.sh` 方便执行：

```bash
#!/bin/bash
set -e

echo "🔍 Running cargo fmt..."
cargo fmt --all -- --check

echo "🔍 Running cargo check..."
cargo check --all-targets --all-features

echo "🔍 Running cargo clippy..."
cargo clippy --all-targets --all-features -- -D warnings

echo "🔍 Running cargo test..."
cargo test --all-features

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
模块名或功能域，如：`parser`、`api`、`utils`、`core`

**Subject 主题：**
- 使用祈使句，现在时态："添加功能" 而非 "添加了功能"
- 首字母小写
- 结尾不加句号
- 限制在 50 字符以内

**示例 Commit 1 - Bug 修复：**

```
fix(parser): 修复 JSON 解析器处理转义字符错误

[背景说明]
在解析包含 Unicode 转义序列（如 \uXXXX）的 JSON 字符串时，
解析器会错误地将反斜杠作为普通字符处理，导致解析失败。
问题由用户在 issue #123 中报告，影响所有包含 Unicode 字符的 JSON 数据。

[处理方式]
1. 在词法分析阶段添加转义序列识别逻辑
2. 使用状态机处理 \u 后的 4 位十六进制数字
3. 将识别的 Unicode 码点转换为 UTF-8 字符
4. 添加错误处理，对无效的转义序列返回明确的错误信息

[改动清单]
- src/parser/lexer.rs: 添加 parse_unicode_escape() 函数 (45 行)
- src/parser/lexer.rs: 修改 parse_string() 状态机逻辑，增加转义处理分支
- src/parser/error.rs: 新增 InvalidUnicodeEscape 错误类型
- tests/parser_tests.rs: 新增 10 个 Unicode 转义测试用例
- tests/parser_tests.rs: 新增 5 个错误处理测试用例
- benches/parser_bench.rs: 添加转义字符性能基准测试

[结果结论]
修复后所有测试用例通过，包括原有的 156 个测试和新增的 15 个测试。
性能测试显示对不含转义字符的字符串无影响，含转义字符的解析
速度提升约 15%（优化了字符复制逻辑）。用户报告的问题已解决。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过（修复了 2 个 clippy::needless_borrow 警告）
- cargo test: ✅ 通过 (171/171 tests, +15 new)
- cargo bench: ✅ 通过（性能无回归，转义场景提升 15%）

Closes #123
```

**示例 Commit 2 - 新功能：**

```
feat(api): 添加异步 HTTP 客户端支持

[背景说明]
当前项目只支持同步 HTTP 请求，在处理大量并发请求时性能受限。
用户反馈在微服务场景下需要异步请求能力以提升吞吐量。
参考 issue #234 和 #256 中的讨论。

[处理方式]
1. 基于 tokio 和 reqwest 实现异步 HTTP 客户端
2. 保持与现有同步 API 一致的接口设计
3. 添加 "async" feature flag，默认不启用以保持向后兼容
4. 实现连接池和超时控制
5. 提供 async/await 友好的 API

[改动清单]
- Cargo.toml: 添加 tokio、reqwest 依赖（仅在 async feature 下）
- src/client/async_client.rs: 新增异步客户端实现 (328 行)
- src/client/mod.rs: 导出异步客户端模块
- src/client/config.rs: 扩展配置支持异步选项
- examples/async_example.rs: 添加异步使用示例
- docs/async-guide.md: 新增异步使用指南文档
- tests/async_integration_test.rs: 添加 25 个异步集成测试

[结果结论]
实现了完整的异步 HTTP 客户端功能，API 设计与同步版本保持一致。
性能测试显示在 1000 并发请求场景下，吞吐量提升 3.2 倍，
延迟降低 60%。通过 feature flag 控制，不影响现有用户。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过（检查了 default 和 async features）
- cargo clippy: ✅ 通过（无警告）
- cargo test: ✅ 通过 (196/196 tests, +25 new)
- cargo test --features async: ✅ 通过
- cargo doc: ✅ 通过（文档生成正常）

Closes #234
Closes #256
```

**示例 Commit 3 - 重构：**

```
refactor(core): 简化错误处理机制

[背景说明]
现有错误类型定义分散在多个模块中，错误转换逻辑复杂且重复。
维护困难，添加新错误类型需要修改多处代码。同时错误信息
对用户不够友好，缺少上下文信息。

[处理方式]
1. 使用 thiserror 统一错误定义
2. 将所有错误类型集中到 src/error.rs
3. 实现统一的错误转换 trait
4. 为每个错误添加详细的上下文信息
5. 移除冗余的错误包装代码

[改动清单]
- Cargo.toml: 添加 thiserror 依赖
- src/error.rs: 重构错误类型定义，从 456 行简化到 178 行
- src/parser/mod.rs: 移除本地错误定义，使用统一错误类型
- src/client/mod.rs: 移除本地错误定义，使用统一错误类型
- src/core/mod.rs: 简化错误传播逻辑
- 删除 src/parser/error.rs: 已合并到统一错误模块
- 删除 src/client/error.rs: 已合并到统一错误模块
- tests/error_tests.rs: 更新错误处理测试用例

[结果结论]
错误处理代码总量减少约 35%，错误定义更加清晰统一。
所有错误都包含了足够的上下文信息，便于调试。
API 保持向后兼容，原有错误类型通过 type alias 保留。
编译时间减少约 8%（减少了重复的派生宏展开）。

[质量检查]
- cargo fmt: ✅ 通过
- cargo check: ✅ 通过
- cargo clippy: ✅ 通过（消除了 15 个重复代码警告）
- cargo test: ✅ 通过 (171/171 tests, 无变化)
- cargo build --release: ✅ 通过（二进制大小减少 12KB）
```

### 5. 提交前检查清单

在 `git commit` 之前，确认：

- [ ] 代码已通过 `cargo fmt`
- [ ] 代码已通过 `cargo check`
- [ ] 代码已通过 `cargo clippy` 且无警告
- [ ] 所有测试通过 `cargo test`
- [ ] 为新功能添加了测试
- [ ] 为公共 API 添加了文档注释
- [ ] 更新了 CHANGELOG.md（如果适用）
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
- `feat(api): 添加异步 HTTP 客户端支持`
- `fix(parser): 修复 Unicode 转义字符处理`
- `docs(readme): 更新安装说明`

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
    #[should_panic(expected = "error message")]
    fn test_error_case() {
        function_under_test(invalid_input);
    }

    #[test]
    fn test_error_result() {
        let result = fallible_function(invalid_input);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "expected error message"
        );
    }
}
```

### 集成测试

放置在 `tests/` 目录，测试模块间的交互：

```rust
// tests/integration_test.rs
use your_crate::*;

#[test]
fn test_end_to_end_workflow() {
    // 完整的使用场景测试
    let client = Client::new();
    let result = client.process(input);
    assert!(result.is_ok());
}
```

### 文档测试

在文档注释中编写可执行的示例代码：

```rust
/// 计算两个数的和
///
/// # Examples
///
/// ```
/// use your_crate::add;
///
/// let result = add(2, 3);
/// assert_eq!(result, 5);
/// ```
///
/// # Edge Cases
///
/// ```
/// use your_crate::add;
///
/// // 处理溢出
/// let result = add(i32::MAX, 1);
/// // 根据实际行为编写测试
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

### 属性测试（推荐使用 proptest）

对于复杂逻辑，使用属性测试验证不变量：

```rust
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_reversible_operation(input in any::<i32>()) {
            let encoded = encode(input);
            let decoded = decode(encoded);
            prop_assert_eq!(decoded, input);
        }
    }
}
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_name

# 显示输出（包括 println!）
cargo test -- --nocapture

# 运行忽略的测试
cargo test -- --ignored

# 并行度控制
cargo test -- --test-threads=1

# 生成测试覆盖率报告（需要 tarpaulin）
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --output-dir coverage
```

## 文档规范

### API 文档

所有公共 API **必须**有文档注释：

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
/// - `ErrorType1` - 当条件 X 发生时
/// - `ErrorType2` - 当条件 Y 发生时
///
/// # Examples
///
/// 基本使用：
///
/// ```
/// use your_crate::FunctionName;
///
/// let result = function_name(arg1, arg2)?;
/// assert_eq!(result, expected);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
///
/// 高级用法：
///
/// ```
/// # use your_crate::*;
/// // 更复杂的示例
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
/// # Performance
///
/// 时间复杂度：O(n)
/// 空间复杂度：O(1)
///
/// # See Also
///
/// - [`related_function`] - 相关功能
/// - [`OtherType`] - 相关类型
pub fn function_name(
    param1: Type1,
    param2: Type2,
) -> Result<ReturnType, Error> {
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
//! use your_crate::module_name::*;
//!
//! // 示例代码
//! ```

// 模块代码
```

### 类型文档

```rust
/// HTTP 客户端配置
///
/// 用于配置 HTTP 客户端的各种参数，包括超时、重试、
/// 连接池等。
///
/// # Examples
///
/// ```
/// use your_crate::ClientConfig;
///
/// let config = ClientConfig::builder()
///     .timeout(30)
///     .max_retries(3)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// 请求超时时间（秒）
    pub timeout: u64,

    /// 最大重试次数
    pub max_retries: u32,
}
```

### 文档最佳实践

1. **使用主动语态和祈使句**：
   - ✅ "计算两个数的和"
   - ❌ "这个函数计算两个数的和"

2. **提供可运行的示例**：
   - 所有示例代码都应该能通过 `cargo test`
   - 使用 `# Ok::<(), Error>(())` 处理 Result 返回

3. **使用内部链接**：
   - 使用 `[`Type`]` 链接到其他类型
   - 使用 `[`function_name`]` 链接到函数

4. **说明复杂度和性能特征**：
   - 对性能敏感的 API 说明时间/空间复杂度

5. **标注不稳定 API**：
   ```rust
   #[doc = "⚠️ **不稳定 API**：此接口可能在未来版本中变更"]
   pub fn experimental_feature() {}
   ```

### 生成和检查文档

```bash
# 生成文档
cargo doc --no-deps --all-features

# 在浏览器中打开文档
cargo doc --no-deps --all-features --open

# 检查文档链接
cargo doc --no-deps --all-features 2>&1 | grep warning

# 运行文档测试
cargo test --doc
```

## 性能要求

### 性能基准测试

使用 Criterion.rs 进行性能测试：

```rust
// benches/my_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use your_crate::*;

fn benchmark_function(c: &mut Criterion) {
    c.bench_function("function_name", |b| {
        b.iter(|| {
            function_under_test(black_box(input))
        });
    });
}

fn benchmark_with_setup(c: &mut Criterion) {
    let data = setup_expensive_data();

    c.bench_function("with_setup", |b| {
        b.iter(|| {
            function_under_test(black_box(&data))
        });
    });
}

criterion_group!(benches, benchmark_function, benchmark_with_setup);
criterion_main!(benches);
```

### 运行基准测试

```bash
# 运行所有基准测试
cargo bench

# 运行特定基准测试
cargo bench benchmark_name

# 保存基线
cargo bench -- --save-baseline before_change

# 对比基线
cargo bench -- --baseline before_change

# 生成详细报告
cargo bench -- --verbose
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
- 考虑使用 `Cow<'_, str>` 处理字符串
- 合理使用内联 `#[inline]` 和 `#[inline(always)]`

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
- 错误处理完善
- 边界条件考虑周全
- 无明显的性能问题
- 代码可读性好

**设计质量：**
- API 设计符合 Rust 惯例
- 模块职责清晰
- 抽象层次合理
- 向后兼容性

**测试质量：**
- 覆盖核心逻辑
- 包含错误场景
- 测试案例有代表性
- 性能测试（如需要）

**文档质量：**
- 公共 API 有完整文档
- 示例代码可运行
- 复杂逻辑有注释
- 更新了相关文档

**安全性：**
- 输入验证充分
- 无明显的安全漏洞
- Unsafe 代码有充分说明
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
2. 调用函数 '...'
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
- Rust 版本: [e.g. 1.60.0]
- 工具版本: [e.g. cargo 1.60.0]
