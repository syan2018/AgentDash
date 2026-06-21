# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CE01 | AgentFrame exposure fact model 决策 | design | completed | D05, D06, D07, D14 | runtime exposure/capability 变更产生新的 `AgentFrame` revision |
| CE02 | PermissionGrant AgentRun 授权系统收敛 | design+implementation | completed | D05 | Grant 分类/投影边界、tool-level Grant 生产 tool surface 消费、active status expiry 与 persisted AgentFrame adoption 通知已落地 |
| CE03 | Canvas expose recovery 顺序 | design+implementation | ready | D06, D07 | AgentRun capability service -> AgentFrame fact -> live VFS / hook runtime / WorkspaceModule presentation |
| CE04 | WorkspaceModule visibility resolver | design+implementation | blocked_by_CE03 | D14 | 从 AgentRun effective capability view 输出最终可见模块，selected current frame 只作为 AgentRun 内部事实输入 |
| CE05 | AgentRun effective capability 唯一路径 | design+implementation | completed | D05 | AgentRun 输出 final visible capability / admission decision；Grant、live cache、local helper 路径统一折入此边界 |
| CE06 | RuntimeGateway channel admission parity | implementation | tracked_in_CS07 | D13 | 实现 owner 在 Control Surface CS07；本簇负责保持 AgentFrame exposure fact 决策边界 |
