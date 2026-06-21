# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CE01 | AgentFrame exposure fact model 决策 | design | completed | D05, D06, D07, D14 | runtime exposure/capability 变更产生新的 `AgentFrame` revision |
| CE02 | PermissionGrant runtime effect 收敛 | design+implementation | ready | D05 | approve/revoke/expire 后通过新 AgentFrame revision 表达 capability effect；grant status 只负责审批/审计 |
| CE03 | Canvas expose recovery 顺序 | design+implementation | ready | D06, D07 | AgentFrame fact -> live VFS / hook runtime / WorkspaceModule presentation |
| CE04 | WorkspaceModule visibility resolver | design+implementation | ready | D14 | base allowlist + runtime refs 从统一 resolver 输出 |
| CE05 | CapabilityResolver granted keys 边界 | design | ready | D05 | active grants 只能产生 frame revision / capability effect，不直接成为 runtime surface fact |
| CE06 | RuntimeGateway channel admission parity | implementation | tracked_in_CS07 | D13 | 实现 owner 在 Control Surface CS07；本簇负责保持 AgentFrame exposure fact 决策边界 |
