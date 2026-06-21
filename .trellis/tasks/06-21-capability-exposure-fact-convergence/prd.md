# Capability Exposure 事实源收敛

## Goal

将 PermissionGrant、AgentFrame、Canvas expose、WorkspaceModule visibility 与 RuntimeGateway admission 收敛到统一运行态能力事实源。运行时可见能力应从 AgentFrame 派生，避免 live VFS、frame JSON、hook runtime refresh、capability state 并列承载事实。

## Decisions

- AgentFrame 是 runtime capability / exposure 的唯一锚定事实源。
- PermissionGrant status 负责审批、审计和生命周期，不直接等于 runtime tool surface。
- Canvas / WorkspaceModule / hook runtime / VFS surface 都应从 AgentFrame exposure 派生或刷新。
- 直接更新 live surface 而不更新 AgentFrame 的路径需要收敛。

## Scope

- PermissionGrant approve / revoke / expire 到 AgentFrame capability effect 的一致性。
- Canvas expose 的可恢复事实顺序。
- AgentFrame exposure model：revision、独立 exposure 表或 capability dimension 的取舍。
- WorkspaceModule visibility resolver。
- RuntimeGateway action/channel admission parity。

## Out Of Scope

- 不定义 AgentRun delivery runtime selection；该部分归 `06-21-runtime-coordinate-convergence`。
- 不改变 Grant 的审批/审计语义，只定义其如何产生 runtime effect。

## Acceptance Criteria

- [ ] `design.md` 定义 AgentFrame capability/exposure fact model 与恢复顺序。
- [ ] `work-items/index.md` 覆盖 D05、D06、D07、D13、D14。
- [ ] 可执行任务不得让 live VFS 或 hook runtime refresh 成为独立事实源。
- [ ] PermissionGrant approve/revoke/expire、Canvas expose、WorkspaceModule visibility 后续任务共享同一 owner 决策。

