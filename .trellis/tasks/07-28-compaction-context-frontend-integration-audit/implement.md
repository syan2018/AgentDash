# 实施拆分

本任务只完成评估。以下切片供后续独立实现任务使用，必须按顺序落地；前一切片的 contract 与纵向测试未通过前，不应先做 UI loading。

## 前置依赖与职责

参考任务 `.trellis/tasks/07-28-runtime-session-state-chain` 拥有：

- 单一 `AgentRuntimeConnection/View`；
- Complete Agent `read/changes` baseline、tail、gap reload与presentation overlay；
- 去除Product Workspace中的Runtime execution/commands副本；
- Composer/Stop对统一Runtime view的接入；
- 删除conversation history数组下标作为control cursor。

本任务不得平行实现第二套connection/store。若两个任务并行：

- 本任务可先完成后端Compaction contract、fixtures、checkpoint与context query；
- Compaction前端接线等待统一Runtime view seam稳定；
- 依赖通过类型与测试显式体现，不以任务目录树隐含。

实施门禁：在 `07-28-runtime-session-state-chain` 完成并稳定统一
`AgentRuntimeConnection/View` 前，本任务保持 `planning`，不得运行 `task.py start`。

## Slice 0：固定生产缺陷 fixture

目标：先把当前错误行为变成会失败的生产顺序测试，避免实现继续依赖合成事件。

新增用例：

- compacted prefix 中包含完整 `ToolCall/ToolResult`，retained tail 不包含该 pair；
- retained messages 位于 `CompactionStarted/Applied/Completed` 之前；
- `CompactionFailed { lost: false/true }`；
- active Turn 请求 manual compaction；
- automatic overflow B 期间提交新输入；
- projection 请求乱序返回与 target 切换。

验证点：

- summary provider capture包含工具事实；
- failed/lost不改变 active context revision；
- Product current context包含 retained suffix；
- UI started card不显示 completed；
- command availability与 owner admission一致。

主要文件：

- `crates/agentdash-integration-native-agent/tests/`
- `crates/agentdash-agent/tests/dash_service.rs`
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs`
- `packages/app-web/src/features/session/**/*.test.ts(x)`
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.test.ts`

## Slice 1：修复 P0 context correctness

目标：

- compactor request不再遗漏工具事实；
- failed/lost不再成为成功 presentation 或 context boundary。

工作：

1. 提取 provider-neutral conversation/context recipe builder。
2. 将 `ToolCall/ToolResult` 成对纳入 compactor input；包含 structured interaction outcome。
3. 明确 cut 与 retain membership，禁止 tool pair 被拆断后两边都不可见。
4. Native canonical projection无损表达 succeeded/failed/lost。
5. Product projector临时也只能以明确成功 Applied checkpoint为边界；随后由 Slice 3 完全替换 event-position 算法。
6. 前端 card根据 typed lifecycle显示 terminal。

主要文件：

- `crates/agentdash-integration-native-agent/src/bridge_execution.rs`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs`
- `crates/agentdash-agent/src/dash/history.rs`
- `crates/agentdash-agent/src/dash/service.rs`
- `crates/agentdash-agent-protocol/`
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs`
- `packages/app-web/src/features/session/model/types.ts`
- `packages/app-web/src/features/session/ui/SessionEntry.tsx`
- `packages/app-web/src/features/session/ui/bodies/ContextCompactionCardBody.tsx`

完成标准：

- 被 cut 的每个 tool pair要么进入 summary capture，要么在 retained suffix；
- failed/lost fixture中旧 context recipe与 revision完全不变；
- live、snapshot、reload三条路径的 terminal outcome相同。

## Slice 2：Agent-owned activity、source observation 与单一 command authority

目标：把 Compaction 建模为正式 activity，消除 Product/Runtime/UI 三套规则。

工作：

1. Service API 增加 `AgentActivitySnapshot` 与 `last_compaction_outcome`。
2. Native observation从 Dash folded `active_compaction` 直接构造 activity。
3. Codex adapter用 provider可观察 operation/event构造 Observed activity。
4. 同一次 `SourceObservation` 原子携带compaction state change与对应canonical presentation。
5. 统一Runtime view原样投影activity、commands与source/context revision；不建立新的durable owner。
6. command availability同时考虑 Turn、queued compaction、running/applied compaction与Lost。
7. 删除 Product Workspace active-turn compact特例及Runtime command副本，统一使用同一command authority与stale identity。
8. 明确并实现 `07-17` 已定义的 manual queue + deferred input语义。
9. close/fork/interrupt/cancel按 activity phase显式计算。

主要文件：

- `crates/agentdash-agent-service-api/src/snapshot.rs`
- `crates/agentdash-integration-native-agent/src/service.rs`
- `crates/agentdash-integration-codex/src/complete_agent.rs`
- `crates/agentdash-agent-runtime-contract/src/managed_projection.rs`
- `crates/agentdash-agent-runtime/src/agent_snapshot_projection.rs`
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs`
- `crates/agentdash-application-agentrun/src/agent_run/product_command_facade.rs`
- 相邻任务稳定后的 `AgentRuntimeConnection/View` seam
- generated TypeScript contracts与对应生成源

完成标准：

- 任一 activity phase下，UI command state与owner实际admission一致；
- active Turn请求manual compaction可durable queue；
- compaction期间输入只按既定deferred策略出现一次；
- reload后仍能恢复同一operation id与phase。

## Slice 3：Exact checkpoint 与 AgentContextSnapshot

目标：让 provider与用户使用同一 context materializer。

工作：

1. 在 Dash owner建立 typed `CompactionCheckpoint`。
2. successful Applied commit保存/可重放：
   - source/context revision；
   - summary frame；
   - compacted/retained coordinates；
   - tool pair membership；
   - digest与usage evidence。
3. producer改用 `ContextFrameSection::CompactionSummary`。
4. 提取一个 context recipe materializer，供 provider round与query共同调用。
5. Complete Agent seam增加 `AgentContextSnapshot` query。
6. Native输出 `AgentOwned/Exact`；Codex输出 `AgentObserved/Observed` 与opaque evidence。
7. Runtime view保持context revision/digest/authority/fidelity；完整payload通过revision-bound query读取。
8. Product context query保持contribution顺序、完整frame、records、authority/fidelity/revision/digest，但不成为第二状态owner。
9. 删除 `ItemCompleted` position-based projection和误名 `active_compaction_id`。

主要文件：

- `crates/agentdash-agent/src/dash/history.rs`
- `crates/agentdash-agent/src/dash/service.rs`
- `crates/agentdash-agent-protocol/src/backbone/context_frame.rs`
- `crates/agentdash-agent-service-api/`
- `crates/agentdash-agent-runtime-contract/`
- `crates/agentdash-agent-runtime/`
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs`
- `crates/agentdash-contracts/src/runtime/session.rs`
- generated frontend contracts

数据库：

- 优先从 source-owned durable history确定性物化，不重复持久化易漂移的frame set。
- 若 checkpoint/work item/lease 需要新增数据库结构，必须新增正式 migration；不写兼容双读、字段fallback或旧shape适配。

完成标准：

- Native provider capture与`AgentContextSnapshot.contributions`成员、顺序、digest一致；
- retained suffix完整显示；
- typed summary section在timeline与current context inspector使用同一frame；
- Codex页面明确显示Observed/Opaque。

## Slice 4：Durable compaction worker 与 crash recovery

目标：Started 后进程退出不会永久卡住 source。

工作：

1. 将 compactor执行从HTTP/service future迁移到durable work item。
2. 实现claim/lease、source revision fence和幂等terminal commit。
3. 定义pre-side-effect cancel与post-side-effect outcome unknown -> Lost。
4. 统一Native inner effect与Complete Agent outer receipt的owner：
   - inspect直接读source-owned effect；或
   - 在同一atomic commit提交。
5. 清理旧双账本reconciliation路径，不保留兼容层。

主要文件：

- `crates/agentdash-agent/src/dash/service.rs`
- `crates/agentdash-agent/src/dash/store.rs`
- `crates/agentdash-agent/src/dash/lifecycle.rs`
- `crates/agentdash-integration-native-agent/src/service.rs`
- `crates/agentdash-infrastructure/src/persistence/postgres/`
- 对应 migrations

完成标准：

- crash injection覆盖Started前、provider调用前、provider返回后、Applied后、outer receipt提交前；
- 每个case最终只能是succeeded/failed/lost/cancelled，不得永久Accepted；
- continuation不会在unknown checkpoint上promote。

## Slice 5：统一 Runtime view 上的 Compaction selector、门禁与 Context query

目标：压缩状态从 durable snapshot恢复，完成后自动收敛到正确context revision。

工作：

1. 扩展相邻任务建立的统一Agent Runtime view，消费`active_activity`、typed terminal与context revision。
2. timeline card使用item lifecycle，不再固定completed。
3. Session header/status显示compaction phase。
4. popup local pending只表示命令发送；operation状态来自snapshot。
5. composer submit、duplicate compact、Fork、Close、interaction response使用统一availability。
6. 草稿编辑、附件准备、只读context/workspace浏览保持可用。
7. 同一source observation原子更新activity/commands/context revision与presentation overlay。
8. `context_revision`变化触发context snapshot invalidation/refetch；不由presentation event触发Workspace refresh。
9. context request增加target key、AbortController/request generation与monotonic revision fence。
10. context inspector渲染完整contributions、Exact/Observed、revision、usage freshness和raw frame。
11. 删除Compaction对`controlPlaneModel`事件特判的需求；Product Workspace refresh/error不影响Composer状态。

主要文件：

- `packages/app-web/src/features/agent-run-runtime/model/`
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts`
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts`
- `packages/app-web/src/features/session/model/`
- `packages/app-web/src/features/session/ui/SessionChatView.tsx`
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts`
- `packages/app-web/src/features/session/ui/SessionProjectionView.tsx`
- `packages/app-web/src/features/session/ui/contextFrame/`
- `packages/app-web/src/features/session/ui/composer/`
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`

完成标准：

- manual/automatic compaction都显示同一权威状态；
- reload任一phase都恢复正确；
- 旧target/旧revision响应不能提交；
- terminal后无需用户手动刷新；
- failure/lost后旧recipe仍显示，commands按policy恢复或进入recovery-only。
- 同一AgentRun target只有一个Agent projection connection；
- Workspace HTTP state的reload/failure不会回退Compaction activity或commands。

## Slice 6：规范与纵向质量门

规范更新：

- 修正 `.trellis/spec/cross-layer/frontend-backend-contracts.md` 中
  “latest ContextCompaction event即message boundary”的错误描述，改为checkpoint retained membership。
- 将 activity、command matrix、typed terminal、Exact/Observed recipe写入：
  - `.trellis/spec/backend/agent-runtime-context.md`
  - `.trellis/spec/backend/agent-runtime-native-adapter.md`
  - `.trellis/spec/backend/agent-runtime-persistence.md`
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- 只记录最终契约与为什么如此设计，不记录旧实现。

验证矩阵：

| 场景 | Owner | Provider capture | Runtime/Product | Frontend |
| --- | --- | --- | --- | --- |
| Native manual success | started/applied/succeeded | summary + retained + tools顺序一致 | revision/digest一致 | 状态、门禁、自动刷新 |
| automatic overflow A/B/C | B/C dependency闭合 | C首轮只用新recipe | activity/commands一致 | 无idle假象 |
| failed/lost/cancelled | recipe不变 | 不使用失败checkpoint | typed terminal | 不隐藏旧历史 |
| concurrent input | durable deferred一次 | 输入位置确定 | availability一致 | 提示与实际一致 |
| reload/reconnect | phase可恢复 | 不重复apply | snapshot/live收敛 | overlay最终移除 |
| crash injection | 无永久Accepted | side effect不重复或Lost | inspect终态一致 | recovery状态明确 |
| Surface/append/revoke | recipe成员正确 | 只有active frames | full frame一致 | current/audit分离 |
| Codex | Observed事实诚实 | private context opaque | fidelity不丢 | 不声称Exact |

建议验证命令应按实际受影响package定向执行，避免无关工作区修改影响：

```text
pnpm --filter app-web test -- <targeted tests>
cargo test -p agentdash-agent <targeted tests>
cargo test -p agentdash-integration-native-agent <targeted tests>
cargo test -p agentdash-agent-runtime <targeted tests>
cargo test -p agentdash-application-agentrun <targeted tests>
pnpm dev
```

浏览器复验必须包含中文会话时，遵循仓库 AGENTS.md 的 UTF-8 脚本要求，避免 PowerShell inline pipe破坏中文。

## 交付顺序与阻塞关系

```text
Slice 0 fixtures
  -> Slice 1 P0 correctness
  -> Slice 2 activity/source observation/commands
  -> Slice 3 checkpoint/context recipe
  -> Slice 4 durable recovery
  -> 依赖 runtime-session-state-chain 的统一 AgentRuntimeView seam
  -> Slice 5 frontend selectors/query
  -> Slice 6 specs + full vertical verification
```

Slice 4可在Slice 3 contract稳定后与Slice 5并行实施，但最终验收必须共同通过crash/reconnect纵向测试。
