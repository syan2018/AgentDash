# Implementation Plan

## Phase 1: Facts And Entry Consolidation

- 为 AgentRun lifecycle surface 定义 typed projection facts，至少区分 session evidence facts、node runtime facts、SkillAsset projection facts、message stream facts。
- 给 `AgentRunLifecycleSurfaceProjector` 增加场景化入口，替代调用方手写 `AgentRunLifecycleSurfaceInput`。
- 让 workspace query、VFS surface resolver、owner bootstrap、session assembler 调用场景化入口，并移除由 `node_projection.is_some()` 推导 mode 的逻辑。
- 在 VFS surface resolver 中拆出只读 node evidence ref，避免复用可写 node runtime projection 类型。

## Phase 2: Mount Builder Boundary

- 删除 `build_lifecycle_mount` 与 `build_lifecycle_mount_with_ports` wrapper。
- 将 session evidence mount builder 与 node runtime mount builder 降到 lifecycle surface/provider 内部可见，业务层不可直接调用。
- 收束 `project_active_workflow_lifecycle_vfs`、`append_active_workflow_lifecycle_mount`、`LifecycleMountSurface`，让 active workflow mount 由 typed facts 进入 projector。
- 保留 `metadata.scope`、node runtime 写入白名单、SkillAsset projection metadata 作为 provider 必需契约。

## Phase 3: Metadata And Projection Refresh

- 删除重复的 `agent_run_lifecycle_surface` 嵌套 metadata；provider 必需 metadata 成为唯一运行时 contract。
- Projector 负责完整生成 SkillAsset/message stream/node metadata，不再从旧 mount metadata 回读事实。
- 明确 VFS overlay / mount directive 是整 mount replace；lifecycle projection refresh 是 projector 内部的局部事实重算。

## Phase 4: Tests And Specs

- 增加 workspace query 与 VFS resolver 对同一 AgentRun 的 lifecycle mount path/scope/skills 一致性测试。
- 增加 node runtime facts 与 session evidence facts 同时存在时不混淆 scope 的测试。
- 调整现有 mount builder/helper 测试，让测试通过 projector 或 provider-level mount spec 验证。
- 更新 backend spec，记录 owner、projection facts、provider metadata 和 replace/merge 边界的原因。

## Work Items

本任务不创建 Trellis 子任务；所有拆分都作为 `work-items/` 下的工作项维护。

- `work-items/01-lifecycle-surface-facts.md`：typed facts 与 projector 场景化入口。
- `work-items/02-mount-builder-boundary.md`：低层 builder 可见性和 active workflow rebuild helper 收束。
- `work-items/03-projection-metadata-refresh.md`：provider metadata contract 与 projection refresh。
- `work-items/04-backend-legacy-cleanup.md`：后端 dead module、旧 enum/factory/API 和旧 schema 行为清理。
- `work-items/05-frontend-contract-legacy-cleanup.md`：前端旧入口、flat fallback、generated contract legacy 清理。

## Validation

- `cargo fmt`
- `cargo test -p agentdash-application lifecycle::surface`
- `cargo test -p agentdash-application agent_run::workspace`
- `cargo test -p agentdash-application session::assembler`
- `cargo check -p agentdash-api`
- 前端 legacy cleanup 工作项另跑对应 `pnpm` 测试切片。
