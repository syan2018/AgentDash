# Workflow 数据驱动重构

## Goal

把当前偏向 Trellis 特化实现的 Workflow 框架，重构为“数据驱动的配置层 + 少量全局内置 workflow 模板”的平台能力。

## Background

当前 Workflow 虽然已经抽象出 `WorkflowDefinition / WorkflowAssignment / WorkflowRun`，但核心内置流程仍然通过硬编码 builder、专用 bootstrap API、前端写死文案和 phase 特判来驱动，尚未达到“内置 Workflow 只是可配置模板”的目标。

## Requirements

- 内置 Workflow 应作为全局模板定义存在，而不是通过 Rust 代码逐条拼装。
- 平台层负责加载、校验、解释 Workflow 定义，不直接写死 Trellis phase 细节。
- 允许存在少量全局内置 Workflow，并由 Project 通过 assignment 选择使用。
- Workflow 的 target、phase、context binding、completion policy、record policy 应主要由数据定义驱动。
- API 层不再暴露 Trellis 专用 bootstrap 入口，而应收敛为通用 builtin/template 接入方式。
- 前端面板应优先展示通用 workflow/template 信息，而不是写死 Trellis 专属交互语义。
- session 绑定、target 存在性、run 生命周期等关键约束需要纳入通用框架校验，不依赖前端猜测。

## Acceptance Criteria

- [x] 明确 builtin workflow/template 的数据模型与加载方式。
- [x] 明确 definition、assignment、run 三层中哪些字段属于配置，哪些字段属于运行态。
- [x] 明确 phase 行为如何由数据驱动，而不是依赖 phase key 特判。
- [x] 明确 API 如何从 Trellis 专用入口收敛到通用模板机制。
- [x] 明确前端如何从 Trellis 特化面板收敛到通用 workflow 渲染方式。
- [x] 明确迁移路径，保证现有 Trellis Dev Workflow 可以作为首个 builtin 模板平滑迁移。
- [x] 让 active workflow phase 的约束与上下文真正进入 Task / Story / Project 会话，而不是只停留在展示层。

## Out of Scope

- 不在本任务内直接完成全部代码改造。
- 不在本任务内设计完整的可视化 workflow designer。
- 不在本任务内扩展到复杂调度器、retry loop、后台托管执行。

## Key Questions

- builtin workflow 应存储在仓库资源文件、数据库，还是两者结合？
- phase action / artifact 规则应如何表达，才能既可配置又不过度设计？
- Trellis 特有路径和脚本引用，应该沉到 resolver/action 层还是保留为模板数据的一部分？
- 如何让 SessionBinding 校验与 Workflow target 绑定关系成为平台统一约束？

## Current Conclusion

- Workflow 现已不再只是“模板注册 + phase 面板”。
- 当前框架已经补上了第一版运行时解释器：
  - phase 的 `agent_instructions` 会自动注入给 Agent
  - `context_bindings` 会被解析成真实的 session/context 片段
  - Project / Story / Task 三条会话链路都能感知 active workflow phase
- 但这仍只是“平台内 Workflow Runtime”第一阶段，不等于已经完全复刻真实 Trellis hooks。
- 下一阶段要继续补的是：
  - `completion_mode` 真实执行语义
  - Trellis task jsonl / hook 级上下文切换
  - 更完整的 SessionStart / PreToolUse 风格自动注入
