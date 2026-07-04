# Fork / lineage / baseline 第一性原理研究

本研究只基于当前代码、测试、migration、contracts、spec 推导，不读取本 task 下既有规划文档或 references。

## 基本真理

### Product fork 的不可约事实

1. Product fork 是一次新的 AgentRun workspace materialization，不是 RuntimeSession 的 UI 别名。
   - Product HTTP surface 已经以 `run_id + agent_id` 暴露 fork / fork-submit：`crates/agentdash-api/src/routes/lifecycle_agents.rs:100-118`。
   - 跨层契约明确 product calls use AgentRun refs，Session id 只是 trace ref，不是 product command route key：`.trellis/spec/cross-layer/frontend-backend-contracts.md:176-184`。

2. Product fork 必须生成新的 child run、child agent、child frame、child runtime session 绑定和 redirect。
   - Fork response DTO 携带 `parent_refs`、`child_refs`、`lineage`、`redirect`：`crates/agentdash-contracts/src/agent/run_mailbox.rs:326-344`。
   - API 把 `redirect` 固定为 child `run_id + agent_id`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:2286-2308`。
   - 前端 fork 后只按 `response.redirect.run_id/agent_id` 导航：`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:147-155`。

3. Product fork 的 child 拥有独立 ownership 和 mailbox；parent 不应被当作继续写入目标。
   - 非 owner composer submit 当前返回 fork outcome，而不是写 parent mailbox：`crates/agentdash-api/src/routes/lifecycle_agents.rs:710-741`。
   - fork-submit 测试确认初始输入只进入 child mailbox，parent mailbox 保持为空：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:1030-1089`。

4. Product fork 必须可幂等 replay。
   - `AgentRunCommandKind` 已把 `agent_run_fork` 与 `agent_run_fork_submit` 纳入 command receipt：`crates/agentdash-domain/src/workflow/command_receipt.rs:39-49`。
   - receipt 保存 command scope、client command id、request digest、accepted refs 与 result_json：`crates/agentdash-domain/src/workflow/command_receipt.rs:86-114`。
   - Postgres claim 以 `scope_kind + scope_key + client_command_id` 查重，digest 不同直接冲突：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_command_receipt_repository.rs:53-68`。

5. Product fork 的 provenance 必须能回答：谁从哪个 parent AgentRun/Agent fork 出哪个 child AgentRun/Agent，在父 runtime trace 的哪个稳定边界。
   - `AgentRunLineage` 已表达 parent/child run+agent、parent/child runtime session、fork point、forked_by、metadata、created_at：`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6-28`。
   - DB `agent_run_lineages` 已约束 `relation_kind = 'fork'`、parent/child run 不同、child run+agent 唯一：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:1-17`。

### fork boundary 应锚定哪些对象

最小正确边界是 **stable message boundary**，语义上落在 **completed turn** 内；`event_seq` 是后端解析出的 cutoff 证据；frame revision 与 context delivery 不属于 fork boundary。

1. MessageRef 是一等稳定边界。
   - `MessageRef` 注释明确：稳定引用对齐 `PersistedSessionEvent` 的 `turn_id + entry_index`，用于 compaction cut、restore、branch lineage：`crates/agentdash-agent-types/src/model/message.rs:6-15`。
   - wire DTO `SessionMessageRefDto` 只包含 `turn_id + entry_index`：`crates/agentdash-contracts/src/runtime/session.rs:364-369`。
   - 前端 round action 从稳定完成轮次的 final agent reply 生成 `turn_id + entry_index`：`packages/app-web/src/features/session/model/roundActions.ts:36-45`，并只在 completed segment 启用 fork：`packages/app-web/src/features/session/model/roundActions.ts:51-87`。

2. Turn 是边界的完整性约束，不是比 MessageRef 更粗的唯一坐标。
   - 后端将 MessageRef 解析为 projection entry 后校验 tool-call/tool-result 组完整性：`crates/agentdash-application-runtime-session/src/session/branching.rs:650-707`。
   - 后端校验被 fork 的非 synthetic turn 已完成：`crates/agentdash-application-runtime-session/src/session/branching.rs:736-765`。

3. event_seq 是解析后的 durable cutoff，不应作为 product API 的主输入。
   - fork 当前通过 MessageRef 查 projected transcript，得到 `source_event_seq` 或 `source_range.end_event_seq`，再用它构建上下文：`crates/agentdash-application-runtime-session/src/session/branching.rs:610-647`。
   - `event_seq` 还会被检查不能超过当前 model context head：`crates/agentdash-application-runtime-session/src/session/branching.rs:330-347`。
   - spec 明确 `turn_id`、`entry_index`、`tool_call_id` 是从 envelope 派生的传输/展示字段，领域判断必须回到 typed event / trace；`event_seq` 是持久化排序与恢复证据，不应替代 typed boundary：`.trellis/spec/cross-layer/backbone-protocol.md:169-176`。

4. Frame revision 是 child execution surface baseline 的来源，不是 conversation fork boundary。
   - `AgentFrame` 是 effective runtime surface snapshot，revision 随 capability/context/VFS/MCP surface 变更产生：`crates/agentdash-domain/src/workflow/agent_frame.rs:6-13`。
   - 当前 fork materialization 从 parent frame 复制 capability/context/VFS/MCP/execution surface 到 child frame：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:130-143`。

5. Context delivery 是 launch-time delivery plan / ordering language，不是 fork cutoff。
   - `ContextDeliveryPlan` 只有 target agent 与按 phase/order 排序的 entries：`crates/agentdash-spi/src/hooks/mod.rs:279-314`。
   - `ContextDeliveryMetadata` 表达本次 turn 如何投递、缓存、展示：`crates/agentdash-spi/src/hooks/mod.rs:336-352`。
   - launch preparation 只是按 `delivery_phase + delivery_order + frame_id` 排序生成 plan：`crates/agentdash-application-runtime-session/src/session/launch/preparation.rs:596-615`。

### AgentRunLineage / AgentLineage / SessionLineage 的不可约职责

1. `AgentLineage`：保留。它是同一 run 内 agent 控制树事实。
   - domain 注释写明它表达 agent spawn/delegation/companion relation，UI 控制树使用 AgentLineage：`crates/agentdash-domain/src/workflow/agent_lineage.rs:5-20`。
   - workspace parent/children 当前由 `agent_lineage_repo.list_by_run` 构建：`crates/agentdash-api/src/routes/lifecycle_agents.rs:546-625`。

2. `AgentRunLineage`：保留，但应收敛成 product fork edge / fork provenance 的唯一事实源。
   - domain 注释写明它是 cross-run AgentRun provenance，AgentLineage 仍是 same-run control tree：`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:6-10`。
   - `agent_run_lineages` 已经是 fork-only 表，`relation_kind` 被 DB check 固定为 `fork`：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:7-17`。

3. `SessionLineage`：不应作为 product fork 事实源。局部最优是删除 product 对它的依赖；若保留，只保留为 RuntimeSession trace diagnostic view。
   - `SessionLineageRelationKind` 当前包含 `Fork / Companion / SpawnedAgent / RollbackBranch`：`crates/agentdash-spi/src/session_persistence.rs:691-708`，这比 product fork 语义更宽。
   - `SessionLineageRecord` 只关联 child/parent session 与 fork point：`crates/agentdash-spi/src/session_persistence.rs:756-772`，没有 product ownership、run/agent refs、mailbox/redirect。
   - Session fork route 明确标注为 internal diagnostics：`crates/agentdash-api/src/routes/sessions.rs:923-966`。

### child baseline 的唯一事实

child baseline 应由一次 **AgentRunForkMaterialization** 产生：输入是 command receipt + parent current delivery snapshot + resolved fork boundary；输出是 child session initial projection + child AgentRun materialization + child mailbox/receipt outcome。

当前代码把 baseline 分成三段：

1. SessionBranchingService 创建 child session、session_lineage、child initial projection：`crates/agentdash-application-runtime-session/src/session/branching.rs:124-154`。
2. PostgresAgentRunForkMaterialization 另一个事务创建 child lifecycle run、agent、frame、anchor、AgentRunLineage：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:185-215`。
3. fork-submit 再调用 mailbox service 写 child mailbox，然后分步 attach/accepted/result receipt：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:343-430`。

这就是 projection / frame / session 三源。正确形态是让一个 fork transaction 产生这些 rows；projection、frame、session 都是 materialized output，不再各自决定 baseline。

## 推荐设计

### 1. Product fork use case

定义一个 product-only use case：

```text
fork_agent_run(parent_run_id, parent_agent_id, actor_user_id, client_command_id, fork_point_ref?)
fork_submit_agent_run(parent_run_id, parent_agent_id, actor_user_id, client_command_id, input, fork_point_ref?)
```

输入只允许：

- parent `run_id + agent_id`
- actor user
- `client_command_id`
- optional `SessionMessageRefDto`
- fork-submit 的 canonical `Vec<UserInput>`
- executor/backend selection 只影响 child mailbox 首条输入，不影响 fork boundary

输出必须是：

- command receipt
- parent refs
- child refs
- fork provenance edge
- optional child mailbox message/outcome
- redirect child `run_id + agent_id`

这与当前 contract 一致：`AgentRunMessageCommandResponse.fork` 表示真实写入目标是 child AgentRun，前端按 `fork.redirect` 导航：`.trellis/spec/cross-layer/frontend-backend-contracts.md:180-184`。

### 2. Fork boundary object

新增或收敛为一个内部值对象：

```rust
struct ResolvedForkBoundary {
    fork_point_ref: Option<MessageRef>,
    fork_point_event_seq: u64,
    parent_projection_kind: &'static str, // model_context
    parent_projection_version: u64,
    parent_head_event_seq: u64,
    parent_active_compaction_id: Option<String>,
}
```

规则：

- Product API 接受 `SessionMessageRefDto`，不接受裸 `event_seq`。
- 后端把 `MessageRef` 解析成 `event_seq`，并验证 tool boundary 与 turn completion。
- 没有 `fork_point_ref` 时，fork 到 parent 当前 model context head。该路径服务 non-owner composer auto-fork 或显式“从当前可见上下文 fork”。
- `event_seq` 只作为 persisted cutoff evidence 存入 lineage/initial projection source refs。
- frame revision 进入 `ParentDeliverySnapshot`，不进入 boundary identity。
- context delivery 不进入 boundary identity。

### 3. Baseline object

定义一个单一 materialization input：

```rust
struct AgentRunForkBaseline {
    receipt_id: Uuid,
    parent_run: LifecycleRun,
    parent_agent: LifecycleAgent,
    parent_frame: AgentFrame,
    parent_runtime_session_id: String,
    boundary: ResolvedForkBoundary,
    actor_user_id: String,
    title: Option<String>,
    metadata_json: Option<Value>,
    submit: Option<ForkSubmitInput>,
}
```

它一次性产生：

- child `sessions` row
- child initial `session_compactions` / `session_projection_segments` / `session_projection_heads`
- child `lifecycle_runs`
- child `lifecycle_agents`
- child `agent_frames` revision 1
- child `runtime_session_execution_anchors`
- `agent_run_lineages` fork edge
- optional child `agent_run_mailbox_messages`
- accepted `agent_run_command_receipts` result

这样 baseline 只有一个事实入口：已 claim 的 fork command + resolved boundary。projection、frame、session 是该事实的三个 materialized views。

### 4. Lineage model

保留两个 product/domain lineage：

1. `AgentLineage`
   - 同一 run 内 agent 控制树。
   - subagent/companion/delegation tree 的唯一事实。
   - DTO 建议改名为 `AgentControlLineageRef`，因为当前 `AgentRunLineageRef` 实际来自 `AgentLineage`，不是 `AgentRunLineage`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:556-625`。

2. `AgentRunLineage`，建议重命名为 `AgentRunForkEdge` 或 `AgentRunForkLineage`
   - 跨 run fork provenance 的唯一事实。
   - `relation_kind` 字段可以删除；当前 DB 已固定为 `fork`，字段不提供信息增量：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:15`。
   - 最小字段：
     - id
     - parent_run_id
     - parent_agent_id
     - child_run_id
     - child_agent_id
     - parent_runtime_session_id
     - child_runtime_session_id
     - fork_point_ref_json
     - fork_point_event_seq
     - forked_by_user_id
     - created_at
   - 可选但建议作为 audit 字段加入：
     - parent_frame_id
     - parent_frame_revision
     - parent_projection_version
     - parent_head_event_seq
     - parent_active_compaction_id

`SessionLineage` 的最精简目标是移出 product fork。若仍需要 RuntimeSession 诊断面，可以用 `agent_run_lineages.parent_runtime_session_id / child_runtime_session_id` 派生 fork trace tree；rollback 属于 projection event，不需要 lineage edge；spawned agent/companion 属于 `AgentLineage`。

### 5. fork-submit / mailbox / receipt / materialization 事务

推荐事务边界：

1. `BEGIN`
2. claim 或 lock command receipt：
   - unique key: `scope_kind + scope_key + client_command_id`
   - digest mismatch -> conflict
   - accepted duplicate -> replay stored result
   - pending duplicate -> conflict / retry later
3. 在同一 transaction 内解析 parent current delivery snapshot：
   - parent run
   - parent agent
   - current frame
   - parent runtime session id
4. 解析 fork boundary：
   - MessageRef -> projection entry -> event_seq
   - validate tool result group
   - validate turn completion
   - capture parent model_context projection head/version/active compaction
5. materialize child：
   - child session
   - child initial model_context projection
   - child lifecycle run/agent/frame/anchor
   - AgentRun fork edge
6. fork-submit 时插入 child mailbox message，绑定同一个 receipt id；只写 durable row，不在 transaction 内做外部 scheduler dispatch。
7. 同一 transaction 内把 receipt 更新为 accepted，写入 accepted_refs、mailbox_message_id、result_json。
8. `COMMIT`
9. commit 后通过 outbox / scheduler wake 消费 child mailbox。

这个组织使 failure outcome 简单：

- transaction 失败 -> 无 child partial rows；receipt 可 terminal_failed 或保持可重试 pending，按错误类型决定。
- mailbox 创建失败 -> fork 不成立；不会留下无首条输入的 fork-submit child。
- receipt accepted/result_json 与 child rows 同时可见；duplicate replay 不需要重新查 parent projection/frame/session。

### 6. 最小仓储 / 表 / port 形态

最小表形态：

- `agent_run_command_receipts`
  - idempotency、accepted refs、mailbox_message_id、result_json。
  - 当前 migration 已有 unique scope command：`crates/agentdash-infrastructure/migrations/0011_agent_run_delivery_command_receipts.sql:28-31`，后续 rename 为 `agent_run_command_receipts`：`crates/agentdash-infrastructure/migrations/0013_agent_run_mailbox.sql:1-8`。

- `agent_run_lineages` 或重命名 `agent_run_forks`
  - product fork edge 唯一事实。
  - child unique，relation kind 不需要泛化。

- existing lifecycle/runtime tables
  - `lifecycle_runs`
  - `lifecycle_agents`
  - `agent_frames`
  - `runtime_session_execution_anchors`
  - `sessions`
  - `session_compactions`
  - `session_projection_segments`
  - `session_projection_heads`
  - `agent_run_mailbox_messages`

最小 port 形态：

```rust
trait AgentRunForkTransactionPort {
    async fn claim_or_replay_and_materialize(
        &self,
        command: AgentRunForkTransactionCommand,
    ) -> Result<AgentRunForkTransactionOutcome, AgentRunForkError>;
}
```

该 port 应拥有写入：

- receipt
- child session/projection
- child run/agent/frame/anchor
- AgentRun fork edge
- optional child mailbox

读取侧保留：

```rust
trait AgentRunForkLineageReadRepository {
    async fn find_parent(child_run_id, child_agent_id) -> Option<AgentRunForkEdge>;
    async fn list_children(parent_run_id, parent_agent_id) -> Vec<AgentRunForkEdge>;
}
```

`AgentRunLineageRepository::create` 不应作为普通 public repository method 暴露给 application service；创建只能走 fork transaction port。

`SessionLineageStore` 不进入 product repository set。若保留 RuntimeSession diagnostic view，应命名为 `RuntimeSessionLineageReadPort`，并从 AgentRun fork edge 或 runtime diagnostic events 派生。

## 删除清单

1. 删除 `AgentRunForkService` 对 `SessionBranchingService::fork_session` 的直接依赖。
   - 当前依赖入口：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:8`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:98-103`。
   - 当前 split call：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:254-289`。
   - 原因：product fork baseline 不应先由 Session service 创建，再由 AgentRun materializer 补齐。

2. 删除补偿式 `cleanup_child_runtime` 作为正常一致性机制。
   - 当前 materialization 失败会 best-effort delete child runtime：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:313-330`、`crates/agentdash-application-agentrun/src/agent_run/fork.rs:568-570`。
   - 目标是单 transaction 内没有 partial child。

3. 删除 product-facing raw Session fork。
   - 当前 route：`crates/agentdash-api/src/routes/sessions.rs:114-121`。
   - 当前 handler 注释为 internal diagnostics：`crates/agentdash-api/src/routes/sessions.rs:923-966`。
   - 目标：product fork 只走 AgentRun route；Session fork 若保留只能作为 diagnostic-only，不参与产品导航、ownership、mailbox。

4. 删除 `SessionLineage` 作为 fork 事实源。
   - 当前 relation kind 过宽：`crates/agentdash-spi/src/session_persistence.rs:691-708`。
   - 当前 record 没有 product refs：`crates/agentdash-spi/src/session_persistence.rs:756-772`。
   - 若仍保留诊断读取，改名为 `RuntimeSessionLineage` 并从 product fork edge 派生。

5. 删除或收敛 `AgentRunLineage.relation_kind`。
   - DB 已固定为 `fork`：`crates/agentdash-infrastructure/migrations/0038_agent_run_lineages.sql:15`。
   - domain `new_fork` 也硬编码 `"fork"`：`crates/agentdash-domain/src/workflow/agent_run_lineage.rs:45-53`。

6. 删除 `AgentRunLineageRepository::create` 的普通写入口。
   - 当前 trait 暴露 create/list/read：`crates/agentdash-domain/src/workflow/repository.rs:136-148`。
   - 创建应隐藏在 fork transaction port 内，避免绕过 receipt/baseline/mailbox 原子性。

7. 重命名 `AgentRunLineageRef` DTO。
   - 当前 workspace `AgentRunLineageRef` 来自 `AgentLineage`，用于 same-run subagent tree：`crates/agentdash-contracts/src/runtime/workflow.rs:1652-1667`。
   - 更准确名称：`AgentControlLineageRef` 或 `AgentTreeLineageRef`。

8. 删除 product request 中的 compaction id fork 参数。
   - Session diagnostic request 当前允许 `fork_point_compaction_id`：`crates/agentdash-contracts/src/runtime/session.rs:380-395`。
   - Product fork 应只接受 stable message ref 或 server-resolved current head；compaction id 属于 projection internals。

## 迁移 / 实施顺序

1. 增加内部值对象与测试：
   - `ResolvedForkBoundary`
   - `ParentDeliverySnapshot`
   - `AgentRunForkBaseline`
   - 覆盖 MessageRef -> event_seq、tool group 完整、turn completed、default current head。

2. 把 `SessionBranchingService` 中可复用的纯函数拆出：
   - MessageRef boundary resolution。
   - parent model context build。
   - child initial projection commit builder。
   - 不再由它直接 create child session / upsert session lineage。

3. 新建 `AgentRunForkTransactionPort` 的 Postgres 实现：
   - 在一个 transaction 内写 receipt、session/projection、lifecycle rows、AgentRun fork edge、optional mailbox。
   - 将 scheduler dispatch 改为 commit 后 outbox / wake。

4. 改写 `AgentRunForkService`：
   - 只做 request validation、authorization 后调用 fork transaction port。
   - duplicate replay 从 receipt result_json 返回，不重新读取 parent projection/frame/session。

5. 收敛 schema：
   - `agent_run_lineages` 可重命名为 `agent_run_forks`。
   - 删除 `relation_kind` 或保留为 generated constant read model。
   - 如需要 audit，补 parent frame/projection snapshot fields。
   - 删除 `session_lineage` 表，或改为 diagnostic projection 表并从 product 写路径移除。

6. 收敛 contracts/frontend：
   - product DTO 继续使用 `AgentRunForkOutcomeView` / `AgentRunMessageCommandResponse.fork`。
   - rename same-run `AgentRunLineageRef`。
   - 确认 product service imports 不再包含 raw Session fork / lineage / rollback helper。

7. 删除旧路径和测试替换：
   - 删除 `SessionForkRequest` product use。
   - 删除 materialization failure cleanup 测试，替换为 transaction rollback 不留 child rows。
   - 增加 fork-submit mailbox failure 不留 child rows。
   - 增加 accepted receipt result_json 与 child rows 同时可见。

8. 验证：
   - Rust unit tests for `agentdash-application-agentrun`、runtime-session fork boundary helper、Postgres repository transaction。
   - contract generation / frontend typecheck。
   - targeted frontend tests for fork redirect and non-owner composer fork outcome。

## 需要验证的代码事实

1. `SessionStoreSet` / Postgres session repository 是否能暴露 external transaction API。
   - 当前 `commit_compaction_projection` 是 store trait 方法：`crates/agentdash-spi/src/session_persistence.rs:911-915`，不接收 transaction。
   - 要做单 transaction，需要把 session projection 写入逻辑移入 Postgres fork transaction，或给 store 增加 tx-aware internal API。

2. mailbox service 的 scheduler side effect 是否能拆成 durable insert + post-commit wake。
   - 当前 fork-submit 在 materialization 后调用 `accept_user_message_for_target(... schedule_on_submit: true ...)`：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:343-362`。
   - 需要确认 scheduler launch/continue 是否已经有 outbox 或可延迟触发机制。

3. 删除 `session_lineage` 是否会损失非 AgentRun runtime diagnostics。
   - 当前 SessionLineageStore 支持 ancestors/descendants/status：`crates/agentdash-spi/src/session_persistence.rs:918-945`。
   - 需要确认是否存在不绑定 AgentRun 的 RuntimeSession fork/companion/spawned trace 入口；如果没有，直接删除更干净。

4. `agent_run_lineages` 当前是否只有 fork write/read。
   - 当前 search 显示写入集中在 fork materialization 与 tests，product workspace parent/children 读取的是 `agent_lineage_repo`，不是 `agent_run_lineage_repo`。
   - 需要在实施前重新 `rg "agent_run_lineage_repo|AgentRunLineageRepository|agent_run_lineages"` 确认没有遗漏 consumer。

5. child frame baseline 是否应该复制 runtime dynamic fields。
   - `AgentFrame` 注释称 `visible_canvas_mount_ids_json` 是运行时追加，不随 revision 复制：`crates/agentdash-domain/src/workflow/agent_frame.rs:24-33`。
   - 当前 fork materialization 复制了 `visible_canvas_mount_ids_json` 与 `visible_workspace_module_refs_json`：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:137-142`。
   - 需要产品决定 fork 是否继承父 run 当时可见的 runtime workspace modules/canvas mounts。

6. child `LifecycleRun.context` / `view_projection` 是否应复制。
   - 当前 materialization 直接复制 parent run context/view_projection：`crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:114-118`。
   - 如果 fork 是新 workspace，只应继承必要 subject/project context 与 fork provenance；不要把 parent read-model projection 变成 baseline truth。

7. no `fork_point_ref` 的语义需要固定。
   - 当前 auto-fork composer submit 传 `fork_point_ref: None`：`crates/agentdash-api/src/routes/lifecycle_agents.rs:715-727`。
   - runtime branching 对 None 使用 current model context head：`crates/agentdash-application-runtime-session/src/session/branching.rs:327-341`。
   - 需要确认 non-owner continue 的目标是“当前 model context head”，而不是“最后完成 turn final assistant message”。

8. command receipt accepted/result_json 是否必须合并为单 update。
   - 当前 `mark_accepted`、`attach_mailbox_message`、`store_result_json` 是分开的 repository calls：`crates/agentdash-application-agentrun/src/agent_run/fork.rs:390-430`。
   - 目标事务应一次写入 accepted refs、mailbox id、result_json，避免 accepted 但无法 replay完整 outcome。

## 结论

局部最优不是再给 SessionLineage、projection head、frame revision 各自补校验，而是把 product fork 收成一个事实：**已 claim 的 AgentRun fork command 在一个 stable message/turn boundary 上 materialize 一个 child AgentRun baseline**。

`AgentLineage` 继续表达 same-run agent control tree；`AgentRunLineage` 表达 cross-run fork provenance；`SessionLineage` 从 product fork 中删除或降级为 RuntimeSession diagnostic projection。child baseline 由一个 fork transaction 产生，projection、frame、session、mailbox、receipt 都是该 transaction 的输出。
