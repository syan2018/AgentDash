# Runtime Failure / Placement 执行计划

## Phase 1: Characterization

- [ ] 验证 backend disconnect 后 running prompt 的 feed / AgentRun / runtime-summary 行为。
- [ ] 验证 session context 下 MCP target fallback 行为。
- [ ] 验证 standalone local backend id 来源和 runtime-summary 表达。

## Phase 2: Design

- [ ] 决定 execution backend missing 是 terminal/lost 还是 fallback。
- [ ] 决定 session context MCP fallback 边界。
- [ ] 决定 standalone local backend identity 是否为 debug/internal。

## Phase 3: Implementation

- [ ] backend disconnect terminal/lost projection。
- [ ] MCP backend fallback 收口。
- [ ] standalone backend id 来源收口。

## Validation

```powershell
cargo test -p agentdash-api relay
cargo test -p agentdash-application session
cargo test -p agentdash-local
pnpm run frontend:check
```

