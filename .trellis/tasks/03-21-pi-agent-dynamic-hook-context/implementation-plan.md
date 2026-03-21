# Pi Agent 动态 Hook 上下文与伴随 Agent 机制实施方案

## 总体判断

这个方向不应再被理解成“补几个 prompt 模板”。

它本质上是要在 Pi Agent 之上增加一层 runtime harness，使平台可以像 Trellis / Claude Code 那样，在不同运行时阶段自动注入上下文、约束、策略与伴随行为。

## 目标分层

### 1. Workflow Runtime

职责：

- 根据 workflow run / current phase 决定当前会话应具备的约束与上下文
- 负责 phase 级 `bindings / instructions / completion signals`

当前状态：

- 已有第一版
- 但仍主要停留在 session prompt 注入

### 2. Hook Runtime

职责：

- 在 session / turn / tool / subagent 生命周期上做动态介入
- 负责实时切换上下文、注入伴随约束、执行 guardrail

当前状态：

- 尚未正式建模
- 需要成为下一阶段重点

### 3. Companion / Subagent Runtime

职责：

- 派发 companion agent / subagent
- 为其裁剪并继承正确上下文
- 记录来源、注入原因与可追踪关系

当前状态：

- 尚未建立
- 需要参考 Trellis 的 `inject-subagent-context.py`

## 推荐分期

### Phase A: Hook 生命周期建模

产出：

- 统一 hook lifecycle 模型
- 统一 hook event / hook context / hook output 模型
- session / turn / tool / subagent 4 类 hook 的接口边界

建议落点：

- `agentdash-agent` 或 `agentdash-executor` 内新增 hook runtime 层
- 不要先直接塞到 API route

### Phase B: SessionStart / SessionPrompt 注入

产出：

- 会话启动时自动注入 workflow / project / story / task / workspace 上下文
- 会话继续执行时也能基于当前状态补充上下文，而不是只靠第一次 prompt

建议落点：

- `PiAgentConnector::build_runtime_system_prompt`
- `ExecutorHub::start_prompt`
- runtime prompt builder / context composer

### Phase C: Subagent / Companion Agent 注入

产出：

- companion agent / subagent dispatch 语义
- 上下文裁剪与继承
- 来源追踪与诊断输出

参考目标：

- 类似 Trellis 的：
  - 当前 task 指针
  - agent-specific jsonl / spec context
  - 派发前自动注入，而不是调用方手写 prompt

### Phase D: Tool 前后 Hook 与 Guardrail

产出：

- PreToolUse / PostToolUse
- 动态上下文补充
- 工具审计 / 权限检查 / 风险提示
- 与 workflow completion signal 的联动

## 建议的接口草图

### Hook Context

- session_id
- owner binding
- workflow runtime snapshot
- current task / story / project
- executor context
- current turn_id
- tool call context
- dispatch target context

### Hook Event

- `session_start`
- `before_prompt`
- `after_prompt`
- `before_tool`
- `after_tool`
- `before_subagent_dispatch`
- `after_subagent_dispatch`
- `before_turn_complete`

### Hook Output

- injected_fragments
- appended_instructions
- policy_decisions
- diagnostic_entries
- optional blocking decision

## 关键设计原则

### 1. 不能把 hook 直接写成 Trellis 特化逻辑

Trellis 只是参考实现，不是平台 API。

### 2. companion agent 不应只靠 prompt 约定

必须有结构化 dispatch context。

### 3. hook 必须可诊断

用户与开发者都应该能看到：

- 当前注入了什么
- 为什么注入
- 来源于哪个 workflow / hook / state

### 4. hook 应与 workflow runtime 协同，而不是互相覆盖

- workflow runtime 负责“此刻应该有什么约束”
- hook runtime 负责“在哪个运行时节点把它送进去”

## 近期建议的直接实施项

### 优先级 P1

- 建立 hook runtime 抽象与生命周期模型
- 先跑通 SessionStart 注入
- 先跑通 subagent / companion dispatch 注入

### 优先级 P2

- 建立 Tool 前后 hook
- 引入 hook 诊断快照
- 打通 workflow completion signal 与 hook runtime

### 优先级 P3

- 再讨论 hook DSL、自动化面板、复杂策略编排
- 再讨论更通用的 automation / orchestration runtime
