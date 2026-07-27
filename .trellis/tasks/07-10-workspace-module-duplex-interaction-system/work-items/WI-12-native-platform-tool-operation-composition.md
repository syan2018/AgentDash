# WI-12 原生平台工具 Operation 组合

Status: done

Depends On: WI-01、WI-03、WI-11

## Problem

OperationGateway 已统一 MCP、Extension、Interaction 与 host action，但 VFS、进程和 Task
原生工具仍只能由 Agent 逐个回调。它们没有 exact OperationRef，因此不能进入 Workspace Module
describe、OperationScript allowed manifest 和 nested admission。

直接让 OperationGateway 调用任意 Runtime executor 会把 catalog、Product authorization、applied
surface 与 callback delivery evidence揉进 application execution core，并可能递归暴露
`workspace_module_*`、`operation_script_*` 等控制工具。

## Target Contract

```text
explicit Operation exposure registry
  -> PlatformToolOperationProvider
  -> OperationGateway
  -> PlatformToolBroker
  -> current Product binding + applied resource surface authorization
  -> native executor
```

- Runtime tool 注册不自动成为 Operation；显式 exposure 必须声明 provider、effect、replay、
  actor visibility、capability、schema 和 provenance。
- V1 只暴露 `mounts_list`、`fs_read`、`fs_glob`、`fs_grep`、`fs_apply_patch`、`shell_exec`、
  `task_read`、`task_write`。
- `workspace_module_*`、`operation_script_*`、lifecycle、companion、wait 等控制工具不进入
  exposure registry。
- OperationGateway 只依赖窄 `PlatformToolOperationAccess` seam，不依赖 Broker、Runtime executor
  或 Complete Agent callback DTO。
- API adapter 从 Agent principal 解析 Product runtime binding 与 current applied resource surface，
  每次调用重新进入 PlatformToolBroker；Broker 继续独占 permission、effect 与 resource grant。
- server-side nested invocation 只携带执行所需坐标，不伪造 Complete Agent service/profile/bound
  surface 或 Host generation。
- Workspace Module 将显式暴露项投影为 `builtin:vfs`、`builtin:process`、`builtin:task`；原生工具
  capability 决定具体 Operation 可见性，Workspace Module extension/canvas allowlist 不隐藏已获授权
  的 builtin capability。

## Write Set

- `crates/agentdash-agent-runtime/src/platform_tool_broker.rs`
- `crates/agentdash-agent-runtime-host/src/runtime_tool_handler.rs`
- `crates/agentdash-application-operation-gateway/src/operation/`
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs`
- `crates/agentdash-application/src/workspace_module.rs`
- `crates/agentdash-application/src/runtime_tools/workspace_module_product.rs`
- `crates/agentdash-domain/src/workspace_module/skills/workspace-module-system/SKILL.md`
- 相关 backend specs 与父任务设计文档

## Exit Criteria

- [x] 八个 V1 原生工具具有 exact `platform:*` OperationRef 和完整 descriptor。
- [x] Agent 可从 `builtin:*` describe 取得这些 refs，并与其他 module Operation 一起通过
      OperationScript preflight/run 组合。
- [x] direct invoke 与 OperationScript nested invoke 都重新进入 OperationGateway 与
      PlatformToolBroker authorization。
- [x] 未显式暴露的 Runtime/control tool 不会因 catalog 注册而成为 Operation。
- [x] server nested invoke 不伪造 Complete Agent callback delivery evidence。
- [x] provider、builtin projection、Broker authorization 与拒绝路径 focused tests 通过。
- [x] Skill、PRD/design 与 backend specs 同步。

## Validation

- `cargo test -p agentdash-application-operation-gateway platform_tool_operations`
- `cargo test -p agentdash-application workspace_module`
- `cargo check` 覆盖受影响 Rust crates。
- `git diff --check`

## Progress

- 2026-07-27：确认现状只支持原生工具直接 callback，尚无 canonical Operation exposure。
- 2026-07-27：按 deep-module 原则固定显式 exposure seam、窄 provider adapter 与 Broker
  execution authority。
- 2026-07-27：实现 `platform:vfs:*`、`platform:process:*`、`platform:task:*` provider 与
  `builtin:vfs`、`builtin:process`、`builtin:task` projection；Agent-visible descriptor 先按
  actor/capability 裁切。
- 2026-07-27：将 Product adapter 独立为 composition module；server nested context 删除
  Complete Agent service/profile/bound-surface 伪造字段，Host generation 改为可选 delivery evidence。
- 2026-07-27：provider、exposure registry、Workspace Module、Broker、Product authorizer focused
  tests与受影响 crates `cargo check` 通过；embedded Skill validation 与 `git diff --check` 通过。
