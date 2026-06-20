# WI-2 Capability snapshot/delta 与 companion roster 收束

## Status

completed

## Goal

将能力事实统一收束到 `CapabilityState` 派生链路，并让 CAP UI 能明确区分完整状态与增量变化。

## Scope

- companion roster 只从 `CapabilityState.companion.agents` 派生。
- 清理 `companion_agents` assignment slot、hook order、contract sample、测试夹具和 spec 说明。
- 明确 initial/bootstrap CAP frame 的 snapshot 语义。
- 明确 runtime transition CAP frame 的 delta 语义。
- 核对 `SetCompanionAgentRosterEffect` 是否有真实生产者；按最终协议保留或删除。

## Primary Files

- `crates/agentdash-application/src/capability/resolver.rs`
- `crates/agentdash-application/src/agent_run/frame/construction/owner_bootstrap.rs`
- `crates/agentdash-application/src/session/dimension/companion_agent.rs`
- `crates/agentdash-application/src/session/hub/runtime_context_transition.rs`
- `crates/agentdash-spi/src/context/injection.rs`
- `.trellis/spec/backend/capability/*.md`

## Acceptance

- [x] owner bootstrap 不再生成 companion roster assignment fragment。
- [x] CAP snapshot/delta section 中能表达 effective companion roster。
- [x] `companion_request` 工具、模型上下文、前端 CAP 使用同一 roster。
- [x] `companion_agents` 作为 roster slot 的协议残留已清理。

## Implementation Notes

- `ASSIGNMENT_CONTEXT_SLOTS` 不再包含 `companion_agents`，hook bridge 也不再为该 slot 分配 roster 专用 order。
- `CompanionAgentRosterDelta` 继续由 `CapabilityState.companion.agents` 派生，并在 initial snapshot 中携带 effective roster；context usage 的 Agents 项从 CAP roster section 计算。
- 前端 CAP 卡通过 `capability_state_snapshot` / `capability_state_delta` 区分完整状态与增量变化，workspace 刷新逻辑同时响应两种 kind。

## Validation

- `pnpm --filter app-web test -- contextFrame ContextFrameCard`
- `pnpm --filter app-web run check`
- `cargo test -p agentdash-application runtime_context_transition --lib`
- `cargo test -p agentdash-application assignment_context_frame --lib`
- `cargo test -p agentdash-contracts projection_view_aggregates_context_frame_usage_items --lib`
