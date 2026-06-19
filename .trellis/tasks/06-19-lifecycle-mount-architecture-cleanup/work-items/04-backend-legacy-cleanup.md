# Work Item 04: Backend Legacy Cleanup

## Status

Planned.

## Goal

清理后端中已经没有业务入口或仍在承载旧数据结构兼容的代码，使本任务完成后不会继续暴露旧 schema / 旧 factory / dead module。

## Scope

- 删除 `crates/agentdash-agent-protocol/src/compat/mod.rs` dead module。
- 删除 `ExecutionSource::Migration` 和无构造点分支。
- 删除 `SessionCapabilityEntry::legacy` 旧 flat skill 工厂。
- 删除 deprecated `ContextBundle::render_section` public API 与仅服务该 API 的测试。
- 评估并收紧 workflow contract 静默忽略旧字段、AgentSource alias、capability `workspace_module` 默认值。

## Guardrails

- 拒绝旧 schema 的测试不是兼容路径；这类测试保留，除非生产 schema 已改成更严格的统一验证并有替代覆盖。
- Shared Library 的 `deprecated` 是业务状态，不属于 legacy cleanup。
- diff/patch 中的 `old/new` 是算法语义，不属于 legacy cleanup。

## Affected Areas

- `crates/agentdash-agent-protocol/src/compat/mod.rs`
- `crates/agentdash-domain/src/workflow/dispatch.rs`
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`
- `crates/agentdash-spi/src/context/capability.rs`
- `crates/agentdash-spi/src/context/bundle.rs`
- `crates/agentdash-domain/src/workflow/value_objects.rs`
- `crates/agentdash-domain/src/workflow/lifecycle_agent.rs`
- `crates/agentdash-application/src/session/capability_state.rs`

## Dependencies

可与 lifecycle 架构主线并行，但每批删除都必须连同对应数据结构、调用点和测试一起清理。

## Validation

- `cargo test -p agentdash-domain workflow`
- `cargo test -p agentdash-spi context`
- `cargo test -p agentdash-application session::capability_state`
- `cargo check --workspace`
