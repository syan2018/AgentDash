# 前后端契约生成全量迁移 Design

## Contract Crate

新增：

```text
crates/agentdash-contracts/
  Cargo.toml
  src/
    lib.rs
    generate_ts.rs
    mcp_preset.rs
    session.rs
    workflow.rs
    vfs.rs
    shared_library.rs
    project_agent.rs
```

`lib.rs` 暴露各 domain module。`generate_ts.rs` 负责把每个 domain root type 导出到 `packages/app-web/src/generated/<domain>-contracts.ts`。

## Generation Model

- 使用 `ts-rs`，保持现有 Backbone 生成器风格。
- 每个 domain 有一个 `export_<domain>_contracts()` root 函数或 root type list。
- 生成器支持：
  - write mode：写入 generated files；
  - check mode：比对现有 generated files，drift 时返回非零。

## Package Scripts

`package.json`：

```json
{
  "contracts:generate": "cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts && cargo run -p agentdash-contracts --bin generate_contracts_ts",
  "contracts:check": "cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check && cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check"
}
```

## Batch 1: MCP Preset

Move these DTOs from `agentdash-api/src/dto/mcp_preset.rs` into `agentdash-contracts/src/mcp_preset.rs`:

- `McpPresetResponse`
- `CreateMcpPresetRequest`
- `UpdateMcpPresetRequest`
- `CloneMcpPresetRequest`
- `ListMcpPresetQuery`
- `ProbeMcpPresetResponse`

`agentdash-api` imports these DTOs from `agentdash-contracts::mcp_preset`.

Frontend:

- Generate `packages/app-web/src/generated/mcp-preset-contracts.ts`.
- `packages/app-web/src/types/mcp-preset.ts` re-exports generated types.
- `packages/app-web/src/services/mcpPreset.ts` keeps runtime validation and request functions, but removes duplicate string union definitions.

## Later Batches

- Session: start with stream envelope and context response DTOs before runtime state internals.
- Workflow: split by value-object groups, matching backend `workflow/validation.rs` boundary.
- VFS: start from surface/source/edit-capabilities DTOs.
- Shared Library and ProjectAgent: migrate after first three prove generated DTO stability.
