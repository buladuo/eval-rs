# 其他 LLM / RAG 评估框架调研

> 调研对象：除 Ragas 外主流的开源 / 云评估框架（截至 2026-07）。
> 侧重：与 `eval-rs`（Rust 评测框架）相关的指标设计思路、评估维度、机制（LLM-as-a-Judge / 规则 / 可观测）、提示词方案。
> 关联文档：`docs/ragas_metrics.md`（Ragas 指标已单独详述）。

---

## 一、总览对照表

| 框架 | 定位 | 核心机制 | 代表性维度 | 参考依赖 | 与 Ragas 关系 |
|---|---|---|---|---|---|
| **DeepEval** | 一体化 LLM 评测（含 CI） | LLM-as-a-Judge + 规则 | RAG / 安全 / Agent / 对话 / 自定义 | 多可配置 | 含 `ragas.py` 直接复刻 Ragas 指标 |
| **TruLens** | RAG 三元组 + 追踪 | LLM-as-a-Judge + 可解释 | Context Relevance / Groundedness / Answer Relevance | 部分无参考 | 维度与 Ragas 高度重叠 |
| **Arize Phoenix** | 可观测 + Eval（OpenTelemetry） | LLM 分类 + embedding 分析 | 检索 / QA / 忠实 / 毒性 / 摘要 | 部分无参考 | 复用 Ragas 风格指标 |
| **Giskard** | 漏洞扫描 + 红队 + RAGET | LLM 生成对抗用例 | 鲁棒性 / 安全 / 公平性 / RAG | 有/无 | 偏"测试"而非"打分" |
| **LangSmith** | 平台化评测 + 数据集 | 自定义 evaluator（任意） | 任意（用户定义） | 可配置 | 通用 harness，本身不定义指标 |
| **PromptFoo** | Prompt/模型对比 + 红队 | 断言 + LLM judge | 任意（YAML 断言） | 可配置 | 偏实验编排 |
| **Braintrust** | 评测平台 + autoevals | 25+ 预置 scorer | 事实性 / 安全 / RAG / 摘要 | 可配置 | 预置 scorer 与 Ragas 类似 |
| **Galileo** | 生产可观测 + 预置指标 | 预置评测 + 轨迹 | 忠实 / 上下文 / 安全 | 部分无参考 | 偏生产监控 |
| **Langfuse** | 追踪 + 评测（开源） | 自定义 / LLM judge | 任意（trace 级） | 可配置 | 通用 harness |
| **OpenAI Evals / ARES / Continuous Eval** | 学术/专项 | 规则 / LLM / 程序化 | 专项 benchmark | 多需参考 | 补充性 |

---

## 二、DeepEval（Confident AI，Apache 2.0，~16k★）

最"全家桶"的开源评测框架，内置 **45 个指标模块**、50+ 开箱即用指标，且提供 pytest 集成、CI、Confident AI 云平台。

### 指标模块清单（按类别分组，源自 `deepeval/metrics` 目录树）
**RAG 检索+生成**
- `answer_relevancy`：答案与问题相关度（反推问题法，与 Ragas 同思路）。
- `contextual_relevancy`（= Ragas Context Precision 思路）：检索上下文与问答的相关度。
- `contextual_precision` / `contextual_recall`：检索精确率/召回（对齐 Ragas）。
- `faithfulness`：答案被检索上下文支撑比例（NLI，对齐 Ragas）。
- `hallucination`：幻觉比例（与 faithfulness 互补，越低越好）。
- `summarization`：摘要质量。
- 对话版 RAG：`turn_contextual_precision` / `turn_contextual_recall` / `turn_contextual_relevancy` / `turn_faithfulness` / `turn_relevancy`（多轮）。

**安全 / 责任（Safety）**
- `toxicity`：毒性；`bias`：偏见；`pii_leakage`：PII 泄露；`misuse`：滥用风险；`non_advice`：是否给出不应有的建议；`role_violation`：角色越界；`prompt_alignment`（对齐系统提示）。

**Agent / 工具（Agentic）**
- `task_completion`：任务完成度；`goal_accuracy`：目标准确；`tool_correctness` / `tool_use` / `tool_permission`：工具调用正确/使用/权限；`mcp` / `mcp_use_metric`：MCP 协议；`plan_adherence` / `plan_quality`：规划依从/质量；`step_efficiency`：步骤效率；`topic_adherence`：主题依从；`role_adherence`：角色依从；`agent_loop_detection`：循环检测。

**对话（Conversation）**
- `conversation_completeness`：对话完成度；`knowledge_retention`：知识记忆；`conversational_g_eval` / `conversational_dag`：对话级 G-Eval / DAG。

**自定义 / 通用**
- `g_eval`：G-Eval（LLM-as-a-Judge + CoT，按任意标准打分，最通用）；`arena_g_eval`：Arena 式对战评分；`dag`：DAG 确定性评分（LLM 决策树）；`prompt_alignment`：提示对齐。

**非 LLM / 结构化**
- `exact_match`：精确匹配；`pattern_match`：模式匹配；`json_correctness`：JSON 正确性；`argument_correctness`：论点正确性。

**多模态**：`multimodal_metrics`。

**社区**：`community`（社区贡献指标）。

> 注意：DeepEval 源码中保留了 `ragas.py` 模块，直接复刻 Ragas 指标，二者指标高度同构。

### 提示词方案
- 每个指标模块内嵌 `templates/<MetricClass>/<method>.txt` 提示词模板（运行时编译进 `templates.json`），可用 `deepeval translate` 做多语言。
- **G-Eval** 是代表性"自定义 LLM judge"：提供 `evaluation_steps`（CoT 步骤）+ `evaluation_params`，框架拼装 system/user prompt，LLM 输出 1~10 分（由 `self_consistency` 多次采样取均值）。
- 机制上与 Ragas 的 `PydanticPrompt` 类似，但 DeepEval 更偏"分数化"，Ragas 偏"标签化（0/1、0~4）"。

---

## 三、TruLens（TruEra / Snowflake，开源）

以 **RAG Triad（三元组）** 著称，在 RAG 架构的每条边上各做一个评估，并强调"可解释性"（每个分数都能回溯到具体 evidence）。

### 三个核心指标
1. **Context Relevance（上下文相关性）**
   - 定义：检索到的每段 chunk 是否与用户查询相关。
   - 维度：检索质量。
   - 机制：LLM 对每 chunk 打相关度，可解释（定位到具体 chunk）。
   - 类型：LLM 型，无参考（仅需 user_input + retrieved_contexts）。
2. **Groundedness（有据性 / 忠实度）**
   - 定义：回答是否立足于检索上下文（与 Faithfulness 等价概念）。
   - 机制：将响应拆为独立 claim，在检索上下文中搜索每条 claim 的证据支撑。
   - 维度：生成忠实度。类型：LLM 型，无参考。
3. **Answer Relevance（答案相关性）**
   - 定义：最终响应是否有助于回答用户原始问题。
   - 机制：评估响应与 user_input 的相关性。
   - 维度：生成相关性。类型：LLM 型，无参考。

### 提示词方案
- TruLens 的 feedback 函数（如 `Groundedness` 用 `GroundednessProvider`，底层为 RAG Triad 提示）通过"claim ↔ context 证据"配对实现，提示词遵循"逐 claim 检索证据、判定是否被支持"的结构，与 Ragas Faithfulness 的 NLI 思路一致，但更强调证据回溯可视化。

### 特点
- 不只给分，还提供 **app 级 / 链级 trace 与 feedback dashboard**；适合定位"幻觉来自检索还是生成"。
- 可作为 LangChain / LlamaIndex 的 feedback 回调嵌 production。

---

## 四、Arize Phoenix（开源，OpenTelemetry 原生）

定位"生产级可观测 + 评测"，通过 OpenInference 规范采集 LLM 调用链，并对检索与生成做评测，强调 **embedding 空间诊断**。

### 预置 Evaluators（常见）
- **Retrieval Evaluators**：检索文档相关性（LLM 判相关/不相关）、`RAG Retriever` 类指标（含 `llm_rag_prompt_relevance` 等）。
- **QA / Correctness**：QA 正确性（`QACorrectness`）、Hallucination（`Hallucination`）。
- **Relevance / Groundedness**：`Relevance`（response↔问题/上下文）、`Groundedness`（忠实度）。
- **Toxicity**：毒性；**Summarization**：摘要；**Code**：代码评测；**Conversation**：对话类。
- 也内置可直接调用 **Ragas 指标**（Phoenix 官方 cookbook 用 Ragas 做 retrieval evals），说明二者指标集高度重叠。

### 机制与提示词
- 多数 evaluator 是"prompt LLM 分类（relevant/irrelevant、grounded/not 等）"的封装，提示词为分类式指令 + 期望输出 schema。
- 独特价值：**embedding 投影可视化**（在向量空间标出 query 与 retrieved docs 的聚类/离群），用距离阈值做非 LLM 的检索诊断。
- 类型：LLM 分类型 + 嵌入型 + 规则型混合。

---

## 五、Giskard（开源 + 企业版）

偏"**测试 / 红队**"而非"连续打分"，强调发现模型/agent 的脆弱面。

### 能力维度
- **Scan Vulnerabilities（漏洞扫描）**：从一句话描述自动生成对抗测试用例，覆盖鲁棒性（扰动）、公平性（人口统计）、安全（提示注入/越狱）、数据泄露等。
- **RAGET（RAG Evaluation Toolkit）**：针对 RAG 生成知识库探测、上下文泄露、检索失败等测试。
- **Test Suites + pytest**：将扫描结果固化为可回归的测试套件，接入 CI。
- **Red Teaming**：面向 agent 的对抗式交互测试。

### 机制与提示词
- LLM 生成对抗 prompt + 执行 + 判定（是否触发不良输出）。提示词方案为"攻击生成模板 + 防御判定模板"，更偏安全领域，可自定义对抗策略。
- 类型：LLM 生成 + 规则判定混合；通常**有参考/有预期**用于回归断言。

---

## 六、LangSmith（LangChain，云/自托管）

定位**评测平台 / harness**，本身不定义固定指标，而是提供"数据集 + 目标函数 + 评估器"三件套。

### 评估器（Evaluator）类型
- **Criteria eval**：用 LLM 按命名标准（correctness、helpfulness、conciseness 等）打分（LLM-as-a-Judge）。
- **Custom evaluator**：任意 Python/JS 函数，返回 score + 理由；可挂多个。
- **String / Exact match、Embedding similarity、JSON 校验**等非 LLM 评估器。
- 生产 trace 可自动跑 evaluator（安全、格式、质量启发式、无参考 LLM judge）。

### 特点
- 强在**实验对比、数据集管理、CI 闭环**，指标完全由用户定义或复用 Ragas/DeepEval 指标。
- 提示词方案：用户自写 system prompt + 评分 schema，无内置评分模板（与 Ragas 的封装式 prompt 形成对比）。

---

## 七、PromptFoo（开源）

侧重点：**prompt / 模型 A/B 对比 + 红队**，YAML 驱动断言。

### 机制
- **Assertions（断言）**：在 YAML 中写 `assert` 列表（`equals`、`contains`、`llm-rubric`、`model-graded-closedqa`、`similar` 等），支持 50+ 模型 provider。
- **Red teaming**：内置越狱/敏感话题对抗生成与检测。
- 输出矩阵视图（多个 prompt × 多个 input）对比通过率、成本、延迟。

### 类型
- 规则断言 + LLM 评分（rubric / closedqa）混合；提示词方案由用户以断言表达式定义，框架提供 `llm-rubric` 等标准化 judge 模板。

---

## 八、其他值得关注

- **Braintrust（autoevals）**：25+ 预置 scorer（factuality、security、moderation、summarization、translation、battle、RAG context 指标等），开源 scorer 可本地运行，偏实验迭代。
- **Galileo**：生产可观测 + 预置评测（Chainpoll/Faithfulness、Context Adherence、Toxicity 等），偏生产监控与告警。
- **Langfuse**：开源 tracing + 评测，支持自定义 / LLM judge，trace 级评测。
- **OpenAI Evals**：JSON/模型分级的评测规范，偏 benchmark 式。
- **ARES**：学术 RAG 自动评估（用 LLM 生成合成数据训练轻量分类器做 Context Relevance / Answer Faithfulness / Answer Relevance）。
- **Continuous Eval（Relari）**：程序化 RAG/Agent 评测，强调可复现与分组件指标（检索命中、噪声、工具调用准确率等）。

---

## 九、对 eval-rs 的启示

1. **指标集高度收敛**：DeepEval/TruLens/Phoenix 的 RAG 指标与 Ragas 几乎同构（Context Relevance/Precision/Recall、Faithfulness/Groundedness、Answer Relevancy、Hallucination）。`eval-rs` 已规划的 Ragas 指标可"一套实现，多框架对齐命名"。
2. **差异化维度值得借鉴**：
   - **安全类**：toxicity / bias / pii_leakage / prompt_injection（DeepEval、LangSmith、PromptFoo 均有），Ragas 弱。
   - **Agent 类**：tool_correctness / task_completion / plan_adherence（DeepEval 已丰富），与 eval-rs 的 Agent 指标规划一致。
   - **对话/多轮**：turn_* 类指标（DeepEval）、knowledge_retention。
   - **红队/鲁棒性**：Giskard 的对抗扫描思路（生成扰动/越狱用例 + 判定）。
3. **提示词工程共性**：各框架 LLM 指标都遵循"指令 + 输入变量 + 输出 schema（分数/标签/理由）"的模板结构，与 Ragas 的 `PydanticPrompt` 一致；eval-rs 的 `src/prompts` 可统一抽象为 `PromptTemplate { instruction, input_schema, output_schema, few_shot }`。
4. **可观测/CI**：LangSmith、Phoenix、PromptFoo、Giskard 都强调 trace + 数据集 + CI 回归；若 eval-rs 面向生产，建议后续补充 dataset 管理与结果导出（JSON/CSV）。
