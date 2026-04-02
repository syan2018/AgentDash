# Pi Agent 动态 Hook 上下文与伴随 Agent 机制

## Goal

为 AgentDash 建立一套正式的 Hook Runtime 架构，使 Pi Agent 具备类似 Claude Code / Trellis 的运行时动态介入能力：

- 会话启动时自动注入结构化上下文
- 工具调用前后可同步执行策略判断与输入/输出改写
- 在 turn 结束与即将 stop 时可根据状态决定是否继续 loop
- companion / subagent 派发时自动继承、裁剪并解释其上下文来源

这套机制的目标不是把 Hook 重新做成一套 Workflow 硬编码，也不是把业务信息查询逻辑塞进 `agent_loop`。目标是：

- 保持 `agent_loop` 作为 Pi 对齐的纯运行时内核
- 让 Hook 决策在 loop 的同步控制边界上及时发生
- 让 Hook 信息获取、业务上下文解析、状态刷新与诊断输出在 loop 外完成

## Background

当前 AgentDash 已具备第一阶段运行时注入能力：

- Workflow builtin template 数据化
- `workflow run -> current phase` 运行时解释器
- phase 级 `agent_instructions` / `context_bindings` 自动解析
- Project / Story / Task 会话都能感知 active workflow phase

当前对应实现主要在：

- [workflow_runtime.rs](crates/agentdash-api/src/workflow_runtime.rs)
- [task_execution_gateway.rs](crates/agentdash-api/src/bootstrap/task_execution_gateway.rs)
- [acp_sessions.rs](crates/agentdash-api/src/routes/acp_sessions.rs)

但这仍不等于完整 Hook Runtime。它本质上还是“prompt 构建前的一次性上下文装配”，缺少真正的 lifecycle hook：

- `SessionStart`
- `UserPromptSubmit`
- `PreToolUse`
- `PostToolUse`
- `after_turn`
- `before_stop`
- companion / subagent dispatch

另一方面，Pi Agent 当前已经有一部分 runtime seam：

- `transform_context`
- `before_tool_call`
- `after_tool_call`
- `AgentEvent::*`

对应位置：

- [agent.rs](crates/agentdash-agent/src/agent.rs)
- [agent_loop.rs](crates/agentdash-agent/src/agent_loop.rs)
- [types.rs](crates/agentdash-agent/src/types.rs)

因此当前最正确的方向不是推翻 Pi runtime，而是把这些 runtime seam 升级为正式的 Hook 控制面，并在 loop 外建立统一的 Hook 信息提供链路。

## Problem Statement

当前系统存在 4 个核心问题：

### 1. 注入逻辑分散在 API / prompt builder 入口

- Task、Story、Project 各自拼接 prompt/context
- Workflow 注入逻辑无法统一复用
- 会话继续执行、turn 结束、tool 前后无法共享同一套状态

### 2. Workflow Runtime 与 Hook Runtime 边界尚未正式确立

- `workflow_runtime` 已经能产出 phase 级约束
- 但它不适合作为 hook 生命周期引擎
- 若继续扩张，最终会重新演变成“workflow 特化硬编码中心”

### 3. Hook 决策与 Hook 信息获取混在一起风险很高

如果把 workflow/task/trellis/project 等业务信息查询直接塞进 `agent_loop`：

- loop 会污染业务依赖
- 无法保持 Pi 对齐的纯运行时结构
- 后续难以在别的 connector/executor 中复用

但如果只在 loop 外做“被动观测”：

- `PreToolUse` / `Stop` 类 hook 决策返回不及时
- 无法影响当前 step 的控制流

### 4. companion / subagent 机制还没有正式运行时抽象

当前没有一套平台级语义来回答：

- subagent dispatch 是什么生命周期事件
- dispatch 时应该继承哪些上下文
- 谁来决定裁剪/增强
- 如何记录注入原因与诊断信息

## Design Principles

### 1. `agent_loop` 保持纯运行时，不直接查询业务对象

`agent_loop` 允许存在通用扩展点，但不允许直接依赖：

- workflow repo
- task/story/project repo
- Trellis 目录
- workspace journal
- API route / AppState

### 2. Hook 信息获取与 Hook 控制决策分离

必须明确区分两条链路：

- 信息面：在 loop 外查询与缓存业务 Hook 信息
- 控制面：在 loop 的同步边界上 `await` Hook 决策

### 3. Workflow Runtime 是 Hook 的信息来源，不是 Hook 引擎

- Workflow Runtime 负责回答“当前应有什么约束”
- Hook Runtime 负责回答“在哪个 runtime 节点把这些约束送进去”

### 4. Hook 必须可诊断、可回放、可解释

需要能解释：

- 当前命中了哪些 hook
- 为什么命中
- 注入了哪些上下文/约束
- 来源于哪个 workflow phase / task / trellis 文件 / system policy

### 5. companion / subagent dispatch 必须成为正式 lifecycle point

不能依赖 prompt 惯例或上层调用者手写 prompt。

## Scope

本 task 负责完整规划并推进以下能力：

- Hook Runtime 分层与依赖关系
- Hook 信息提供链路
- Pi Agent 同步控制边界上的 Hook 决策接口
- Session / turn / tool / stop / subagent 生命周期模型
- Workflow Runtime 与 Hook Runtime 的协作关系
- Hook 诊断、trace 与 snapshot 结构
- 第一阶段到第三阶段的落地路径

## Non-Goals

- 不要求短期内完整复制 Claude Code 全部内部实现
- 不要求首轮就实现完整 Hook DSL
- 不把 Trellis 的目录结构、脚本名、jsonl 文件名直接定义成平台 API
- 不要求首轮覆盖所有 executor / connector
- 不在本任务内先做完整 UI 面板

## Requirements

### 架构要求

- 必须保持 `agentdash-agent` 不依赖 application / api / repo
- 必须让 `agentdash-executor` 承担 Hook Runtime 编排职责
- 必须让 `agentdash-api` / `agentdash-application` 实现 Hook 信息提供接口
- 必须把当前散落在 route / gateway 的 prompt augment 逻辑逐步收敛

### 控制时序要求

- `PreToolUse` 必须在 tool 真正执行前同步返回决策
- `PostToolUse` 必须在 tool result 回写 loop 前同步返回后处理结果
- `after_turn` / `before_stop` 必须在 outer loop 决定继续或退出前同步返回控制结果
- 不能只依赖事件广播做异步观察

### 信息链路要求

- Session 级 Hook snapshot 必须可缓存
- Hook snapshot 必须支持 refresh
- Hook provider 必须能按 event / owner / workflow phase / task phase / tool name / subagent type 做匹配
- Workflow Runtime 产物必须能作为 Hook snapshot 的一部分被消费

### companion / subagent 要求

- 必须有正式 dispatch 事件语义
- 必须支持按 dispatch target 选择上下文切片
- 必须记录 dispatch 注入来源与原因

### 诊断要求

- 必须输出 Hook trace / diagnostic entry
- 必须能解释“为什么当前 phase / tool / subagent 注入了这些约束”
- 必须支持 session snapshot 对外暴露，供后端与前端调试

## Acceptance Criteria

- [ ] 明确 `agent_loop`、executor、provider 三层的职责与依赖边界。
- [ ] 明确 Hook 的信息面与控制面两条链路。
- [ ] 明确 Session / UserPrompt / Tool / TurnStop / Subagent 五类 lifecycle。
- [ ] 明确 `Workflow Runtime -> Hook Snapshot` 的协作模型。
- [ ] 明确 Hook 决策接口、诊断输出与 snapshot refresh 机制。
- [ ] 给出分阶段落地路径，并为每阶段指定主要改动 crate。
- [ ] 给出首轮建议实现的 trait / struct 草图。

## References

- [Claude Code hooks](https://docs.anthropic.com/en/docs/claude-code/hooks)
- [Claude Code hooks guide](https://docs.anthropic.com/en/docs/claude-code/hooks-guide)
- [Claude Code settings](https://docs.anthropic.com/en/docs/claude-code/settings)
- [Trellis README](references/Trellis/README.md)
- [Trellis `session-start.py`](references/Trellis/.claude/hooks/session-start.py)
- [Trellis `inject-subagent-context.py`](references/Trellis/.claude/hooks/inject-subagent-context.py)
- [Trellis 架构文档](https://docs.trytrellis.app/zh/guide/ch04-architecture)
- [Pi 设计文档](crates/agentdash-agent/agent-design/RUST_PI_HYBRID_DESIGN.md)

## Current Judgment

当前最优路径不是把 hook 做成另一套 workflow phase 配置，也不是把 repo 查询逻辑侵入 `agent_loop`。

更正确的结构是：

1. `agent_loop`
   只保留通用运行时扩展点与同步控制边界
2. `agentdash-executor`
   承担 Hook Runtime 编排、snapshot 缓存、决策适配
3. `agentdash-api` / `agentdash-application`
   负责从 workflow/task/story/project/trellis/workspace 中“向外捞”Hook 信息
4. `workflow_runtime`
   退回为 Hook 信息来源之一，而不是生命周期引擎

也就是说：

- Hook 信息获取在 loop 外
- Hook 决策调用发生在 loop 的同步边界上
- 这两者必须同时成立，才能既不污染 Pi Runtime，又保证控制流来得及生效
