# CB04 Implementation Queue

CB04 不再拆成 Trellis 子任务；所有执行入口收束在 Contract Boundary 父任务内部。每个目录是一个 work item，保留 `prd.md`、`design.md`、`implement.md` 和必要 research 证据，不包含 `task.json` 或 Trellis context manifest。

## Items

| ID | Work Item | Status | Parallel Slot | Scope |
| --- | --- | --- | --- | --- |
| CB04-A | `A-mcp-preset-incoming-conversion` | completed | wave 1 | MCP preset request DTO 到 domain command/value 的 incoming conversion 迁到 API adapter/application command boundary |
| CB04-B | `B-agentrun-workspace-snapshot-split` | blocked by runtime coordinate design | later | AgentRun workspace snapshot 从 generated contract DTO 拆出 application read model |
| CB04-C | `C-session-context-usage-projection` | completed | wave 1 | Session context usage 的 SPI `ContextFrame` 分析迁到 application projection，API/stream boundary 映射 DTO |
| CB04-D | `D-capability-catalog-read-model` | blocked by AgentFrame exposure design | later | Capability catalog 从 contract DTO 返回值拆出 application read model |
| CB04-E | `E-routine-llm-settings-reverse-conversion` | completed | wave 1 | Routine / LLM Provider / Settings 的 request DTO reverse conversion 迁到对应 API route mapper |
| CB04-F | `F-backend-access-command-conversion` | completed | wave 2 | Backend access update status/access_mode 的 route-local command parsing 清理 |
| CB04-G | Session message ref command mapper | completed | tail cleanup | `SessionMessageRefDto -> MessageRef` fork command parsing 迁到 session API route mapper |

## Parallel Plan

- Wave 1 dispatched CB04-A, CB04-C and CB04-E concurrently. Their write sets stayed separate and generated frontend files remained unchanged.
- CB04-E completed Routine, LLM Provider and Settings in one owned route-mapper pass.
- CB04-F completed as a small backend access route-local command parsing cleanup.
- CB04-G completed after review found `SessionMessageRefDto -> MessageRef` was request-facing fork command parsing.
- CB04-B waits for Runtime Coordinate current delivery binding / selection service to settle.
- CB04-D waits for AgentFrame exposure/capability revision design to settle.

## Blocked Follow-Up Tracking

| ID | Blocker | Owning Parent Task | Resume Condition | Next Action |
| --- | --- | --- | --- | --- |
| CB04-B | AgentRun workspace snapshot currently depends on runtime/session/current delivery selection semantics. | `.trellis/tasks/06-21-runtime-coordinate-convergence/` | `LifecycleAgent` current delivery binding and `DeliveryRuntimeSelectionService` design are implemented or at least stabilized as code-level contracts. | Re-open `B-agentrun-workspace-snapshot-split` and split application read model from generated workflow/session DTOs. |
| CB04-D | Capability catalog read model depends on the runtime exposure fact source. | `.trellis/tasks/06-21-capability-exposure-fact-convergence/` | AgentFrame revision becomes the canonical capability/exposure fact and derived visibility reads from latest frame. | Re-open `D-capability-catalog-read-model` and move catalog query output to application read facts before API DTO mapping. |

These blockers are intentionally tracked here so Contract Boundary cleanup does not forget them while Runtime Coordinate and Capability Exposure converge.

## Validation Baseline

```powershell
cargo check -p agentdash-contracts -p agentdash-api -p agentdash-application
pnpm run contracts:check
```

Use each work item `implement.md` for narrower filters during implementation.
