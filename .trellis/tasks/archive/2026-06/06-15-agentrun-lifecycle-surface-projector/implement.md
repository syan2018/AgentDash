# Implement Plan

## Phase 0: Planning Confirmation

- [x] 用户确认 projector 标准化方向。
- [x] 用户确认 `node_runtime` 是相对独立且通常归属于 orchestration 的行为。
- [x] 用户确认 `lifecycle` 是唯一通用聚合面，不做也永远不会做双 lifecycle mount。
- [x] 第一阶段保留 node write semantics；projector 只标准化它与 AgentRun identity / optional message stream / SkillAsset projection 在同一个 `lifecycle` aggregate mount 下的路径投影关系。
- [x] 用户指出 session 与 orchestration 的直接关联容易误读；projector contract 应把 AgentRun runtime address / optional message stream 与 orchestration node projection 分开。
- [x] 用户确认运行时对外业务入口应统一面向 AgentRun；RuntimeSession 只属于 message stream / connector trace 语境。
- [x] 进入实现前运行 `trellis-before-dev`，重新读取 backend / vfs / session / cross-layer specs。

## Phase 1: Typed Contracts

- [x] 新增 `AgentRunLifecycleSurfaceInput` / `AgentRunLifecycleSurface`。
- [x] 新增 `AgentRunLifecycleSurfaceMode`。
- [x] 新增 `AgentRunRuntimeAddress`，作为 projector 的业务 runtime address。
- [x] 新增 `MessageStreamProjectionRef`，仅用于 optional `session/*` / transcript / connector trace 投影。
- [x] 新增 `BuiltinLifecycleSkill` / `BuiltinLifecycleSkillPolicy`。
- [x] 新增 `OrchestrationNodeProjectionInput`，作为 node path/artifacts/records 投影与写入策略的事实源。
- [x] 新增 typed lifecycle mount metadata structs，并提供 JSON serialization/parsing helpers。

## Phase 2: Projector Implementation

- [x] 实现 `AgentRunLifecycleSurfaceProjector`。
- [x] projector 从 base VFS 读取并保留同 Project SkillAsset projection metadata。
- [x] projector 根据 builtin policy 调用 `SkillAssetService::bootstrap_builtins`。
- [x] projector 负责合并 explicit skill keys 与 builtin skill keys。
- [x] projector 根据 mode、AgentRun runtime address、optional message stream 和 optional orchestration node projection 构造单个 aggregate lifecycle mount。
- [x] projector 输出 final VFS、lifecycle mount、projection facts、effective skill keys。

## Phase 3: Caller Migration

- [x] 迁移 `OwnerBootstrapComposer::prepare_owner_bootstrap_vfs`。
- [x] 迁移 plain companion child lifecycle projection。
- [x] 迁移 companion+workflow lifecycle skill projection。
- [x] 迁移 `AgentRunWorkspaceQueryService::resolve_agent_run_frame_vfs`。
- [ ] 逐步删除或收窄旧 helper 的 public surface，避免业务调用方继续绕过 projector。

## Phase 4: Tests

- [x] projector unit test: workspace read surface preserves projected skill metadata without bootstrapping new builtins.
- [x] projector unit test: launch evidence surface emits a single aggregate lifecycle mount with AgentRun identity, optional `session/*`, and projected companion-system + explicit skills.
- [x] projector unit test: companion child surface starts from parent slice and emits child AgentRun identity plus optional message stream projection.
- [x] projector unit test: workflow node execution surface keeps orchestration-owned node write projection and projected skills.
- [x] invariant test: non-message runtime surface inputs use AgentRun address, not `runtime_session_id`, as the business index.
- [ ] regression test: workspace query no longer uses implicit `&[]` preserve-only behavior.
- [x] metadata tests: typed metadata roundtrips JSON and rejects missing identity fields.
- [x] invariant test: projector never emits parallel lifecycle mounts such as `lifecycle-session` / `lifecycle-node`.

## Phase 5: Specs

- [x] Update `.trellis/spec/backend/vfs/vfs-access.md`.
- [x] Update `.trellis/spec/backend/embedded-skill-bundles.md`.
- [x] Update `.trellis/spec/backend/session/runtime-execution-state.md`.
- [x] Update workflow/frame construction spec if ownership boundary changes.

## Current Residuals

- `project_active_workflow_lifecycle_vfs` remains in fallback paths that do not yet have a full AgentRun runtime address, and in task context projection. This is intentionally left for the AgentRun runtime entry/session convergence line.
- Provider-level `metadata.scope = "agent_run_session" | "node_runtime"` remains an internal `lifecycle_vfs` dispatch detail. The public construction path is now the projector.
- Workspace query preserve-only behavior is now expressed via `BuiltinLifecycleSkillPolicy::PreserveProjected`; remaining helper tests still exercise legacy helper behavior.

## Validation Commands

```powershell
cargo test -p agentdash-application workflow::lifecycle
cargo test -p agentdash-application workflow::frame_construction
cargo test -p agentdash-application session::assembler
cargo test -p agentdash-application vfs::provider_lifecycle
cargo check -p agentdash-application
```

If generated contracts or frontend resource projection changes:

```powershell
pnpm run contracts:check
pnpm --filter app-web typecheck
```

## Risk Points

- Node write behavior powers artifacts/records; do not accidentally remove write capability in first phase.
- `RuntimeSessionExecutionAnchor` may reference an orchestration node, but node runtime ownership must remain on orchestration/node coordinates.
- Projector contract must not imply that orchestration/node projection is owned by the runtime session.
- Projector contract must not imply that RuntimeSession is the external runtime business index; session is only a message stream / trace ref.
- `lifecycle` must remain a single connector-visible aggregate surface; path availability comes from projection facts.
- Projector must not bootstrap builtin SkillAssets in workspace query preserve-only mode.
- SkillAsset key merge must remain deterministic and deduped.
- Typed metadata must preserve current JSON field names consumed by `LifecycleMountProvider`.
- Existing frame construction paths must keep `CapabilityState.vfs.active` aligned with final VFS.

## Rollback Points

- Typed metadata can land before caller migration.
- Projector can first wrap existing helpers, then inline behavior after tests pass.
- Session-owned node write migration is not a valid follow-up direction because it violates the aggregate lifecycle ownership model.
