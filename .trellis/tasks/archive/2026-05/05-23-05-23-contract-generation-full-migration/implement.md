# 前后端契约生成全量迁移 Implement

## Order

1. Start task and read specs:
   - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
   - `.trellis/spec/frontend/type-safety.md`
   - `.trellis/spec/backend/architecture.md`
2. Create `agentdash-contracts` crate and add workspace dependency.
3. Implement `mcp_preset` DTO module with serde + ts-rs derives.
4. Add `generate_contracts_ts` with write/check modes.
5. Update `package.json` contract scripts to run Backbone + business contracts.
6. Update `agentdash-api` MCP Preset route/dto imports.
7. Generate `packages/app-web/src/generated/mcp-preset-contracts.ts`.
8. Update frontend MCP Preset types/service imports.
9. Run validation and update progress.

## Validation

```powershell
cargo fmt -p agentdash-contracts -p agentdash-api
cargo check -p agentdash-contracts -p agentdash-api
pnpm run contracts:check
pnpm run frontend:check
```

## Progress

- Batch 1 MCP Preset 已完成：
  - 新增 `crates/agentdash-contracts`，接入 workspace。
  - 新增 `mcp_preset` contract DTO 与 `generate_contracts_ts` 写入/检查模式。
  - `package.json` 的 `contracts:generate` / `contracts:check` 已串联 Backbone 与业务 contract。
  - `agentdash-api` MCP Preset route 改用 `agentdash-contracts::mcp_preset` DTO，API 侧旧 DTO 模块已删除。
  - 前端生成 `packages/app-web/src/generated/mcp-preset-contracts.ts`，`types/mcp-preset.ts` 改为 re-export generated type。
  - `services/mcpPreset.ts` 保留 runtime mapper，但不再手写 MCP enum/union 事实源。
  - 已验证 `cargo test -p agentdash-contracts`、`cargo check -p agentdash-contracts -p agentdash-api`、`pnpm run contracts:check`、`pnpm run frontend:check`。
- Batch 2 Session stream envelope 已完成：
  - 新增 `agentdash-contracts::session`，承载 `SessionEventResponse`、`SessionEventsPageResponse`、`SessionNdjsonEnvelope`。
  - `agentdash-api` 会话事件分页和 ACP NDJSON stream 改用 contract DTO / envelope。
  - 生成 `packages/app-web/src/generated/session-contracts.ts`，并显式引用 `BackboneEnvelope`。
  - 前端 `SessionEventEnvelope` 改为 generated `SessionEventResponse`，stream parser 只读取 snake_case contract 字段。
  - 已验证 `cargo check -p agentdash-contracts -p agentdash-api`、`pnpm run contracts:check`、`pnpm run frontend:check`。
