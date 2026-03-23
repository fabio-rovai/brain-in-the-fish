<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>基于 Rust 的 MCP 服务器，用于通用文档评估——认知建模的 AI 智能体，以 OWL 本体为骨架，配合 SNN 反幻觉验证。</strong>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/tests-239%20passing-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

<p align="center">
  <a href="README-CN.md">中文</a> | <a href="README.md">English</a>
</p>

---

## 问题所在

此前有两个系统尝试过多智能体文档评估，但均未达到理想效果。

**MiroFish** 赋予了鱼群——智能体围绕文档展开辩论并最终收敛至一个预测结果。但 MiroFish 的智能体本质上是无状态的 LLM 提示词。它们在轮次间没有记忆、没有结构化认知，评阅内容与评分之间也缺乏形式化的关联。基于 LLM 集群的预测天然容易产生幻觉：智能体编造看似合理的理由，却未将其锚定在文档的实际内容上。当 MiroFish 的智能体预测"该政策将减少 50% 的投诉"时，不会进行任何证据检查——预测的可信度取决于 LLM 的温度参数设置。

**AgentSociety** 赋予了智能体心智——马斯洛需求、计划行为理论、信任动态。但这套认知模型存储在 Python 字典中，对推理过程不透明，无法用 SPARQL 查询，无法在辩论轮次间进行差异比对，也无法与任何外部知识系统互操作。心智确实存在，但无人能审视它。

两个系统有一个更深层的共同缺陷：在所提问题与所发现证据之间，缺乏结构化、可审计的映射关系。分数凭空出现，但从文档内容到评估标准再到智能体判断的推理链条是隐式的、不可复现的。

## 解决方案

Brain in the Fish 为心智赋予了骨架——一个结构化、可查询、可比对的 OWL 本体基底，智能体不仅使用它，更存在于其中。

**三套本体，一张图。** 文档本体、评估标准本体和智能体本体以 OWL 三元组的形式共存于 Oxigraph 存储中（通过 [open-ontologies](https://github.com/fabio-rovai/open-ontologies)）。每一个章节、论断、评估标准、评分标准等级、智能体信念、马斯洛需求和信任权重都是一等 RDF 节点。

**评估优于预测。** MiroFish 预测分数应该是多少。Brain in the Fish 则根据显式的评估标准来评估文档实际包含的内容。评估从根本上比预测更可靠，因为证据就是文档本身——它是具体的、在场的、可验证的。系统不做猜测，而是映射、评分、论证。

**智能体认知即本体。** 每个评估智能体的马斯洛需求、信任关系和领域专长都是 OWL 个体。当一个智能体在辩论质疑后对同事的信任发生变化时，这个变化就是一次三元组更新——可查询、可比对、可审计。

**本体对齐将文档映射到评估标准。** `onto_align` 在文档章节与评估标准之间生成数学化的映射。在评分开始前就能识别覆盖缺口。没有任何评估标准会被无声忽略。

**预测可信度，而非预测。** MiroFish 预测未来。Brain in the Fish 评估文档中的预测是否可信。它提取每一个预测目标、承诺和成本估算，然后根据文档自身的证据基础逐一检验。"减少 50% 的投诉"会基于支撑该数字的证据获得可信度评分——而非基于 LLM 认为会发生什么。

**版本化辩论。** 每一轮辩论产生新的评分三元组。轮次间的 `onto_diff` 精确揭示哪些智能体改变了立场、变化幅度多大、以及原因是什么。漂移速度衡量收敛程度。整个审议过程可从图状态完整复现。

## 对比

| 特性 | MiroFish | AgentSociety | Brain in the Fish |
|------|----------|--------------|-------------------|
| 智能体认知 | 无状态 LLM 提示词 | Python 字典中的马斯洛 + TPB | Oxigraph 中作为 OWL 个体的马斯洛 + TPB |
| 证据基础 | LLM 生成论证 | LLM 生成论证 | 通过本体对齐将文档内容映射到评估标准 |
| 辩论追踪 | 轮次计数器 + 文本日志 | 轮次计数器 + JSON 状态 | 版本化 RDF 三元组 + `onto_diff` + 漂移速度 |
| 可复现性 | 非确定性 | 非确定性 | 每轮确定性图状态，可用 SPARQL 查询 |
| 跨评估学习 | 无 | 无 | Turtle 导出支持跨会话分析 |
| 预测处理 | 智能体凭空预测未来 | 未涉及 | 从文档中提取预测，根据证据评估可信度 |
| 运行时依赖 | Python + 多个 LLM API | Python + LLM API | 单一二进制文件 Rust，内嵌 Oxigraph |
| 部署复杂度 | 多服务 Python 技术栈 | 多服务 Python 技术栈 | `cargo build` 生成一个二进制文件 |

## 工作原理

评估流水线分 10 个阶段运行：

1. **摄入** -- 从 PDF（或纯文本）中提取文本，按标题检测拆分为章节，在 Oxigraph 中构建文档本体的 RDF 三元组。

2. **加载评估标准** -- 选择或生成评估框架（学术评分标准、招标 ITT 标准、通用质量框架）。每个评估标准、评分标准等级和权重都成为评估标准本体中的 OWL 个体。

3. **预测提取** -- 提取定量目标、成本估算、时间线、比较声明和承诺。根据文档自身的证据基础评估每项预测的可信度。标记缺乏证据支撑的预测。

4. **生成智能体面板** -- 从意图字符串中检测评估领域，生成 3-5 名专家智能体和一名主持人。每个智能体的认知模型（马斯洛需求、信任权重、领域专长）作为智能体本体加载。

5. **对齐** -- 使用关键词重叠（未来将通过 open-ontologies 的语义嵌入）将文档章节映射到评估标准。识别文档内容未覆盖某项评估标准的缺口。

6. **评分（第 1 轮）** -- 每个智能体独立为每项评估标准打分。评分提示包含智能体角色、评估标准的评分标准以及相关文档章节。分数以 RDF 三元组记录。

7. **检测分歧** -- 找出分数差异超过阈值的评估标准-智能体对，这些将成为辩论目标。

8. **辩论** -- 质疑方智能体针对目标分数构建基于证据的论证。被质疑方进行辩护或修正。信任权重根据说服结果更新。每一轮产生新的评分三元组。

9. **主持调停** -- 当漂移速度低于阈值（达成收敛）时，主持人计算信任加权的共识分数，识别离群异议，生成最终调停结果。

10. **生成报告** -- 生成结构化的 Markdown 报告，包含执行摘要、评分卡表格、覆盖缺口分析、完整辩论记录、改进建议和面板总结。将完整评估导出为 Turtle RDF 以供跨会话分析。

## 反幻觉：SNN 验证层

MiroFish 的核心弱点在于智能体评分是 LLM 输出——看似合理的文本，却没有数学基础。一个智能体可以为文档中毫无支撑证据的评估标准"论证"出 9/10 的高分。这本质上是附带了置信度分数的幻觉。

Brain in the Fish 通过一个**脉冲神经网络（SNN）**验证层来解决这个问题，该验证层位于本体证据与 LLM 评分之间。SNN 是确定性的：给定相同的证据，它总是产生相同的分数。LLM 是随机的：它提供定性判断。两者结合使得幻觉可以被检测。

### SNN 工作原理

每个评估智能体拥有一个神经网络，每项评估标准对应一个**神经元**。来自文档本体的证据生成**输入脉冲**：

| 证据类型 | 脉冲强度 | 示例 |
| -------- | -------- | ---- |
| 量化数据 | 0.8-1.0 | "富时100指数上涨45%" |
| 可验证声明 | 0.6-0.8 | "英格兰银行购入8950亿英镑资产" |
| 引用文献 | 0.5-0.7 | "(Bernanke, 2009)" |
| 一般性声明 | 0.3-0.5 | "量化宽松作为稳定工具是有效的" |
| 章节对齐 | 0.2-0.4 | 章节标题与评估标准匹配 |

神经元使用**漏积分-发放**动力学：

- 脉冲在膜电位中累积
- 电位随时间衰减（泄漏）
- 当电位超过**阈值**（由评分标准派生）→ 神经元发放
- 发放频率映射为分数
- 图中无证据 = 无脉冲 = 不发放 = 零分

### 混合评分：SNN + LLM

最终分数混合两个层的输出，按 SNN 置信度加权：

```text
final_score = snn_score × snn_weight + llm_score × llm_weight
```

当 SNN 置信度高（证据充分）时，SNN 主导评分。当置信度低（证据稀疏）时，LLM 补充——但如果 LLM 评分显著高于证据支撑的水平，将触发**幻觉标记**。

```text
LLM 评分 9/10。SNN 评分 2/10（仅收到 2 个弱脉冲）。
→ hallucination_risk = true
→ "警告：LLM 评分显著高于证据支撑水平。"
```

### 辩论即侧向抑制

在辩论轮次中，来自其他智能体的质疑对目标智能体的神经元施加**侧向抑制**。这降低了膜电位，需要更多证据才能维持高分。信任权重调节智能体间的脉冲传输——高度受信任的质疑者产生更强的抑制信号。

### 理论基础：ARIA 安全保障 AI

该架构与 [ARIA 的 5900 万英镑安全保障 AI 计划](https://www.aria.org.uk/programme-safeguarded-ai/)（由 davidad 领导，与 Yoshua Bengio、Stuart Russell 和 Max Tegmark 共同撰写）保持一致。他们在["迈向有保障的安全 AI"](https://arxiv.org/abs/2405.06624)中的论点：**不要让 LLM 变得确定性——而是让验证变得确定性。**

| ARIA 框架 | Brain in the Fish |
| --------- | ----------------- |
| 世界模型（现实的形式化描述） | 本体（OWL 知识图谱） |
| 安全规范（可接受的输出） | 评分标准等级 + SNN 阈值 |
| 确定性验证器（证明检查器） | SNN（相同脉冲 → 相同分数，始终如此） |
| 证明证书（推理痕迹） | 脉冲日志 + onto_lineage（可审计的证据路径） |

LLM 生成定性判断。SNN 提供确定性、可审计的验证门控。本体提供形式化的世界模型。三者结合，实现了 ARIA 的文档评估"守门人"架构。

## 快速开始

### 前提条件

- Rust 1.85+（edition 2024）
- [open-ontologies](https://github.com/fabio-rovai/open-ontologies) 克隆在本仓库旁边

```bash
git clone https://github.com/fabio-rovai/open-ontologies.git
git clone https://github.com/fabio-rovai/brain-in-the-fish.git
cd brain-in-the-fish
cargo build --release
```

### 连接 Claude

Brain in the Fish 是一个 MCP 服务器。将其添加到你的 Claude Code 或 Claude Desktop 配置中：

**Claude Code (~/.claude.json):**

```json
{
  "mcpServers": {
    "brain-in-the-fish": {
      "command": "/path/to/brain-in-the-fish",
      "args": ["serve"]
    }
  }
}
```

**Claude Desktop (claude_desktop_config.json):**

```json
{
  "mcpServers": {
    "brain-in-the-fish": {
      "command": "/path/to/brain-in-the-fish",
      "args": ["serve"]
    }
  }
}
```

然后向 Claude 提问：

> "根据绿皮书标准评估这份政策文件"

Claude 将通过调度子智能体调用 eval_* MCP 工具来编排评估。每个子智能体以专家角色进行评分。SNN 验证层根据知识图谱中的证据验证每一个评分。

### 独立模式（无需 Claude）

仅使用确定性评估，无需 LLM 判断：

```bash
brain-in-the-fish evaluate document.pdf --intent "mark this essay for A-level"
```

运行 SNN 评分管道——基于证据、确定性、无需 API 密钥。输出包括 Markdown 报告、Turtle RDF 导出、交互式图谱和编排任务（Claude 可稍后接手）。

### 验证

```bash
cargo test
```

## 使用方法

### 作为 MCP 服务器（推荐）

```bash
# 启动 MCP 服务器
brain-in-the-fish serve

# Claude 处理一切——只需提问：
# "评估这篇 A-level 经济学论文"
# "审查这份合同的 GDPR 合规性"
# "评估这份 NHS 临床治理报告"
# "审计这份调查方法论"
```

### 作为 CLI 工具（确定性模式）

```bash
# 使用 SNN 评分（无需 LLM）
brain-in-the-fish evaluate essay.pdf --intent "mark this economics essay"

# 使用自定义评估标准
brain-in-the-fish evaluate policy.pdf --intent "evaluate against Green Book" --criteria rubric.yaml

# 输出到指定目录
brain-in-the-fish evaluate report.pdf --intent "audit this clinical report" --output ./results
```

### 输出文件

| 文件 | 描述 |
|------|------|
| `evaluation-report.md` | 完整评分卡、缺口分析、辩论记录、改进建议 |
| `evaluation.ttl` | Turtle RDF 导出，用于跨评估分析 |
| `evaluation-graph.html` | 交互式层次知识图谱 |
| `orchestration.json` | Claude 增强评分的子智能体任务 |

## 通用评估

系统在你告知之前并不知道要评估什么。同一引擎通过将三套本体适配到具体领域来处理任意类型的文档。

| 用例 | 文档本体 | 评估标准本体 | 智能体面板 |
|------|----------|-------------|-----------|
| 批改学生论文 | 段落、论点、引用、论题 | 评分标准、分数线、学习成果 | 学科专家、写作专家、批判性思维评估员 |
| 评估政策文件 | 目标、措施、影响预测 | 绿皮书评估、影响标准、利益相关方需求 | 政策分析师、利益相关方代表、实施专家 |
| 审查合同 | 条款、义务、术语、定义 | 法律清单、风险标准、监管要求 | 法律审查员、合规官、商务分析师 |
| 分析调查结果 | 回应主题、方法论、人口统计 | 研究问题、效度标准 | 统计学家、研究设计师、伦理审查员 |
| 评估招标方案 | 章节、论断、证据、案例研究 | ITT 标准、权重、通过/不通过阈值 | 采购负责人、领域专家、社会价值倡导者、财务评估员 |

## 适用领域

Brain in the Fish 可评估任何领域的文档。引擎根据评估意图自动调整其评估标准本体、智能体面板和评分标准。

| 领域 | 文档类型 | 框架与标准 |
|------|---------|-----------|
| **教育** | 论文、课程作业、学位论文、考试答卷 | AQA、Ofsted、布鲁姆分类法、QAA |
| **医疗** | 临床治理报告、患者安全审计、护理计划 | CQC、NICE 指南、NHS England |
| **政府** | 政策文件、影响评估、商业计划书 | 绿皮书、紫皮书、公务员能力框架 |
| **法律** | 合同、合规报告、条款和条件 | GDPR、消费者权益法、监管清单 |
| **研究** | 调查方法论、伦理申请、同行评审 | ESRC 框架、研究委员会标准 |
| **采购** | 招标方案、提案、ITT 回复 | PPN、社会价值法案、框架特定标准 |
| **通用** | 任何文档对任何自定义标准 | 自定义评分标准、加权评分、通过/不通过阈值 |

## 架构

所有模块编译为单一二进制文件。没有微服务，没有 Python，本体引擎无需网络调用。

| 模块 | 用途 | 代码行数 |
|------|------|---------|
| `types` | 核心评估领域类型（Document、Criteria、Agent、Score、Session） | 289 |
| `ingest` | PDF 文本提取、章节拆分、文档本体 RDF 生成 | 557 |
| `criteria` | 评估框架加载（7 个内置 + YAML/JSON）、评估标准本体 RDF 生成 | 1,360 |
| `agent` | 智能体认知模型（马斯洛 + 信任）、智能体本体 RDF、面板生成 | 840 |
| `scoring` | SPARQL 查询、评分记录、为子智能体生成评分提示 | 1,278 |
| `debate` | 分歧检测、质疑提示、漂移速度、收敛判定 | 875 |
| `moderation` | 信任加权共识、离群检测、综合结果计算 | 678 |
| `report` | Markdown 报告生成、Turtle RDF 会话导出 | 713 |
| `server` | MCP 服务器，提供 12 个 eval_* 工具（rmcp，stdio + HTTP 传输） | 905 |
| `main` | CLI 入口（clap），evaluate 和 serve 子命令，完整流水线编排 | 737 |
| `snn` | 脉冲神经网络评分——确定性证据驱动验证 | 752 |
| `llm` | Claude API 客户端，用于子代理增强评分（可选） | 320 |
| `alignment` | 文档章节与评估标准之间的本体对齐（7 个结构信号） | 843 |
| `research` | 证据收集与综合的研究管道 | 493 |
| `memory` | 跨评估轮次的智能体记忆持久化 | 315 |
| `visualize` | 评估可视化、交互式图 HTML、图表生成 | 2,520 |
| `validate` | 15 项确定性文档验证检查，向 SNN 提供脉冲/抑制信号 | 2,147 |
| `batch` | 批量评估多个文档 | 602 |
| `belief_dynamics` | 基于评估发现更新马斯洛需求 | 166 |
| `epistemology` | 基于经验、规范和证言的有根据信念 | 347 |
| `philosophy` | 康德主义、功利主义和美德伦理分析 | 316 |
| `orchestrator` | 为 Claude 增强评分生成子智能体任务 | 292 |
| `semantic` | 通过嵌入进行语义相似度计算（TextEmbedder + VecStore） | 154 |
| `lib` | 模块声明 | 22 |

**总计：约 17,520 行 Rust 代码，共 24 个模块。**

## MCP 工具

MCP 服务器暴露 12 个工具，用于以编程方式编排评估流程：

| 工具 | 描述 |
|------|------|
| `eval_status` | 服务器状态、版本、会话状态、三元组数量 |
| `eval_ingest` | 摄入 PDF 并构建文档本体 |
| `eval_criteria` | 加载评估框架（通用、学术、政策、临床、法律） |
| `eval_align` | 在文档章节与评估标准之间运行本体对齐 |
| `eval_spawn` | 根据意图生成评估智能体面板 |
| `eval_score_prompt` | 为指定的智能体-评估标准对生成评分提示 |
| `eval_record_score` | 将智能体的评分记录到图存储中 |
| `eval_debate_status` | 当前轮次的分歧、漂移速度和收敛状态 |
| `eval_challenge_prompt` | 生成一个智能体质疑另一个智能体的提示 |
| `eval_scoring_tasks` | 为智能体面板生成所有评分任务，作为编排器可分发的提示 |
| `eval_whatif` | 模拟文本更改并估算其对分数的影响 |
| `eval_report` | 生成包含调停和共识的完整评估报告 |

## 基于 open-ontologies 构建

Brain in the Fish 不是 [open-ontologies](https://github.com/fabio-rovai/open-ontologies) 的分支。它是一个依赖 crate，以库的形式消费 open-ontologies。

```toml
open-ontologies = { path = "../open-ontologies", features = ["embeddings"] }
```

它使用 `GraphStore` 进行三元组存储和 SPARQL 查询，`Reasoner` 进行推理，`Aligner` 进行本体对齐，`Embedder` 进行语义相似度计算——全部作为进程内 Rust 函数调用。零网络开销。无序列化边界。本体引擎与评估逻辑运行在同一地址空间中。

## 测试

239 个测试覆盖全部 24 个模块：摄入、评估标准加载、智能体生成、评分、辩论机制、调停、报告生成、SNN 验证、对齐、验证、信念动态、认识论、哲学、编排、批处理和 MCP 服务器工具。

```bash
cargo test
```

## 贡献

请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解贡献指南。

## 致谢

- [MiroFish](https://github.com/666ghj/MiroFish) — 多智能体群体预测引擎，启发了本项目的智能体辩论架构
- [AgentSociety](https://github.com/tsinghua-fib-lab/AgentSociety) — 清华大学认知智能体仿真，启发了马斯洛 + 计划行为理论模型
- [open-ontologies](https://github.com/fabio-rovai/open-ontologies) — 提供知识图谱骨架的 OWL 本体引擎
- [epistemic-deconstructor](https://github.com/NikolasMarkou/epistemic-deconstructor) — Nikolas Markou 的贝叶斯假设追踪与证伪优先认识论，启发了校准置信度评分与似然比上限
- [ARIA Safeguarded AI](https://www.aria.org.uk/programme-safeguarded-ai/) — 5900万英镑的守门人架构（世界模型 + 确定性验证器 + 证明证书），验证了 SNN + 本体验证方法

## 许可证

MIT
