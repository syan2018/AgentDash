# 实施拆分

本任务先完成评估，再在同一任务内按切片实施；不创建子任务。前一切片的contract与纵向测试未通过
前，不应先做UI loading。

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

实施门禁：Slice 0/1不依赖`AgentRuntimeConnection/View`，在本次设计修订获用户确认后即可
`task.py start`。Slice 5前端接线等待相邻任务稳定统一Runtime view seam；不得平行实现第二套
connection/store。

## 当前实施状态

- [x] Slice 0：精确context prefix、tool pairing、正式Turn与失败边界fixture已固定。
- [x] Slice 1：原Session Compaction Turn、共享materializer与canonical lifecycle已实现。
- [ ] Slice 2及以后：等待后续实施；Slice 5继续依赖相邻任务的统一Runtime view seam。

## Slice 0：固定生产缺陷 fixture（已完成）

目标：先把当前错误行为变成会失败的生产顺序测试，避免实现继续依赖合成事件。

新增用例：

- 同一history在normal Turn与Compaction Turn materialization下产生结构相同的context prefix；
- prefix包含active ContextFrames、previous summary、完整`ToolCall/ToolResult`与retained suffix；
- Compaction Turn只在末尾追加一次synthetic instruction，且request tools为空；
- synthetic instruction与provider summary candidate不成为普通conversation records；
- canonical顺序为`TurnStarted -> ItemStarted(ContextCompaction) -> Applied ->
  ItemCompleted -> TurnCompleted`；
- `CompactionFailed { lost: false/true }`；
- retained boundary不会拆分tool call/result pair。

验证点：

- Compaction provider capture精确复用normal Turn context prefix；
- 不存在`effective_conversation`式第二套summary transcript；
- failed/lost不改变 active context revision；
- failed/lost不产生成功`ItemCompleted`或context boundary；
- live、snapshot与reload中的Compaction都属于一个正式Turn。

主要文件：

- `crates/agentdash-integration-native-agent/tests/`
- `crates/agentdash-agent/tests/dash_service.rs`
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs`

## Slice 1：修复 P0 context correctness（已完成）

目标：

- compactor作为原Session上的正式Turn运行，复用精确context prefix；
- failed/lost不再成为成功 presentation 或 context boundary。

工作：

1. 删除`effective_conversation`与独立summary transcript lane。
2. 在Dash owner建立dedicated Compaction Turn primitive，直接使用同一`DashProvider`。
3. context materializer使用显式模式：normal Turn排除单独提交的active input；Compaction Turn保留
   当前完整session prefix。
4. Compaction Turn只追加一次synthetic instruction，本轮tools为空；历史tool pairs保持原样。
5. provider output只返回summary candidate，不写入普通conversation records。
6. retained boundary从同一history/materialization coordinate计算，禁止第二套projector拆分tool pair。
7. source history与Native canonical projection补齐正式Turn lifecycle。
8. failed/lost以带error的Turn terminal结束，不提交成功item/context boundary。
9. Product projector临时也只能以明确成功Applied checkpoint为边界；随后由Slice 3替换
   event-position算法。

主要文件：

- `crates/agentdash-integration-native-agent/src/bridge_execution.rs`
- `crates/agentdash-integration-native-agent/src/canonical_projection.rs`
- `crates/agentdash-agent/src/dash/core_execution.rs`
- `crates/agentdash-agent/src/dash/history.rs`
- `crates/agentdash-agent/src/dash/service.rs`
- `crates/agentdash-agent-protocol/`
- `crates/agentdash-application-agentrun/src/agent_run/session_context_projection.rs`

完成标准：

- Compaction request prefix与normal Turn context完全一致，只多一个末尾压缩指令；
- Compaction不再建立或维护平行summary transcript；
- synthetic records不污染canonical conversation；
- 成功/失败/lost都具有完整Turn terminal；
- failed/lost fixture中旧 context recipe与 revision完全不变；
- live、snapshot、reload三条路径的terminal outcome相同。

## Slice 2：Agent-owned Turn state、source observation 与单一 command authority

目标：把Compaction Turn的kind/phase投影到统一Runtime execution view，消除
Product/Runtime/UI三套规则。

工作：

1. Service API的active Turn snapshot补充`kind/phase`与`last_compaction_outcome`，不新增平行activity
   owner。
2. Native observation从Dash folded Turn与compaction state构造同一个active Turn。
3. Codex adapter用provider可观察operation/event构造Observed Compaction Turn。
4. 同一次`SourceObservation`原子携带Turn state change与对应canonical presentation。
5. 统一Runtime execution view原样投影Turn、commands与source/context revision；不建立新的
   durable owner。
6. command availability同时考虑normal Turn、queued/running/applied Compaction Turn与Lost。
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

- 任一Turn phase下，UI command state与owner实际admission一致；
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
Slice 0 Compaction Turn fixtures
  -> Slice 1 same-session P0 correctness
  -> Slice 2 activity/source observation/commands
  -> Slice 3 checkpoint/context recipe
  -> Slice 4 durable recovery
  -> 依赖 runtime-session-state-chain 的统一 AgentRuntimeView seam
  -> Slice 5 frontend selectors/query
  -> Slice 6 specs + full vertical verification
```

Slice 4可在Slice 3 contract稳定后与Slice 5并行实施，但最终验收必须共同通过crash/reconnect纵向测试。
