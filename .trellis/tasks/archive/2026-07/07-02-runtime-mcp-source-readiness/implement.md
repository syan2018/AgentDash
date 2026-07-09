# MCP 工具源 readiness 收束实施计划

## Preconditions

- 工作区存在非本任务修改时，不触碰那些文件的 unrelated diff；若实现必须改同一文件，先阅读现有 diff 并在其上增量修改。
- 实现前加载 `trellis-before-dev`，并读取相关 package/layer spec。
- 本任务为复杂任务，进入实现前需要用户审核 PRD/design/implement，随后运行 `task.py start`。

## Implementation Checklist

- [ ] SPI model: 引入 MCP source readiness / outcome 类型，删除 `unavailable_mcp_servers` 独立字段。
- [ ] Capability delta/frame: MCP delta 从 source surface 派生 readiness change；context frame 渲染 unavailable source。
- [ ] Discovery contract: 将 `McpToolDiscovery` 改为 partial outcome，更新 test fakes。
- [ ] Direct discovery: per-server 收集 ready/unavailable outcome，不 first-failure abort。
- [ ] Relay provider: `list_relay_tools` 返回 tools + per-server outcome，覆盖 backend unresolved/offline/timeout/error/unexpected response。
- [ ] Tool assembly: `AssembledToolSurface` 同源返回 tools/schema/source outcomes，移除 `mcp_failures`。
- [ ] Session launch: `TurnPreparer` 合并 outcome 后再派生唯一最终 `CapabilityState`；`PreparedTurn` 携带结构化 startup notice。
- [ ] Commit/eventing: `TurnCommitter` 在 accepted commit 阶段提交 readiness notice，保持 user input / turn_started / notice / context frame 顺序。
- [ ] Local runtime: MCP health 变化后触发 `EventCapabilitiesChanged`，云端 registry/runtime health 更新。
- [ ] Frontend/contracts: 如新增 Backbone event 或 DTO，生成 TS binding；前端系统事件卡片消费结构化 payload。
- [ ] Tests: 覆盖 direct/relay partial outcome、TurnPreparer final state、context frame、local capabilities changed、generated contract drift。

## Suggested File Areas

- `crates/agentdash-spi/src/connector/mod.rs`
- `crates/agentdash-spi/src/connector/capability_delta.rs`
- `crates/agentdash-application-ports/src/mcp_discovery.rs`
- `crates/agentdash-executor/src/mcp/direct.rs`
- `crates/agentdash-executor/src/mcp/relay.rs`
- `crates/agentdash-executor/src/mcp/mod.rs`
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs`
- `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/preparation.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/commit.rs`
- `crates/agentdash-application-runtime-session/src/session/dimension/mcp_server.rs`
- `crates/agentdash-local/src/mcp_client_manager.rs`
- `crates/agentdash-local/src/ws_client.rs`
- `packages/app-web/src/generated/backbone-protocol.ts` if protocol changes
- `packages/app-web/src/features/session/...` if a new renderable platform event is introduced

## Validation Commands

Run focused checks first:

```powershell
cargo test -p agentdash-executor mcp
cargo test -p agentdash-api relay::mcp
cargo test -p agentdash-application-runtime-session tool_assembly
cargo test -p agentdash-application-runtime-session mcp
cargo test -p agentdash-local mcp
```

Run contract/codegen checks if wire types change:

```powershell
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
cargo run -p agentdash-contracts --bin generate_ts
```

Run broader checks after focused tests pass:

```powershell
cargo test -p agentdash-application-runtime-session
cargo test -p agentdash-api
pnpm --filter @agentdash/app-web test
pnpm --filter @agentdash/app-web typecheck
```

If these commands are too broad for the current iteration, record skipped checks and why.

## Review Gates

- Verify no independent `unavailable_mcp_servers` field remains.
- Verify every requested MCP server produces exactly one source outcome in relay and direct discovery.
- Verify accepted capability state and connector context use the same final MCP source surface.
- Verify UI/system event and model context both derive from structured source readiness.
- Verify runtime summary health can update after registration.

## Rollback Points

- SPI model update should compile before touching session launch.
- Discovery outcome should have focused tests before updating UI/eventing.
- Local capabilities changed propagation should be isolated enough to revert independently if noisy.
