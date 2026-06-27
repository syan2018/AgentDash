# CB04 Implementation Queue

CB04 不再拆成 Trellis 子任务；所有执行入口收束在 Contract Boundary 父任务内部。每个目录是一个 work item，保留 `prd.md`、`design.md`、`implement.md` 和必要 research 证据，不包含 `task.json` 或 Trellis context manifest。

## Items

| ID | Work Item | Status | Parallel Slot | Scope |
| --- | --- | --- | --- | --- |
| CB04-A | `A-mcp-preset-incoming-conversion` | completed | wave 1 | MCP preset request DTO 到 domain command/value 的 incoming conversion 迁到 API adapter/application command boundary |
| CB04-B | `B-agentrun-workspace-snapshot-split` | completed | wave 3 | AgentRun workspace snapshot 从 generated contract DTO 拆出 application read model |
| CB04-C | `C-session-context-usage-projection` | completed | wave 1 | Session context usage 的 SPI `ContextFrame` 分析迁到 application projection，API/stream boundary 映射 DTO |
| CB04-D | `D-capability-catalog-read-model` | completed | wave 2 | Capability catalog 从 contract DTO 返回值拆出 application read model |
| CB04-E | `E-routine-llm-settings-reverse-conversion` | completed | wave 1 | Routine / LLM Provider / Settings 的 request DTO reverse conversion 迁到对应 API route mapper |
| CB04-F | `F-backend-access-command-conversion` | completed | wave 2 | Backend access update status/access_mode 的 route-local command parsing 清理 |
| CB04-G | Session message ref command mapper | completed | tail cleanup | `SessionMessageRefDto -> MessageRef` fork command parsing 迁到 session API route mapper |

## Parallel Plan

- Wave 1 dispatched CB04-A, CB04-C and CB04-E concurrently. Their write sets stayed separate and generated frontend files remained unchanged.
- CB04-E completed Routine, LLM Provider and Settings in one owned route-mapper pass.
- CB04-F completed as a small backend access route-local command parsing cleanup.
- CB04-G completed after review found `SessionMessageRefDto -> MessageRef` was request-facing fork command parsing.
- CB04-D completed the capability catalog application read model split; workflow API route now owns contract DTO mapping.
- CB04-B completed the AgentRun workspace/conversation snapshot read model split; lifecycle API adapter now owns browser-facing contract DTO mapping.

## Completed Follow-Up Tracking

| ID | Constraint | Owning Parent Task | Completion |
| --- | --- | --- | --- |
| CB04-B | AgentRun workspace snapshot split preserves settled current delivery/resource surface coordinate semantics. | `.trellis/tasks/06-21-contract-boundary-ownership-audit/` | Completed with `resource_surface_coordinate` mapped at the lifecycle API adapter boundary. |

These follow-ups stay tracked here so Contract Boundary cleanup remains recoverable from the parent work-item queue without scattered Trellis tasks.

## Validation Baseline

```powershell
cargo check -p agentdash-contracts -p agentdash-api -p agentdash-application
pnpm run contracts:check
```

Use each work item `implement.md` for narrower filters during implementation.
