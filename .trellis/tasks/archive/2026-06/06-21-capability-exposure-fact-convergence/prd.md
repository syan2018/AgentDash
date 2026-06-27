# Capability Exposure 事实源收敛

## Goal

将 PermissionGrant、AgentFrame、Canvas expose、WorkspaceModule visibility 与 RuntimeGateway admission 收敛到 AgentRun effective capability 唯一路径。运行时可见能力只围绕 AgentRun 获取；AgentFrame 提供模型可见 surface revision，PermissionGrant 作为 AgentRun 授权/护栏系统参与投影与准入。

## Decisions

- AgentRun 是 final visible capability 与 runtime admission decision 的唯一访问入口。
- AgentFrame 是 AgentRun model-visible runtime surface 的 revision/snapshot。
- PermissionGrant 负责审批、审计、工具内部准入与工具集拓展请求；Grant state 只由 AgentRun effective capability/admission 服务消费。
- Canvas / WorkspaceModule / hook runtime / VFS surface 都应从 AgentRun capability view 派生或刷新。

## Scope

- PermissionGrant approve / revoke / expire 到 AgentRun admission projection 与 AgentFrame surface revision 的分类一致性。
- Canvas expose 的可恢复事实顺序。
- AgentFrame exposure model：revision、独立 exposure 表或 capability dimension 的取舍。
- WorkspaceModule visibility resolver。
- RuntimeGateway action/channel admission parity。

## Out Of Scope

- 不定义 AgentRun delivery runtime selection；该部分归 `06-21-runtime-coordinate-convergence`。
- 不改变 Grant 的审批/审计语义，只定义其如何作为 AgentRun 授权系统参与最终能力投影。

## Acceptance Criteria

- [ ] `design.md` 定义 AgentFrame capability/exposure fact model 与恢复顺序。
- [ ] `work-items/index.md` 覆盖 D05、D06、D07、D13、D14。
- [ ] 可执行任务不得让 live VFS 或 hook runtime refresh 成为独立事实源。
- [ ] PermissionGrant approve/revoke/expire、Canvas expose、WorkspaceModule visibility 后续任务共享同一 AgentRun owner 决策。
