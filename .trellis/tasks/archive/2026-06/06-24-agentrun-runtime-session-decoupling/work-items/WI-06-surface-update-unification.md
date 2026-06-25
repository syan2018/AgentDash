# WI-06 Surface Update Unification

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/permission_runtime_surface_update.rs`
  - `crates/agentdash-application/src/agent_run/runtime_surface_update.rs`
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-application/src/permission/mod.rs`
  - `crates/agentdash-application/src/permission/service.rs`
  - `crates/agentdash-application/src/workspace_module/tools.rs`
  - `crates/agentdash-api/src/routes/permission_grants.rs`
- Tests run:
  - `cargo check -p agentdash-application`
  - `cargo check -p agentdash-api`
  - `cargo test -p agentdash-application permission::service::tests`
  - `cargo test -p agentdash-application invoke_canvas_bind_data_routes_to_host_canvas_use_case`
  - `cargo test -p agentdash-application invoke_canvas_bind_data_runtime_update_preserves_external_integration_skill`
  - `cargo test -p agentdash-application canvas::`
  - `rg -n "AgentFrameBuilder|adopt_persisted_frame_revision_into_active_runtime|AgentRunActiveRuntimeSurfaceAdopter|PermissionRuntimeSurfaceAdopter" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission -g "*.rs"` returned only `#[cfg(test)]` fixture hits.
  - `git diff --check` passed.
- Blockers: 无。
- Handoff summary: Permission grant 的 surface-changing 写入已进入 AgentRun-owned permission runtime surface update service；permission service 只提交 typed `RuntimeSurfaceUpdateRequest` 并接收 AgentRun-owned outcome。Canvas expose/bind 和 WorkspaceModule canvas bind/present/create 继续通过 AgentRun runtime surface update service。`permission` 与 `workspace_module` 中剩余的 `AgentFrameBuilder` 命中均位于 `#[cfg(test)]` 夹具，用于构造初始或切换后的测试 frame。

## Contract-only Variants

- `McpPresetChanged`、`ProjectVfsMountChanged`、`SkillInventoryChanged`、`AgentProcedureContractChanged` 保持为 AgentRun facade owned 的 typed `RuntimeSurfaceUpdateRequest` variants。
- 本 WI 不新增这些 variants 的 production business path，因为当前调用方仍只更新各自 domain facts，本 wave 尚无对应 live surface-changing 路径。typed variants 保留在 AgentRun 中是为了固化公共契约：后续 MCP preset、Project VFS mount、skill inventory、AgentProcedure contract 变化应进入同一个 surface update facade，而不是在模块内新增 frame writer。

## Purpose

Make AgentRun typed update facade the single public entry for surface-changing business paths.

## Dependencies

- `WI-01`
- `WI-02`
- `WI-03`

## Scope

- Route Canvas expose/bind through generic AgentRun update command.
- Move Permission frame-writing adapter under AgentRun or hide it behind AgentRun-owned port.
- WorkspaceModule surface-changing paths submit typed AgentRun update requests only.
- Decide and document handling for currently contract-only update variants: MCP preset, Project VFS mount, Skill inventory, AgentProcedure contract.

## Out Of Scope

- Do not change PermissionGrant domain state machine.
- Do not change Canvas domain storage semantics.

## Deliverables

- Surface-changing business modules do not own `AgentFrameBuilder` or active adoption primitive.
- Regression tests for Canvas bind, Permission apply/revoke, WorkspaceModule update.

## Acceptance

- `rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission` has no public business-path usage except AgentRun-owned adapter exceptions.
- Relevant tests pass.
