# CB04 Implementation Queue

CB04 不再拆成 Trellis 子任务；所有执行入口收束在 Contract Boundary 父任务内部。每个目录是一个 work item，保留 `prd.md`、`design.md`、`implement.md` 和必要 research 证据，不包含 `task.json` 或 Trellis context manifest。

## Items

| ID | Work Item | Status | Parallel Slot | Scope |
| --- | --- | --- | --- | --- |
| CB04-A | `A-mcp-preset-incoming-conversion` | ready | wave 1 | MCP preset request DTO 到 domain command/value 的 incoming conversion 迁到 API adapter/application command boundary |
| CB04-B | `B-agentrun-workspace-snapshot-split` | blocked by runtime coordinate design | later | AgentRun workspace snapshot 从 generated contract DTO 拆出 application read model |
| CB04-C | `C-session-context-usage-projection` | ready | wave 1 | Session context usage 的 SPI `ContextFrame` 分析迁到 application projection，API/stream boundary 映射 DTO |
| CB04-D | `D-capability-catalog-read-model` | blocked by AgentFrame exposure design | later | Capability catalog 从 contract DTO 返回值拆出 application read model |
| CB04-E | `E-routine-llm-settings-reverse-conversion` | ready | wave 1 | Routine / LLM Provider / Settings 的 request DTO reverse conversion 迁到对应 API route mapper |
| CB04-F | `F-backend-access-command-conversion` | ready | wave 2 | Backend access update status/access_mode 的 route-local command parsing 清理 |

## Parallel Plan

- Wave 1 can dispatch CB04-A, CB04-C and CB04-E concurrently. Their write sets are mostly separate; avoid manual edits to generated frontend files and run `pnpm run contracts:check` after merge.
- CB04-E can split internally into Routine+Settings first and LLM Provider second, because LLM provider route is larger and should have a single owner.
- CB04-F is small and can run after wave 1 or in parallel if no other worker touches `crates/agentdash-contracts/src/backend/contract.rs`.
- CB04-B waits for Runtime Coordinate current delivery binding / selection service to settle.
- CB04-D waits for AgentFrame exposure/capability revision design to settle.

## Validation Baseline

```powershell
cargo check -p agentdash-contracts -p agentdash-api -p agentdash-application
pnpm run contracts:check
```

Use each work item `implement.md` for narrower filters during implementation.
