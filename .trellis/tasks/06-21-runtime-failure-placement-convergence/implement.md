# Runtime Failure / Placement 执行计划

## Phase 1: Characterization

- [x] 验证 backend disconnect 后 running prompt 的 feed / AgentRun / runtime-summary 行为。
- [x] 验证 session context 下 MCP target fallback 行为。
- [x] 验证 standalone local backend id 来源和 runtime-summary 表达。

## Phase 2: Design

- [x] 决定 execution backend missing 是 terminal/lost 还是 fallback。
- [x] 决定 session context MCP fallback 边界。
- [x] 决定 standalone local backend identity 是否为 debug/internal。

## Phase 3: Implementation

- [x] backend disconnect terminal/lost projection。
- [x] MCP backend fallback 收口。（session context 仅使用 session route；验证被当前 PermissionGrant 编译错误阻塞，待上游修复后重跑聚焦测试）
- [x] standalone backend id 来源收口。

## Validation

```powershell
cargo test -p agentdash-api relay
cargo test -p agentdash-application session
cargo test -p agentdash-local
cargo test -p agentdash-api relay_mcp_backend_resolution --lib
pnpm run frontend:check
```
