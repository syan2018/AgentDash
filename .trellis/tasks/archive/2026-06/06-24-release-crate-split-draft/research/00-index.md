# Release Crate Split Draft Research Index

## Scope

本目录保存 2026-06-25 对 `.trellis/tasks/06-24-release-crate-split-draft` 的并行 review 结果。目标是把 first-round crate split draft 从方向讨论更新为可执行拆分计划。

## Reports

| Report | Scope | Main Finding |
| --- | --- | --- |
| `session-runtime-crate-review.md` | RuntimeSession / session substrate | `session` 已部分收敛，但 RuntimeSession extraction 仍被 live hub 的 AgentFrame/Lifecycle/Permission/mailbox 依赖阻塞。 |
| `agentrun-lifecycle-crate-review.md` | AgentRun / Lifecycle control plane | AgentRun owns current/resource surface and update/admission; Lifecycle owns dispatch/orchestration/reducer. 两者之间需要 projection/materialization/creation ports。 |
| `api-gateway-vfs-consumer-review.md` | API / RuntimeGateway / VFS / business consumers | API current surface helper 已迁到 AgentRun 命名，Canvas/Extension Project guard 部分完成；API route/helper 仍需要避免直接锁定 implementation DTO。 |
| `import-graph-crate-review.md` | Cargo graph / module imports / extraction waves | Cargo graph 当前不是主要阻塞；`session <-> agent_run` 与 `agent_run <-> lifecycle` 是硬边。RuntimeGateway setup actions 需要先 port 化。 |

## Current Baseline

- `cargo metadata --no-deps --format-version 1` 可运行。
- `agentdash-application-ports` 当前已有 `backend_transport`、`extension_runtime`、`mcp_discovery`、`runtime_gateway_mcp_surface`、`vfs_materialization`。
- 当前 application 关键横向引用计数：
  - `session -> agent_run`: 48
  - `agent_run -> session`: 45
  - `agent_run -> lifecycle`: 36
  - `lifecycle -> agent_run`: 8
  - `agent_run -> vfs`: 16
  - `vfs -> session`: 5
  - `vfs -> lifecycle`: 6

## Synthesis

物理 crate extraction 的第一步不是移动文件，而是补齐 ports，并让 implementation imports 先降到目标方向。最早可抽取的 implementation 不是“已经 MCP port 化的 RuntimeGateway”，而是“完成 MCP + setup action backing port 后的 RuntimeGateway”。RuntimeSession 同理，只有 live hub / launch / adoption / mailbox / effective capability 依赖全部 port 化后才能抽。
