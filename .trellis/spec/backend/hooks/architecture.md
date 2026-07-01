# Hooks Architecture

## Role

Hook 子系统把 workflow / project / story / task / session 等来源的策略解析成 runtime snapshot，并在 Agent Loop 生命周期边界同步消费这些决策。它的职责是让动态上下文、审批、完成门禁、companion 回流等行为保持可审计且不污染 Agent Loop。

## Invariants

- Hook 信息获取发生在 loop 外；控制决策发生在 loop 边界。
- `agentdash-agent` 只依赖 `AgentRuntimeDelegate`，不得查询 workflow、task、story、project 或 repository。
- `UserPromptSubmit` 是动态文本注入主通道。
- `BeforeTool` 的 Ask 必须在 tool call 边界同步挂起等待审批。
- runtime context update 不作为第二条即时 live notification 推给 Agent；它进入 turn-start 队列，在 `transform_context(UserPromptSubmit)` 边界统一消费。
- Agent-visible runtime steering 的一等展示结构是 `ContextFrame`。
- `HookTurnStartNotice.content` 必须等于对应 `ContextFrame.rendered_text`。
- AgentRun-anchored hook delivery message 进入 AgentRun Mailbox；hook runtime 继续拥有 block/context injection、tool approval 和 trace。这样 delivery 调度、dedup、恢复和前端投影与用户/system message 使用同一 control-plane envelope。

## Current Baseline

分层：

```text
global builtin / workflow / task / story / project / session
        -> ExecutionHookProvider
        -> HookContributionSet merge
        -> AgentFrameHookSnapshot + HookResolution
        -> AgentFrameHookRuntime
        -> AgentRuntimeDelegate
```

脚本引擎 baseline：contract-driven hook rules 通过 Rhai `HookScriptEngine` 执行，global 基础设施级规则保留 Rust 硬编码。

## Local Decisions

- Hook rule 脚本选择 Rhai，原因是短小规则脚本需要沙箱限制、AST 缓存和 Rust 类型互操作。
- Workflow policy authority 是 `ActiveWorkflowProjection.effective_contract`，原因是运行态应消费闭包后的 active contract，而不是静态模板。

## Contract Appendices

- [Execution Hook Runtime](./execution-hook-runtime.md)
- [Hook Script Engine](./hook-script-engine.md)
