<p align="center">
  <img src="assets/logo.png" alt="Brain in the Fish" width="200" />
</p>

<h1 align="center">Brain in the Fish</h1>

<p align="center">
  <strong>基于 Rust 的通用文档评估引擎——认知建模的 AI 智能体可评估任何文档、服务于任何目的，而其全部心智状态均存在于 OWL 本体之中。</strong>
</p>

<p align="center">
  <img src="https://github.com/fabio-rovai/brain-in-the-fish/actions/workflows/ci.yml/badge.svg" alt="CI" />
  <img src="https://img.shields.io/badge/License-MIT-yellow.svg" alt="License: MIT" />
  <img src="https://img.shields.io/badge/tests-104%20passing-brightgreen" alt="Tests" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange" alt="Rust" />
</p>

<p align="center">
  <a href="README-CN.md">中文</a> | <a href="README.md">English</a>
</p>

---

## 问题所在

此前有两个系统尝试过多智能体文档评估，但均未达到理想效果。

**MiroFish** 赋予了鱼群——智能体围绕文档展开辩论并最终收敛至一个预测结果。但 MiroFish 的智能体本质上是无状态的 LLM 提示词。它们在轮次间没有记忆、没有结构化认知，评阅内容与评分之间也缺乏形式化的关联。基于 LLM 集群的预测天然容易产生幻觉：智能体编造看似合理的理由，却未将其锚定在文档的实际内容上。

**AgentSociety** 赋予了智能体心智——马斯洛需求、计划行为理论、信任动态。但这套认知模型存储在 Python 字典中，对推理过程不透明，无法用 SPARQL 查询，无法在辩论轮次间进行差异比对，也无法与任何外部知识系统互操作。心智确实存在，但无人能审视它。

两个系统有一个更深层的共同缺陷：在所提问题与所发现证据之间，缺乏结构化、可审计的映射关系。分数凭空出现，但从文档内容到评估标准再到智能体判断的推理链条是隐式的、不可复现的。

## 解决方案

Brain in the Fish 为心智赋予了骨架——一个结构化、可查询、可比对的 OWL 本体基底，智能体不仅使用它，更存在于其中。

**三套本体，一张图。** 文档本体、评估标准本体和智能体本体以 OWL 三元组的形式共存于 Oxigraph 存储中（通过 [open-ontologies](https://github.com/fabio-rovai/open-ontologies)）。每一个章节、论断、评估标准、评分标准等级、智能体信念、马斯洛需求和信任权重都是一等 RDF 节点。

**评估优于预测。** MiroFish 预测分数应该是多少。Brain in the Fish 则根据显式的评估标准来评估文档实际包含的内容。评估从根本上比预测更可靠，因为证据就是文档本身——它是具体的、在场的、可验证的。系统不做猜测，而是映射、评分、论证。

**智能体认知即本体。** 每个评估智能体的马斯洛需求、信任关系和领域专长都是 OWL 个体。当一个智能体在辩论质疑后对同事的信任发生变化时，这个变化就是一次三元组更新——可查询、可比对、可审计。

**本体对齐将文档映射到评估标准。** `onto_align` 在文档章节与评估标准之间生成数学化的映射。在评分开始前就能识别覆盖缺口。没有任何评估标准会被无声忽略。

**版本化辩论。** 每一轮辩论产生新的评分三元组。轮次间的 `onto_diff` 精确揭示哪些智能体改变了立场、变化幅度多大、以及原因是什么。漂移速度衡量收敛程度。整个审议过程可从图状态完整复现。

## 对比

| 特性 | MiroFish | AgentSociety | Brain in the Fish |
|------|----------|--------------|-------------------|
| 智能体认知 | 无状态 LLM 提示词 | Python 字典中的马斯洛 + TPB | Oxigraph 中作为 OWL 个体的马斯洛 + TPB |
| 证据基础 | LLM 生成论证 | LLM 生成论证 | 通过本体对齐将文档内容映射到评估标准 |
| 辩论追踪 | 轮次计数器 + 文本日志 | 轮次计数器 + JSON 状态 | 版本化 RDF 三元组 + `onto_diff` + 漂移速度 |
| 可复现性 | 非确定性 | 非确定性 | 每轮确定性图状态，可用 SPARQL 查询 |
| 跨评估学习 | 无 | 无 | Turtle 导出支持跨会话分析 |
| 运行时依赖 | Python + 多个 LLM API | Python + LLM API | 单一二进制文件 Rust，内嵌 Oxigraph |
| 部署复杂度 | 多服务 Python 技术栈 | 多服务 Python 技术栈 | `cargo build` 生成一个二进制文件 |

## 工作原理

评估流水线分 9 个阶段运行：

1. **摄入** -- 从 PDF（或纯文本）中提取文本，按标题检测拆分为章节，在 Oxigraph 中构建文档本体的 RDF 三元组。

2. **加载评估标准** -- 选择或生成评估框架（学术评分标准、招标 ITT 标准、通用质量框架）。每个评估标准、评分标准等级和权重都成为评估标准本体中的 OWL 个体。

3. **生成智能体面板** -- 从意图字符串中检测评估领域，生成 3-5 名专家智能体和一名主持人。每个智能体的认知模型（马斯洛需求、信任权重、领域专长）作为智能体本体加载。

4. **对齐** -- 使用关键词重叠（未来将通过 open-ontologies 的语义嵌入）将文档章节映射到评估标准。识别文档内容未覆盖某项评估标准的缺口。

5. **评分（第 1 轮）** -- 每个智能体独立为每项评估标准打分。评分提示包含智能体角色、评估标准的评分标准以及相关文档章节。分数以 RDF 三元组记录。

6. **检测分歧** -- 找出分数差异超过阈值的评估标准-智能体对，这些将成为辩论目标。

7. **辩论** -- 质疑方智能体针对目标分数构建基于证据的论证。被质疑方进行辩护或修正。信任权重根据说服结果更新。每一轮产生新的评分三元组。

8. **主持调停** -- 当漂移速度低于阈值（达成收敛）时，主持人计算信任加权的共识分数，识别离群异议，生成最终调停结果。

9. **生成报告** -- 生成结构化的 Markdown 报告，包含执行摘要、评分卡表格、覆盖缺口分析、完整辩论记录、改进建议和面板总结。将完整评估导出为 Turtle RDF 以供跨会话分析。

## 快速开始

```bash
cargo build

# 评估一份文档
brain-in-the-fish evaluate document.pdf --intent "mark this essay"

# 使用自定义评估标准和输出目录
brain-in-the-fish evaluate proposal.pdf --intent "score this tender bid" --criteria rubric.yaml --output ./results

# 启动 MCP 服务器（stdio 传输）
brain-in-the-fish serve
```

## 通用评估

系统在你告知之前并不知道要评估什么。同一引擎通过将三套本体适配到具体领域来处理任意类型的文档。

| 用例 | 文档本体 | 评估标准本体 | 智能体面板 |
|------|----------|-------------|-----------|
| 批改学生论文 | 段落、论点、引用、论题 | 评分标准、分数线、学习成果 | 学科专家、写作专家、批判性思维评估员 |
| 评估招标方案 | 章节、论断、证据、案例研究 | ITT 标准、权重、通过/不通过阈值 | 采购负责人、领域专家、社会价值倡导者、财务评估员 |
| 评估政策 | 目标、措施、影响预测 | 政策框架、影响标准、利益相关方需求 | 政策分析师、利益相关方代表、实施专家 |
| 分析调查结果 | 回应主题、方法论、人口统计 | 研究问题、效度标准 | 统计学家、研究设计师、伦理审查员 |
| 审查合同 | 条款、义务、术语、定义 | 法律清单、风险标准、监管要求 | 法律审查员、合规官、商务分析师 |

## 架构

所有模块编译为单一二进制文件。没有微服务，没有 Python，本体引擎无需网络调用。

| 模块 | 用途 | 代码行数 |
|------|------|---------|
| `types` | 核心评估领域类型（Document、Criteria、Agent、Score、Session） | 289 |
| `ingest` | PDF 文本提取、章节拆分、文档本体 RDF 生成 | 559 |
| `criteria` | 评估框架加载、评估标准本体 RDF 生成 | 451 |
| `agent` | 智能体认知模型（马斯洛 + 信任）、智能体本体 RDF、面板生成 | 779 |
| `scoring` | SPARQL 查询、评分记录、为子智能体生成评分提示 | 811 |
| `debate` | 分歧检测、质疑提示、漂移速度、收敛判定 | 879 |
| `moderation` | 信任加权共识、离群检测、综合结果计算 | 678 |
| `report` | Markdown 报告生成、Turtle RDF 会话导出 | 714 |
| `server` | MCP 服务器，提供 10 个 eval_* 工具（rmcp，stdio + HTTP 传输） | 737 |
| `main` | CLI 入口（clap），evaluate 和 serve 子命令 | 200 |
| `lib` | 模块声明 | 9 |

**总计：约 6,100 行 Rust 代码。**

## MCP 工具

MCP 服务器暴露 10 个工具，用于以编程方式编排评估流程：

| 工具 | 描述 |
|------|------|
| `eval_status` | 服务器状态、版本、会话状态、三元组数量 |
| `eval_ingest` | 摄入 PDF 并构建文档本体 |
| `eval_criteria` | 加载评估框架（通用、学术、招标） |
| `eval_align` | 在文档章节与评估标准之间运行本体对齐 |
| `eval_spawn` | 根据意图生成评估智能体面板 |
| `eval_score_prompt` | 为指定的智能体-评估标准对生成评分提示 |
| `eval_record_score` | 将智能体的评分记录到图存储中 |
| `eval_debate_status` | 当前轮次的分歧、漂移速度和收敛状态 |
| `eval_challenge_prompt` | 生成一个智能体质疑另一个智能体的提示 |
| `eval_report` | 生成包含调停和共识的完整评估报告 |

## 基于 open-ontologies 构建

Brain in the Fish 不是 [open-ontologies](https://github.com/fabio-rovai/open-ontologies) 的分支。它是一个依赖 crate，以库的形式消费 open-ontologies。

```toml
open-ontologies = { path = "../open-ontologies", features = ["embeddings"] }
```

它使用 `GraphStore` 进行三元组存储和 SPARQL 查询，`Reasoner` 进行推理，`Aligner` 进行本体对齐，`Embedder` 进行语义相似度计算——全部作为进程内 Rust 函数调用。零网络开销。无序列化边界。本体引擎与评估逻辑运行在同一地址空间中。

## 测试

95 个测试覆盖所有模块：摄入、评估标准加载、智能体生成、评分、辩论机制、调停、报告生成和 MCP 服务器工具。

```bash
cargo test
```

## 贡献

请参阅 [CONTRIBUTING.md](CONTRIBUTING.md) 了解贡献指南。

## 许可证

MIT
