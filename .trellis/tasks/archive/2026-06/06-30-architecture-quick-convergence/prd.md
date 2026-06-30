# 架构快速收束执行

## Goal

承接 `06-30-module-adversarial-review` 的 Quick / Medium 可执行项，组织一批边界清晰、修改范围可控的并行收束工作，优先修复会影响运行正确性或后续架构收敛的残留问题。

本任务只负责总目标、工作项拆分、跨工作项验收和最终集成复核；具体实现按 `work-items/` 中的工作项分配。

## Source

- Review task: `.trellis/tasks/06-30-module-adversarial-review/`
- Main report: `.trellis/tasks/06-30-module-adversarial-review/adversarial-review.md`
- Scope triage: `.trellis/tasks/06-30-module-adversarial-review/cleanup-scope-triage.md`

## Requirements

- 只纳入 Quick / 部分 Medium 项，不纳入 Design 类问题。
- 每个工作项拥有独立可验证的代码范围和验收标准。
- 工作项之间允许并行，但不得互相扩大范围。
- 数据库字段或 API shape 如需调整，按当前预研阶段直接改到正确形态，并补 migration。
- 不为兼容旧实现保留回退路径。
- Design 类问题只在 review 任务 followups 中记录，不在本任务直接实现。

## Work Items

1. Authority 能力准入快速修复
   - Path: `work-items/01-authority-capability-admission.md`
   - Scope: PermissionGrant tool-level grant、AgentRun admission projection、visible capability/admission 最小分离。
2. Extension 与 Workspace Module 一致性收束
   - Path: `work-items/02-extension-workspace-module-consistency.md`
   - Scope: schema validator、extension invocation workspace resolver、renderer-aware loadability。
3. VFS 与 Local guard rails 收束
   - Path: `work-items/03-vfs-local-guard-rails.md`
   - Scope: runtime tool name guard、workspace root guard、handler-declared scheduling、builtin skill identity。
4. Mailbox steering 语义一致性
   - Path: `work-items/04-mailbox-steering-consistency.md`
   - Scope: delegate steering 与 scheduler steering 的 receipt/status/error 语义统一。
5. Settings preference 事实源收束
   - Path: `work-items/05-settings-preference-convergence.md`
   - Scope: 旧 `user_preferences` 迁入 scoped settings，移除 backend preference port，处理 migration。

## Out of Scope

- LifecycleDispatchService 内部 owner 拆分。
- CompanionGate resolver / delivery adapters 拆分。
- Launch command/source 单一模型。
- AgentRuntimeDelegate 拆 delegate set。
- RuntimeGateway dynamic extension action discovery owner。
- WorkspacePlacementService。
- Desktop profile/claim/settings 下沉。
- Relay prompt typed payload。
- VFS per-mount/path authorization model。

这些进入 `module-adversarial-review/followups/design-backlog.md`。

## Acceptance Criteria

- [x] 每个工作项具备足够指导开发的范围、证据、要求和验收标准。
- [x] 每个工作项只修改其 own scope 内的文件。
- [x] 每个工作项完成 targeted verification。
- [x] 最终复核所有工作项互相不冲突。
- [x] 更新 `06-30-module-adversarial-review` 中对应问题的 resolved/residual 状态。

## Completion Notes

本任务完成 Issue 1/2 的 quick boundary 修复、Issue 9/11/13/14/20/22/24/25/26 的收束实现。Issue 1 仍保留 D1 residual：tool invocation execution entry 尚未完整消费 `AgentRunEffectiveCapabilityPort::admit_tool`，需要在后续设计任务中处理 production boundary。
