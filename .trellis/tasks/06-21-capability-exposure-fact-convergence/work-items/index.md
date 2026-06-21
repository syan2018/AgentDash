# Work Items Index

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CE01 | AgentFrame exposure fact model 决策 | design | completed | D05, D06, D07, D14 | runtime exposure/capability 变更产生新的 `AgentFrame` revision |
| CE02 | PermissionGrant runtime effect 收敛 | design+implementation | blocked_by_CE05 | D05 | approve/revoke 可先做 AgentFrame revision；expire 需要新增 application-owned per-grant effect path |
| CE03 | Canvas expose recovery 顺序 | design+implementation | blocked_by_CE02 | D06, D07 | AgentFrame fact -> live VFS / hook runtime / WorkspaceModule presentation；复用 persist revision then adopt runtime helper |
| CE04 | WorkspaceModule visibility resolver | design+implementation | blocked_by_CE03 | D14 | base allowlist + selected AgentFrame runtime refs 从统一 resolver 输出 |
| CE05 | CapabilityResolver granted keys 边界 | design | implementation_ready | D05 | active grants 只能产生 frame revision / capability effect，不直接成为 runtime surface fact；作为 CE02 前置边界检查 |
| CE06 | RuntimeGateway channel admission parity | implementation | tracked_in_CS07 | D13 | 实现 owner 在 Control Surface CS07；本簇负责保持 AgentFrame exposure fact 决策边界 |
