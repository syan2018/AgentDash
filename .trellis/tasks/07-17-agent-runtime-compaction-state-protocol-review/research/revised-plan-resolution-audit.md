# Revised Plan Resolution Audit

审计范围仅限原独立报告的 M1–M5、m1–m3，并以当前父 `prd.md`、`design.md`、
`implement.md`、根 manifests 及对应 workstream 为准。

## Resolution Audit

- **M1 — Resolved。** durable saga 的唯一 owner、持久化身份/阶段、跨 owner 提交顺序、
  unknown outcome reconcile、restart 与 `Lost` 语义已进入父契约和实施工作包
  （`prd.md:262-280`；`design.md:659-719`；`implement.md:365-373,422-423`；
  `workstreams/07-product-protocol-cutover/prd.md:15,24-27`；
  `workstreams/09-recovery-final-conformance/prd.md:29`）。剩余契约或用户决策：无。

- **M2 — Resolved。** W2 独占 Agent/Core 物理 move，W8 仅验证该 shape 并独占 legacy
  crate 删除，文件树 ownership 不再重叠
  （`implement.md:142-152,410-427`；
  `workstreams/02-dash-agent-core/prd.md:7-15`；
  `workstreams/08-schema-crate-hard-cut/prd.md:12-18`）。剩余契约或用户决策：无。

- **M3 — Resolved。** Complete Agent surface apply/update/revoke、applied evidence、
  `AgentHostCallbacks` reverse Tool/Hook 及 remote ack/replay/generation 语义已进入父合同、
  W1/W4 与最终 conformance
  （`prd.md:185-202`；`design.md:258-353,1067`；
  `implement.md:103-129,243-264,478-479`；
  `workstreams/01-contracts-crate-skeleton/prd.md:23-29`；
  `workstreams/04-surface-tool-hook/prd.md:23-30`；
  `workstreams/09-recovery-final-conformance/prd.md:27-29`）。剩余契约或用户决策：无。

- **M4 — Resolved。** 已删除 crate 的物理缺席与 Application/API/contracts/SPI/Relay/gateway
  中 `RuntimeSession*` 语义残留清零均已成为父验收、W8 hard cut 和 negative gate
  （`prd.md:354-382,480-485`；`design.md:1013-1031`；
  `implement.md:432-434,488-493`；
  `workstreams/08-schema-crate-hard-cut/prd.md:27-31`）。剩余契约或用户决策：无。

- **M5 — Resolved。** 用户确认的 Companion 映射已进入父文档；平台中立
  `InitialAgentContextPackage` 也已定义 stable ID/schema version、typed variants、
  contribution provenance/revision/digest 与整体 digest，并与 Product/Companion DTO 及
  Agent Surface 分离。Fresh create 原子携带 package，receipt/inspect 返回 applied
  digest/fidelity/source coordinate，Runtime 在 evidence 前不激活 child；
  `TypedNative/CanonicalRendered/Unsupported` admission 及首个普通 `SubmitInput` 的后续
  顺序和语义边界均已冻结
  （`prd.md:185-220,289-306,456-459`；
  `design.md:316-330,398-407,736-806`；
  `implement.md:101-112,158-166,289-299,331-339,380-407`；
  `workstreams/01-contracts-crate-skeleton/prd.md:23-30`；
  `workstreams/02-dash-agent-core/prd.md:23-28`；
  `workstreams/05-native-dash-adapter/prd.md:19-26`；
  `workstreams/06-codex-remote-adapters/prd.md:21-27`；
  `workstreams/07-product-protocol-cutover/prd.md:24-31`；
  `workstreams/09-recovery-final-conformance/prd.md:21-30`）。剩余契约或用户决策：无。

- **m1 — Resolved。** W8 是唯一正式 forward migration owner；W2/W3 只交付
  domain/repository contract、in-memory behavior 与最终 schema/constraint specification，
  不产生待重写的正式 migration
  （`design.md:942-955`；`implement.md:194-214,410-446`；
  `workstreams/03-runtime-host-state/prd.md:7-27`；
  `workstreams/08-schema-crate-hard-cut/prd.md:12-29`）。剩余契约或用户决策：无。

- **m2 — Resolved。** 根 manifests 以父 PRD/design/implement 为最高任务约束；W3/W5
  manifests 已明确旧 Runtime specs 只提供当前行为/测试事实，最终 owner、schema 与 API
  以父 design 为准
  （`implement.jsonl:1-3`；`check.jsonl:1-3`；
  `workstreams/03-runtime-host-state/implement.jsonl:3,5`；
  `workstreams/05-native-dash-adapter/implement.jsonl:4`）。剩余契约或用户决策：无。

- **m3 — Resolved。** `DashAgentCommit` 已把 effect settlement、history append/head CAS、
  derived change 与 continuation intent 收入同一 transaction，并禁止跨事务 append
  handoff；W2/W8 同时承担 crash 与 constraint 闭环
  （`prd.md:388-400`；`design.md:928-940`；
  `implement.md:169-181,422-423`；
  `workstreams/02-dash-agent-core/prd.md:23-27`）。剩余契约或用户决策：无。

结论：原报告八项均已闭环；本审计范围内没有遗留契约，也没有待确认的用户决策。
