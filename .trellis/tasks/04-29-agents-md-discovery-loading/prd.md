# AGENTS.md / 项目级隐式文件自动发现与加载

> **状态**：brainstorm 阶段。先对齐范围和语义，再定实现。

## Goal

让 AgentDashboard 在创建 session 时能自动识别并加载项目根的"约定级隐式文件"（AGENTS.md、CLAUDE.md、MEMORY.md 等），作为 session 上下文的一部分交给下游 Agent。

## 背景

[research/../../04-29-session-context-builder-unification/research/context-injection-map.md](../04-29-session-context-builder-unification/research/context-injection-map.md) 调研结论明确：

> 项目**当前没有**自动加载 MEMORY.md / CLAUDE.md / AGENTS.md。`.trellis/workflow.md` 只是 workflow contract 里写死的 binding locator，不是全局隐式文件。

这和业界常见预期（Claude Code 自动读 CLAUDE.md、Codex CLI 自动读 AGENTS.md）有差距，会导致：

1. 用户在项目里写了 AGENTS.md，换到 AgentDashboard 跑 Claude/Codex 时这些约定**不会被 Agent 看到**
2. 需要手动复制粘贴到 system prompt 或 agent default 里，容易漂移
3. 与姊妹任务 `session-context-builder-unification` 要设计的 ContextBuilder 强相关 —— 这类文件应该作为一种独立的 `ContextSource`

## 讨论议题

### Q1: 识别哪些文件

候选清单（需要和你对齐）：

- `AGENTS.md`（Codex / 多 agent 生态约定）
- `CLAUDE.md`（Claude Code 约定）
- `.cursor/rules/*.md` 或 `.cursorrules`（Cursor）
- `MEMORY.md`（通用）
- 项目根 + 子目录（cascade 搜索）vs 仅项目根
- **是否要做一个"约定文件策略表"，按 agent 类型选择加载哪些**？

### Q2: 加载时机与粒度

- 仅 session 创建时一次性读取 vs 每轮都检查（文件可能被编辑）
- 全文注入 vs 只注入摘要/路径（超大文件如何处理）
- 是否带文件大小上限 / 截断

### Q3: 作用域（谁会看到）

- 仅 runtime agent？还是同时给 title generator / summarizer？
- 是否按 agent 类型过滤（给 Claude 的 session 只注入 CLAUDE.md + AGENTS.md，给 Codex 的只注入 AGENTS.md）
- 用户能否在 session 级别手动关掉某个文件的注入

### Q4: 与 ContextBuilder 的接口边界

- 这个任务应该**等** ContextBuilder 抽象定下来再做，还是**先行**作为独立 source 接入现有 `system_context`？
- 若先行：以什么最小改动挂到现有 `build_runtime_system_prompt`？
- 与 agent defaults / workflow 的优先级关系

### Q5: 发现策略

- 工作目录（cwd）vs session 挂载的 project root
- 多 project / monorepo 场景（根 + package 级都有 AGENTS.md）
- .gitignore 要不要尊重（`AGENTS.md` 通常入库，但子目录可能被 ignore）

### Q6: 和 MCP / plugin 注入的重叠

- 现在是否已有 plugin 在做类似的事（需要核实）
- 如果有，是替代它们还是并存

## Non-Goals

- 定义新的隐式文件格式（复用已有生态约定）
- 编辑器/IDE 集成（仅做后端读取与注入）
- 文件内容的语义理解、摘要生成（按需另起任务）
- 跨项目共享的全局 AGENTS.md（仅项目级）

## 依赖 / 协同任务

- `**session-context-builder-unification`**（姊妹任务）：本任务的最终挂载方式取决于 ContextBuilder 的 source 抽象。讨论阶段需要两者对齐接口。
- 调研底座：[../04-29-session-context-builder-unification/research/context-injection-map.md](../04-29-session-context-builder-unification/research/context-injection-map.md)

## Acceptance Criteria（brainstorm 阶段）

- Q1–Q6 每题有对齐的结论
- 明确 MVP 范围（比如：只识别项目根的 AGENTS.md + CLAUDE.md，session 创建时一次性加载，全作用域注入）
- 与 ContextBuilder 任务对齐 source 接口
- 产出 Technical Approach 章节（文件发现算法、注入点、配置开关）