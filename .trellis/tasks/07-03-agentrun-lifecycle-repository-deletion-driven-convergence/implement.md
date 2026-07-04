# 实施规划

## Execution Artifacts

正式执行不再只依赖本文件的阶段列表，而是拆分到 `work-items/` 目录：

- `decisions.md` 定义 research 结论如何进入正式架构决策。
- `inventory.md` 定义 WI-00 代码事实清点、仓储/表/API/frontend 分类和执行前开放问题回填。
- `target-state.md` 定义重构前后状态图和最终收口检查目标。
- `work-items/README.md` 定义工作项索引、依赖图和执行规则。
- `work-items/WI-*.md` 是可分发执行单元，每个文件包含范围、依赖、验收和验证方式。

本文件保留全局阶段顺序，用于说明重构推进节奏；具体分派、验收和执行追踪以工作项文件为准。

## Dispatch Protocol

本任务在 Trellis sub-agent dispatch 模式下执行。主会话负责恢复上下文、选择工作项、派发、接收结果、更新必要的 spec/task artifact、提交和收口；代码实现和变更检查默认由 `trellis-implement` / `trellis-check` sub-agent 完成。

每轮恢复上下文时先运行 `get_context.py` 和 `get_context.py --mode phase`，确认 active task、分支、dirty state 和当前 workflow step；随后按顺序读取 `prd.md`、`design.md`、`implement.md`、`decisions.md`、`inventory.md`、`target-state.md`、`work-items/README.md`，再读取本轮锚定的 `work-items/WI-*.md`。

每个派发 prompt 以 `Active task: .trellis/tasks/07-03-agentrun-lifecycle-repository-deletion-driven-convergence` 开头，并声明 worker 已经是对应的 `trellis-implement` 或 `trellis-check` 角色。派发内容必须绑定具体工作项、decision IDs、允许写入范围、需要读取的 task artifacts/jsonl/spec、验证命令、完成报告格式和交付风险。

`trellis-implement` 负责按锚定工作项完成实现并运行 scoped lint/typecheck。`trellis-check` 负责检查同一工作项 diff，按同一写入边界直接修复机械问题，并重新运行验证。进入最终任务收口前，最后一轮 `trellis-check` 必须从工作项局部检查升级为全任务 affected-scope 检查。

并行派发以工作项和写入集合为边界。主会话可以同时派发互不重叠的 `trellis-implement` worker、只写 `research/` 的 `trellis-research` worker，或在实现 worker 运行时准备后续工作项上下文；同一接口、同一文件集合、同一 migration 链或同一提交边界内的实现保持串行。这样 check worker 看到的是稳定 diff，主会话也能按主题顺序提交，而不是让多个 worker 在同一事实边界上互相覆盖。

并行批次必须在派发前写清每个 worker 的锚定 WI、允许写入路径、互斥路径、预期验证命令和完成后合流顺序。任一并行 worker 发现需要触碰另一个 worker 的写入范围时，先回报主会话，由主会话重新切分或改为串行。

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

## Current Execution Position

截至 commit `2cf181f2 refactor(accepted): 收束 RuntimeSession accepted 边界`，A-E 主体拆迁已经完成：

- `RuntimeSession` 已从产品身份降级为 internal trace/delivery substrate，trace 表已破坏式重命名为 `runtime_session_*`。
- AgentRun 产品 DTO、application read model、frontend workspace 主链路已不再以 `RuntimeSession` 作为用户认知 identity。
- AgentRun start/fork admission、mailbox owner、delivery binding anchor、accepted boundary、fork baseline、AgentFrame context delivery 和 product surface cleanup 已按工作项提交。
- Lifecycle `NodeStarted` 现在由 accepted turn fact 推进；dispatch/materialization 仅 claim node。
- `RepositorySet` 已从多数业务服务 constructor 中拆除，但 AppState/bootstrap/少量 route helper 仍有组合根残留，需要最终拆迁扫尾。
- WI-12 ledger 已记录 mailbox、delivery binding、fork baseline、lifecycle storage 等迁移结论，但 AgentFrame physical surface、transition evidence、final migration guard 仍需收口。

剩余执行不再重跑全局 review，也不再围绕已完成批次微调。当前只执行 R 批次：把最后的组合根残留、产品命名残留、数据库物理形态和长期 spec 一次性清掉。每个 R worker 必须直接删除或收束旧概念；如果只发现无法删除的事实，必须写明保留资格并补到 WI-12 或 spec，不允许留下“以后再看”的松散状态。

## Remaining Parallel Demolition Plan From `2cf181f2`

### Batch R1: Repository Composition Final Demolition

目标：删除业务 route / use case 对大 `RepositorySet` 的直接认知，只允许 composition root 或 bootstrap 以明确 dependency struct 组装依赖。

| Worker | Role | WI | 写入范围 | 互斥边界 | 验证 | 合流 |
| --- | --- | --- | --- | --- | --- | --- |
| R1a Backend composition | `trellis-implement` | WI-11 | `crates/agentdash-api/src/app_state.rs`, `crates/agentdash-api/src/agent_run_mailbox.rs`, `crates/agentdash-api/src/routes/lifecycle_agents.rs`, API route helper deps,必要的 application deps structs/tests | frontend/contracts、migration、AgentFrame physical schema、runtime trace store | `cargo fmt --check`; `cargo check -p agentdash-api -p agentdash-application -p agentdash-application-agentrun -p agentdash-application-lifecycle`; targeted route/service tests | R1a 返回后立即派同范围 `trellis-check`，通过后提交 |

R1a 不得把 `RepositorySet` 换成另一个未命名 service locator。允许保留的唯一形态是 composition root 拥有聚合后的 app state；业务函数和 service constructor 必须看到具名依赖。

### Batch R2: Product Trace Naming And Frontend Boundary

目标：产品 UI 只表达 AgentRun / delivery trace，不再把 raw RuntimeSession 当作可操作工作区实体；保留的 runtime id 必须只出现在 trace/diagnostic/terminal connector 边界。

| Worker | Role | WI | 写入范围 | 互斥边界 | 验证 | 合流 |
| --- | --- | --- | --- | --- | --- | --- |
| R2a Frontend product boundary | `trellis-implement` | WI-09 | `packages/app-web/src/**`, generated contract consumer naming tests, frontend-only docs in WI-09 if needed | backend API contracts、migration、AppState/repository deps | `pnpm --filter app-web run lint`; `pnpm run frontend:check`; app-web targeted tests touched by workspace/control plane | 可与 R1a/R3a 并行；若发现必须改 backend contract，停止并回报主会话串行切分 |

R2a 的验收不是把所有 `runtime_session_id` 字符串删光，而是让产品层命名和 gate 不再以 session identity 做主体判断。诊断 trace、terminal connector 和 generated DTO 中的 trace ref 可以保留，但调用点必须语义清晰。

### Batch R3: Database Physical Design Final Demolition

目标：按 D-016/D-017 和 WI-12 ledger 清理仍过分冗余的表/列/索引：能作为父事实 child 的合并或降级，只有独立查询、并发更新、反向索引或重建成本合理的状态才保留独立表。

| Worker | Role | WI | 写入范围 | 互斥边界 | 验证 | 合流 |
| --- | --- | --- | --- | --- | --- | --- |
| R3a Physical schema sweep | `trellis-implement` | WI-07, WI-12 | `crates/agentdash-domain/src/workflow/agent_frame*`, `crates/agentdash-infrastructure/src/persistence/**agent_frame**`, `crates/agentdash-infrastructure/src/persistence/**session**`, `crates/agentdash-infrastructure/migrations/**`, `work-items/WI-07*`, `work-items/WI-12*` | API AppState/repository cleanup、frontend product cleanup、public contract regeneration unless explicitly required | migration guard; AgentFrame/session repository tests; `cargo check -p agentdash-domain -p agentdash-infrastructure -p agentdash-application-runtime-session -p agentdash-application-agentrun`; `git diff --check` | R3a schema/migration diff 必须由主会话单独合流；若与 R1/R2 返回顺序冲突，migration commit 串行优先 |

R3a 优先处理三类残留：`agent_frames` split write surface 是否合并为 canonical document、`agent_frame_transitions` 是否仍有独立事实资格、fork/runtime diagnostic index 是否仍有 SQL 查询资格。无法删除的表必须给出 file:line 级保留理由和未来重建来源。

### Batch R4: Spec And Full Task Check

目标：把最终事实边界固化到长期规范，并用一个 full-scope checker 证明当前任务可以执行收口。

| Step | Role | 内容 | 验证/输出 |
| --- | --- | --- | --- |
| R4a Spec update | main 或 `trellis-implement` | 更新 `.trellis/spec/`：AgentRun/Lifecycle/RuntimeSession/AgentFrame/Mailbox/repository composition 的最终事实边界 | spec diff 只记录为什么这么做，不记录历史错误清单 |
| R4b Full check | `trellis-check` | 全任务 affected-scope 检查，允许在范围内修复遗漏 | cargo fmt/check, targeted Rust tests, migration guard, contracts check, frontend check, app-web targeted/full tests, `git diff --check` |
| R4c Final commit gate | main | 按 R1/R2/R3/R4 主题顺序提交；最后只剩 PR gate | clean worktree; task work-items records complete |

## Dispatch Discipline From Now On

1. 主会话先确认 clean worktree，再同时派发 R1a/R2a/R3a。主会话等待期间只做不写代码的协调，避免破坏 worker diff。
2. 每个 implement worker 返回后，主会话先看 `git status` 和 changed paths；若写入范围符合矩阵，立即派同范围 `trellis-check`。
3. 每个 check worker 可以在同一范围内修复问题；check 通过后主会话立刻提交该主题，不等其它 worker。
4. migration、generated contracts、public API surface 永远串行合流；并行 worker 触碰这些边界时必须停下汇报。
5. 批次内不允许重复全局 review。所有发现都必须落到“删除/合并/降级/具名保留”的四类结论之一。
6. R1/R2/R3 全部提交后才进入 R4。R4 只做长期 spec、最终 affected checks 和任务收口，不再新增架构概念。

## Execution Discipline

每个批次遵循固定节奏：

1. 主会话确认 clean worktree，写出本批次 worker matrix。
2. 同时派发互不重叠的 `trellis-implement` worker。
3. worker 返回后，主会话按合流顺序只读检查 diff 形状。
4. 对每个主题 diff 立即派同范围 `trellis-check`。
5. check 通过后立即提交该主题；提交后才合流下一个主题 diff。
6. 每个批次结束时更新 WI 验收记录和 WI-12 migration ledger。

批次不以文件数量衡量完成度，而以删除结果衡量完成度：旧入口被删除、旧 owner 被替换、旧事实源被降级或合并、旧 service-locator 依赖被收窄，才允许进入下一批。

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
- hook delivery 不再在 anchored AgentRun 和 unanchored session 间形成两套语义。

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
