# Agent Runtime Legacy Replacement Manifest

## 判定规则

S5 只删除已经具备 final semantic owner、production caller、composition、behavior
tracer 与 zero legacy consumer 证据的旧 Runtime implementation。Product 业务、
Agent Core `AgentTool` 与 canonical conversation 均不在 manifest 中。

## 已完成且无需重复执行

### `agentdash-agent-types`

该 crate 已在 `e781a136` 从 Git、Cargo workspace 与 `Cargo.lock` 删除。当前路径仅含
Git 不跟踪的空目录，不产生 S5 提交。

### 旧 Runtime schema

`0065_agent_runtime_cutover.sql` 与
`0084_agent_runtime_complete_agent_hard_cut.sql` 已删除旧 `runtime_session_*` 表；
`migration.rs::RETIRED_POSTGRES_TABLES` 与 mailbox repository tests 持续验证其物理
缺席。历史 migration 是数据库演进事实，应由 migration history guard 保留。

## M1 — 旧 Runtime Tool 接入壳

Target production path：

```text
ProductRuntimeToolService / AppliedVfsRuntimeToolService
  -> RuntimeToolExecutor
  -> PlatformToolBroker
  -> Complete Agent Host callback
```

当前 typed catalog 已包含 VFS、Task、Workspace Module Present、Wait、Lifecycle 与
dynamic MCP；Companion request/respond 及 Workspace Module
list/describe/operate/invoke 完成后执行本单元。

可删除：

- VFS、Task、Wait、Lifecycle、Companion 的旧 `RuntimeToolProvider` adapters；
- `RuntimeThreadToolComposer` 与只服务旧 provider 的 tests/context helper；
- SPI `RuntimeToolProvider` trait 与 re-export；
- 零实现者的旧 `HookRuntimeAccess` adapter 在 Companion typed seam 后复审并删除。

需要先迁移：

- `runtime_tools/provider.rs` 中仍被 Companion 使用的
  `RuntimeThreadToolServices` / `SharedRuntimeThreadToolServicesHandle` 移入 Companion
  owner，或由新的 typed Companion service 取代。

保留：

- Agent Core `AgentTool`；
- Product tool command/service/repository/saga；
- `PlatformToolBroker` 与 typed `RuntimeToolExecutor`。

## M2 — Aggregate Hook execution adapter

Target production path：

- Complete Agent callback：
  `AppExecutionHookProvider::evaluate_complete_agent_hook`；
- Product event：
  `AppExecutionHookProvider::evaluate_product_hook_event`；
- immutable plan：
  `AgentFrameHookPlanCompiler` + `load_product_hook_snapshot`。

`ExecutionHookProvider` 当前无 production caller。typed Hook tracers 完成后可删除：

- `ExecutionHookProvider` / `NoopExecutionHookProvider`；
- `AgentFrameHookRefreshQuery` 与 SPI re-export；
- `impl ExecutionHookProvider for AppExecutionHookProvider`；
- 仅由旧 aggregate `evaluate_frame_hook` 消费的 adapter/evaluate_rules；
- rules tests 迁到 typed query 后的 `HookEvaluationQuery`。

保留 Product presets、rules、scripts、effects、typed evaluators、plan compiler、snapshot
loader、resolution/pending log 与 workflow projection。

## M3 — Product surface 旧 capability/command carrier

Legacy：

- `PendingCapabilityStateTransition`；
- `AgentFrameTransitionRecord` 与 declaration/effect records；
- `RuntimeCapabilityTransition`；
- `RuntimeCommandRecord/Status/DeliveryCommand*`；
- frame construction 的 `requested_runtime_commands` 类型链与 replay 分支。

这些 carrier 当前没有外部 producer，只通过旧输入类型链维持编译。target owner 是
Product `AgentFrame`、`AgentRunAppliedResourceSurface`、ProductLaunch 与 Managed
Runtime `Activate/Rebind`。

执行门禁：

```text
RuntimeSurfaceUpdateRequest
  -> new immutable AgentFrame revision
  -> AppliedResourceSurface materialize/commit
  -> Activate/Rebind
  -> exact applied revision tracer
```

该 tracer 通过后删除旧 requested command 字段、参数、transition registry/replay
carrier；保留活跃的 `CapabilityState` projection、Frame surface compiler 与 VFS
compose/merge。

## M4 — `session_persistence` journal/read-model

Legacy：

- `SessionMeta` / `ExecutionStatus`；
- `PersistedSessionEvent` / `SessionEventBacklog` / `SessionEventPage`；
- `SessionCompaction*` / `SessionProjection*` / `SessionLineage*`；
- `NewCompactionProjectionCommit` / `CompactionProjectionCommitResult`；
- `SESSION_PROJECTION_KIND_*`。

Production consumers 为零。target owners：

- Dash Agent `AgentHistory`；
- Complete Agent fork/compaction state；
- `CanonicalConversationRecord`；
- `ManagedRuntimeSnapshot.conversation_history` 与 Managed Runtime change feed。

canonical conversation、fork 与 compaction tracer 通过后，删除这些定义/re-export，并把
`agentdash-agent/src/model/message.rs` 的历史注释改为 Agent history entry
coordinate。

`SessionStoreError/SessionStoreResult` 没有构造者；现存 consumer 只是 AgentRun、
Workflow 与 API 的死错误转换。删除类型及三个转换，不新增同义公共错误。

M3 与 M4 完成后，`session_persistence.rs` 整体归零并删除。

## M5 — API 与 canonical protocol 死分支

### API journal DTO

`AgentRunJournalStreamQuery` 与 `AgentRunJournalEventsQuery` 只有定义、没有 route/caller；
删除二者，保留同文件的 Context Audit DTO。

### `PlatformEvent::ExecutorSessionBound`

该 variant 没有 Rust producer 或 Rust match consumer。Runtime binding 的 final owner
是 `ManagedRuntimeSnapshot.thread_id`、Managed Runtime source binding 与 Product
runtime binding，不是 conversation event。

同一提交删除：

- Rust protocol variant；
- generated `backbone-protocol.ts`；
- frontend `platformEvent.ts` extractor；
- `systemEventPolicy.ts` key；
- `SessionSystemEventCard.tsx` labels/render branch；
- `SessionEntry.tsx` 历史注释；
- session-parity inventory entry。

随后执行 protocol generation freshness 与 Session frontend tests。

## 最终机械顺序

1. 完成 Companion 与 Workspace Module typed tools/tracers；
2. 完成 Product surface update/Rebind tracer；
3. 执行 M1 Runtime Tool adapters/composer/SPI cut；
4. 执行 M2 aggregate Hook adapter cut；
5. 执行 M3 capability/command carrier cut；
6. 执行 M4 journal/read-model/error carrier cut；
7. 执行 M5 API/protocol/frontend dead branch cut；
8. 运行 migration guard、contract freshness、crate dependency gates、Product/Runtime/
   frontend longitudinal tests 与最终全仓负搜索。

