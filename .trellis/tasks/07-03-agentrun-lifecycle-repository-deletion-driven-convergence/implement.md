# 实施规划

## Execution Artifacts

正式执行不再只依赖本文件的阶段列表，而是拆分到 `work-items/` 目录：

- `decisions.md` 定义 research 结论如何进入正式架构决策。
- `inventory.md` 定义 WI-00 代码事实清点、仓储/表/API/frontend 分类和执行前开放问题回填。
- `target-state.md` 定义重构前后状态图和最终收口检查目标。
- `work-items/README.md` 定义工作项索引、依赖图和执行规则。
- `work-items/WI-*.md` 是可分发执行单元，每个文件包含范围、依赖、验收和验证方式。

本文件保留全局阶段顺序，用于说明重构推进节奏；具体分派、验收和执行追踪以工作项文件为准。

## Work Item Mapping

| Phase | Work item |
| --- | --- |
| Phase 0 Evidence Inventory And Invariants | WI-00 Decision Inventory |
| Phase 1 RuntimeSession Product Surface Removal | WI-01 RuntimeSession Product Internalization, WI-09 Projection Permission API Frontend |
| Phase 2 Mailbox Owner Correction | WI-04 Command Mailbox Queue, WI-12 Database Migration Verification |
| Phase 3 AgentRun Admission Boundary | WI-03 AgentRun Admission Boundary |
| Phase 4 Accepted Turn And Frame Commit Boundary | WI-05 Accepted Turn Frame Lifecycle |
| Phase 5 Delivery Binding And Anchor Semantics | WI-06 Delivery Binding Anchor |
| Phase 6 Command Lifecycle Unification | WI-04 Command Mailbox Queue |
| Phase 7 AgentRun Fork And Lineage Rebuild | WI-08 Fork Lineage Baseline |
| Phase 8 AgentFrame And Context Delivery Rebuild | WI-07 AgentFrame ContextDelivery |
| Phase 9 Lifecycle State And Projection Review | WI-09 Projection Permission API Frontend, WI-10 Lifecycle Storage Gates Subjects |
| Phase 10 RepositorySet And Composition Root Cleanup | WI-11 Repository Composition Cleanup |

## Phase 0: Evidence Inventory And Invariants

- 以 `references/adversarial-first-principles-review.md` 作为当前规划的第一性原理输入。
- 固化不可约事实：
  - `AgentRun` 是用户可见工作区和单 Agent 会话身份。
  - `LifecycleRun` 是多 AgentRun 控制面 aggregate。
  - `LifecycleAgent` 是身份，不保存 live runtime 指针。
  - `AgentFrame` 是 append-only / versioned capability + cognition surface。
  - `Mailbox` 是 AgentRun durable queue，不归 RuntimeSession 所有。
  - `RuntimeSession` 是 internal delivery/trace。
  - `RuntimeSessionExecutionAnchor` 是 immutable launch evidence。
- 清点当前所有 P0/P1 偏离点的调用路径、schema、contracts、frontend 使用点。
- 产出仓储/表/port 分类表：独立事实源、父 child fact、父 owned child table、application port、runtime trace store、projection/cache。

验收：

- 每个候选仓储都有保留、合并、降级或删除结论。
- 每个 P0 偏离都有可执行删除路径和 migration 影响说明。
- 后续每一阶段都能说明删除了哪个旧事实源或错误组合方式。

## Phase 1: RuntimeSession Product Surface Removal

- 删除或内部化 raw Session 产品写入口：
  - fork
  - rollback
  - delete
  - title/meta patch
  - tool approval
  - runtime-control mutation
- AgentRun scoped runtime endpoints 不再委托 `sessions::*` route handler。
- RuntimeSession id 从前端产品 identity 中移除：
  - workspace command availability 不再由 `sessionId` gate。
  - header 不展示 raw RuntimeSession id。
  - extension/workspace panel 产品坐标改用 `{ runId, agentId, frameId }`。
- contracts 中 raw `runtime_session_id`、`RuntimeSessionCommandStateDto`、`turn_id` 从 AgentRun command result 的产品事实降级为 diagnostic/delivery evidence。

验收：

- 用户写操作无法绕过 AgentRun scoped API。
- RuntimeSession route 只剩 internal/diagnostic trace 能力。
- 前端主 workspace 不以 `sessionId` 判断 composer/cancel/tool approval/mailbox 可用性。

## Phase 2: Mailbox Owner Correction

- mailbox message/state owner 改为 `run_id + agent_id + message_id`。
- 删除 mailbox 对 `sessions(id)` 的 cascade ownership。
- 删除 runtime-scoped claim 作为 queue ownership。
- 将 `runtime_session_id` 移到 nullable delivery ref、accepted ref 或 `mailbox_delivery_attempts`。
- mailbox move/promote/delete/resume/reorder 统一走 AgentRun command receipt/stale guard。

验收：

- 删除 RuntimeSession 不会删除 AgentRun mailbox durable intent。
- 一个 mailbox item 可以在 runtime session 轮换后继续表达待处理用户意图。
- 所有用户可见 mailbox 写操作都有 command receipt 或明确不需要幂等的设计说明。

## Phase 3: AgentRun Admission Boundary

- 引入 `AgentRunAdmission` 用例边界。
- ProjectAgent start / AgentRun start 原子产出：
  - LifecycleRun / LifecycleAgent or child AgentRun control records。
  - initial AgentFrame revision。
  - immutable runtime execution anchor or delivery trace ref。
  - initial mailbox envelope。
  - outer command receipt accepted refs。
- 删除 API 层首条消息调度职责。
- 删除 start accepted 但 initial mailbox/frame/runtime/receipt 半成品的语义。

验收：

- start 失败不会留下互相不可解释的 run/agent/frame/session/receipt/mailbox 半成品。
- API 只调用 admission，不直接调度 initial delivery。

## Phase 4: Accepted Turn And Frame Commit Boundary

- 将 RuntimeSession launch accepted 与 AgentRun frame/current surface commit 合并成同一 accepted boundary。
- 禁止生产路径使用 noop accepted launch commit。
- frame commit、mailbox accepted refs、command receipt outcome、delivery attempt 状态必须与 accepted turn 同步成功或同步失败。
- Lifecycle node started 由 `AgentRunTurnAccepted` 推进，不由 materialization 推进。

验收：

- 不存在 RuntimeSession accepted success 但 AgentRun frame/current state 丢失的路径。
- `NodeStarted` 只在真实 accepted turn 后出现。
- accepted commit 失败会让整体 launch/turn accepted 失败并可恢复。

## Phase 5: Delivery Binding And Anchor Semantics

- 删除 `LifecycleAgent.current_delivery_*` 作为身份聚合字段。
- 删除 persisted `DeliveryBindingStatus` 作为 current truth。
- 将 current delivery 建模为 explicit attachment/read model，或由 anchor + live state + frame resolver 推导。
- `RuntimeSessionExecutionAnchor` 改为 insert-once/idempotent create。
- 删除 anchor upsert 改写坐标和 `latest_updated_anchor_for_agent` 业务选择。

验收：

- `LifecycleAgent` 只表达身份。
- anchor 是 immutable launch evidence。
- current delivery selection 有唯一策略，不依赖派生缓存遮蔽 anchor。

## Phase 6: Command Lifecycle Unification

- 清点并重构三层事实：
  - user instruction / command receipt
  - queue item / mailbox state
  - delivery attempt / runtime outbox
- 删除 mailbox、receipt、runtime command 各自定义业务终态的重复。
- 明确 cancel、retry、failure、idempotency、stale guard 的唯一事实源。
- hook delivery 不再在 anchored AgentRun 和 unanchored session 间 fallback 成两套语义。

验收：

- 一条用户命令可画成单线状态机。
- 任一状态字段都能归属到 instruction、queue item 或 delivery attempt。
- hook output 的审计、去重、重放语义统一。

## Phase 7: AgentRun Fork And Lineage Rebuild

- `AgentRunForkRecord` 成为唯一 product fork 事实。
- Fork baseline 单一化：parent AgentRun、message/turn boundary、child AgentRun、child baseline、fork owner。
- RuntimeSession lineage 降级为 internal trace provenance 或可重建派生。
- `agent_run_lineages` 不再必填 parent/child runtime session id。
- fork receipt `result_json` 不再承载 child refs/lineage 事实；只保存 idempotent command outcome ref。
- 清理同 run control tree、product fork、runtime trace lineage 的 DTO 命名。

验收：

- product fork 事务不再以 RuntimeSession fork 为第一持久事实。
- fork replay 读取 canonical fork record，而不是 receipt result cache。
- UI/permission 不依赖 raw Session lineage。

## Phase 8: AgentFrame And Context Delivery Rebuild

- AgentFrame 保持为 append-only/versioned surface revision。
- 删除 historical frame revision 原地 append visible canvas/workspace module refs。
- 删除 `effective_capability_json + vfs_surface_json + mcp_surface_json` 覆盖式双源模型。
- 删除 `AgentFrameRepository.get_current(agent_id)` 作为 runtime truth 的概念。
- 引入 `DeliverySurfaceBinding(runtime_session_id, launch_frame_id, current_applied_frame_id, accepted_turn_id)` 或等价边界。
- 引入 `ContextDeliveryRecord` 或等价 accepted input fact，作为 connector input 与 ContextFrame emission 的共同来源。

验收：

- historical AgentFrame revision 可审计且不被 runtime path 原地修改。
- 当前/applied frame 不再由最高 revision 推断。
- ContextFrame 不再由 launch/commit/transition/compaction 多处各自构造事实。

## Phase 9: Lifecycle State And Projection Review

- 将 Lifecycle materialization、orchestration node state、gate、task、execution log 分清：
  - 不可约控制面事实。
  - append-only event。
  - 可重建 projection。
- 评估 `lifecycle_runs.context/orchestrations/tasks/view_projection/execution_log` 中哪些应继续内嵌 JSON，哪些应拆为可约束 child facts 或 read model。
- 所有 projection/read model 标注可重建性：
  - `sessions.last_*`
  - AgentRun workspace snapshot
  - context projection/head/segments
  - Lifecycle view projection
  - resource surface summary

验收：

- projection 与 state/binding 命名不再混用。
- 不可重建且参与决策的 projection 被改名或提升为 state/binding。
- terminal 判断、delivery selection、permission check 不依赖可丢失 projection。

## Phase 10: RepositorySet And Composition Root Cleanup

- 保留唯一 composition root。
- 删除业务服务对全量 `RepositorySet` / `AgentRunRepositorySet` 的依赖。
- 为主要 use case 引入小型 deps struct：
  - `AgentRunAdmissionDeps`
  - `AgentRunCommandDeps`
  - `AgentRunCommandQueueDeps`
  - `DeliveryAttemptDeps`
  - `AgentFrameRevisionDeps`
  - `AgentRunForkDeps`
  - `LifecycleStateDeps`
- 跨聚合写入通过显式 command port / unit of work。

验收：

- 业务服务构造函数只暴露自身 use case 需要的能力。
- repository set 不再作为 service locator 泄漏进 application service。

## Validation

每个实现阶段至少执行：

- Rust 编译检查：`cargo check` 或对应 workspace/package check。
- 前端类型检查：使用项目既有 pnpm 校验命令。
- 相关 repository/service 单元测试。
- 涉及 migration 时执行数据库迁移验证。
- 涉及 API/前端路径时执行 AgentRun workspace 基本流程验证：start、submit、mailbox queue、runtime stream、tool approval、fork/cancel。

## Risk Controls

- 每个阶段必须删除旧入口或旧事实源，避免新旧两套长期并存。
- 数据库表合并必须在使用点和查询需求清点完成后执行。
- `Mailbox` 的物理存储选择要基于队列操作事实，而不是基于领域归属直接决定。
- `RuntimeSessionExecutionAnchor` 作为 immutable reverse index 应保持清晰，不与 current delivery selection 混为同一事实。
- AgentFrame 内部结构可以重建，但不能削弱它作为 capability/cognition surface 基准事实源的地位。
