# Hook 事件体系补全

> 状态：implemented
> 参考：`references/pi-mono/packages/coding-agent/src/core/extensions/types.ts`

## 背景

我们当前的 `HookTrigger` 覆盖了 agent 生命周期的主干：
`SessionStart` / `UserPromptSubmit` / `BeforeTool` / `AfterTool` / `AfterTurn` / `BeforeStop` / `SessionTerminal` / `BeforeSubagentDispatch` / `AfterSubagentDispatch` / `SubagentResult` / `BeforeCompact` / `AfterCompact`

pi-coding-agent 的 extension 系统还有若干我们缺失的事件类型和 Plugin 注册能力。经过分析和讨论，最终决策如下：

## 决策记录

### 1. UserInput → 合并到 UserPromptSubmit（已实现）

**决策**：不新增 `UserInput` trigger，而是扩展现有 `UserPromptSubmit` 的能力。

**原因**：两者的管道位置差距很小（prompt_pipeline 入口 vs agent loop 的 transform_context 阶段），新增独立 trigger 只增加概念数量而不带来实质收益。

**实现**：
- `HookResolution` 新增 `block_reason: Option<String>`（已有）+ `transformed_message: Option<String>`（新增）
- `hook_delegate.rs` 的 `transform_context()` 方法在评估 `UserPromptSubmit` 后检查这两个字段
- `TransformContextOutput` 新增 `blocked: Option<String>` 字段
- agent loop 检测到 blocked 时返回 error assistant message 终止当前轮次

### 2. BeforeAgentStart → 不实现

**决策**：砍掉。

**原因**：现有 `UserPromptSubmit` 的 injection 机制已足够覆盖"每轮注入动态上下文"的场景。`system_prompt_append` 能力（直接修改 system prompt）风险过高，且没有紧迫的具体场景。

### 3. BeforeProviderRequest → 仅观测（已实现）

**决策**：新增 `BeforeProviderRequest` trigger，但仅作为观测点，不允许改写 payload。

**原因**：完整 payload 改写的实现代价高（深入 rig_bridge 层、每轮多次 LLM 调用都要触发评估），且缺乏紧迫场景。观测足以满足日志记录和 token 统计需求。

**实现**：
- `HookTrigger` 新增 `BeforeProviderRequest` 变体
- `AgentRuntimeDelegate` 新增 `on_before_provider_request()` 方法（默认空实现）
- agent loop 在 `bridge.stream_complete()` 前调用
- hook_delegate 实现中评估 hook 规则并记录 trace（payload 仅含 `system_prompt_len` / `message_count` / `tool_count`）
- 忽略评估错误（不阻塞 LLM 调用）

### 4. Plugin API → 拆为独立任务

**决策**：`registerCommand()` / `registerFlag()` / `CustomMessage<T>` 拆到独立任务，不在此 PR 中实现。

**原因**：Plugin API 本质上是构建扩展系统，与 Hook 事件体系是不同层面的东西。

## 当前 HookTrigger 全量枚举（13 个）

```rust
pub enum HookTrigger {
    SessionStart,
    UserPromptSubmit,        // 扩展：支持 block + transformed_message
    BeforeTool,
    AfterTool,
    AfterTurn,
    BeforeStop,
    SessionTerminal,
    BeforeSubagentDispatch,
    AfterSubagentDispatch,
    SubagentResult,
    BeforeCompact,
    AfterCompact,
    BeforeProviderRequest,   // 新增：仅观测
}
```

## 改动文件清单

### SPI 层
- `agentdash-spi/src/hooks.rs` — `HookTrigger` 新增 `BeforeProviderRequest`；`HookResolution` 新增 `transformed_message`
- `agentdash-spi/src/hook_trace_notification.rs` — trigger key 映射、decision 描述、severity 规则
- `agentdash-spi/src/lifecycle.rs` — re-export `BeforeProviderRequestInput`
- `agentdash-spi/src/lib.rs` — re-export

### Agent Types 层
- `agentdash-agent-types/src/decisions.rs` — `TransformContextOutput` 新增 `blocked`；新增 `BeforeProviderRequestInput`
- `agentdash-agent-types/src/delegate.rs` — `AgentRuntimeDelegate` 新增 `on_before_provider_request()`
- `agentdash-agent-types/src/message.rs` — `AgentMessage` 新增 `is_user()` / `replace_user_text()`
- `agentdash-agent-types/src/lib.rs` — re-export

### Agent 层
- `agentdash-agent/src/agent_loop.rs` — transform_context blocked 处理 + BeforeProviderRequest 观测调用
- `agentdash-agent/src/types.rs` — re-export
- `agentdash-agent/src/lib.rs` — re-export
- `agentdash-agent/tests/runtime_alignment.rs` — 适配新字段

### Application 层
- `agentdash-application/src/session/hook_delegate.rs` — transform_context 处理 block/transform + on_before_provider_request 实现
- `agentdash-application/src/hooks/provider.rs` — evaluate_hook 分发新增 BeforeProviderRequest 分支
- `agentdash-application/src/hooks/presets.rs` — domain_trigger_to_spi 映射
- `agentdash-application/src/hooks/script_engine.rs` — trigger 字符串映射

### Domain 层
- `agentdash-domain/src/workflow/value_objects.rs` — `WorkflowHookTrigger` 新增 `BeforeProviderRequest`
