# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CE01 | AgentFrame exposure fact model 决策 | design | pending | D05, D06, D07, D14 | revision / exposure table / capability dimension 三选一 |
| CE02 | PermissionGrant runtime effect 收敛 | design+implementation | blocked_by_CE01 | D05 | approve/revoke/expire 后 AgentFrame capability 与 grant status 一致 |
| CE03 | Canvas expose recovery 顺序 | design+implementation | blocked_by_CE01 | D06, D07 | AgentFrame fact -> live VFS / hook runtime / WorkspaceModule presentation |
| CE04 | WorkspaceModule visibility resolver | design+implementation | blocked_by_CE01 | D14 | base allowlist + runtime refs 从统一 resolver 输出 |
| CE05 | CapabilityResolver granted keys 边界 | design | blocked_by_CE01 | D05 | active grants fold 顺序或 frame revision 唯一事实 |
| CE06 | RuntimeGateway channel admission parity | implementation | tracked_in_CS07 | D13 | 实现 owner 在 Control Surface CS07；本簇负责保持 AgentFrame exposure fact 决策边界 |
