# 执行计划：Lifecycle Control Plane 结构性审计

## Purpose

本文规定后续修复顺序：先修事实源与封装边界，再删旧路径和补测试。阶段性收尾后的后续批次索引见 [follow-up-roadmap.md](./follow-up-roadmap.md)。

## 已完成阶段总结

### Phase 1: 固化运行闭环不变量 ✓

- `ActivityRuntimeAssociationResolver` 以 `RuntimeSession -> current AgentFrame -> LifecycleAgent -> AgentAssignment -> LifecycleRun` 解析 terminal/advance provenance。
- `select_assignment_for_runtime_frame` 先 exact launch frame evidence，再按 `graph_instance_id + activity_key` scope 回到原 assignment。
- Terminal resolver 只读取 frame/agent/assignment/run repos，不读取 read model、route DTO 或前端 shape。

### Phase 2: 拆分 LifecycleRun 与 WorkflowGraphInstance ownership ✓

- `WorkflowGraphInstance` 持有 typed `ActivityLifecycleRunState`，engine/scheduler/orchestrator 以 `graph_instance_id` 为推进入口。
- `LifecycleRun` 不再持有 `activity_state`；`active_node_keys` 是派生 projection。
- Postgres migration `0086_drop_lifecycle_run_activity_state.sql` 删除 run 级 `activity_state`。

### Phase 3: 重塑 Dispatch Intent Taxonomy ✓

- `ExecutionIntent` / `ExecutionDispatchResult` 已是 discriminated taxonomy；`AgentLaunchDispatchResult` 不携带 required `assignment_ref`。
- `WorkflowGraphResolver` 作为 dispatch 前置边界解析 `ById` / `ByKey`；missing key 不会创建随机 run/graph/assignment。
- Story root/freeform launch 通过 `AgentLaunchIntent { subject_ref }` 进入 `LifecycleDispatchService::launch_agent`。

### Phase 4: 收束 AgentFrame 作为 runtime surface owner（partial）

已关闭的子项：
- `AgentFrameBuilder` 同源 frame revision 输出（procedure/context/capability/VFS/MCP/runtime refs）
- `StepActivation` live apply target-first（`apply_to_frame_runtime_target`）
- Canvas capability sync target-first
- Hook runtime target-aware caller（workflow/canvas/companion）
- Hook runtime target resolver / lazy rebuild
- Provider/rule-engine frame evaluation（`HookRuleEvaluationQuery`）
- `HookRuntimeAccess` provenance query（`evaluate_from_provenance` / `refresh_from_provenance`）
- ContinueRoot target resolution + policy split
- ContinueRoot definition vocabulary（`AgentReusePolicy` + `RuntimeSessionPolicy`）
- Companion gate target caller
- 多 RuntimeSession selection policy

未关闭（deferred to Batch 1）：
- `StepActivation` 纳入 `AgentFrameBuilder` 内部阶段
- Hook/capability command primary target 完全 frame-first（session facade/provider session adapter 仍保留 delivery adapter 入口）
- `session_id` 仅作为 runtime adapter provenance（hub lazy rebuild 仍用 session lookup）

### Phase 5: 收束业务入口与 interaction/gate（partial）

已关闭的子项：
- Routine Reuse 通过 `LifecycleAgentReuseResolver` 查询
- Task cancel → `SubjectExecutionControlService` + `CancelSubjectExecutionCommand`
- Task view status vocabulary 区分 Cancelled 与 Failed
- CompanionChannel / LifecycleGate / RuntimeNotification 分层（human/parent gate-first）
- Story root/freeform launch 进入 dispatch

未关闭（deferred to Batch 3-4）：
- Task execution command 使用 SubjectExecution contract
- Permission 明确 source runtime session 只是 provenance

## 当前状态

### 构建与验证

- `cargo check --workspace` ✓
- `cargo test --workspace` ✓（665+ tests pass）
- `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` ✓
- `cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check` ✓
- `pnpm --filter app-web run typecheck` ✓

### 集成修复

- 基础设施测试：`AgentSessionPolicy` → `AgentReusePolicy + RuntimeSessionPolicy` 拆分修复
- Hook runtime 测试：`RecordingHookProvider` / `EmptyHookProvider` 需实现 `resolve_runtime_hook_target` 返回有效 `HookControlTarget`
- 上述修复属于 Batch 1 hooks 与 Batch 4 ContinueRoot vocabulary 的跨批次交叉影响

### 耦合规则验证

| 规则 | 状态 | 说明 |
|------|------|------|
| Subject 模块不导入 frame construction/runtime launch | ✓ PASS | 零命中 |
| Session 层不使用 owner_type/owner_id | ⚠ PARTIAL | `owner_type` 用于 `CapabilityScope` routing，是合法的 scope 概念（非 binding/session ownership vocabulary） |
| ActivityAttemptState 不作为 routing key | ✓ PASS | 仅用于 view/assembler 构建 |
| Command path 不接受 read model types | ✓ PASS | View types 只出现在 `lifecycle_run_view_builder.rs` |
| 无 cross-layer prefix fields | ✓ PASS | 仅剩一处注释说明旧方案已清理 |

## 延期事项

### P0 命名债务：`WorkflowDefinition` → `AgentProcedure` 重命名

**状态**：已识别、已延期

**原因**：
- 这是 concept-alignment 设计中识别的 P0 命名债务
- 重命名本身是机械操作但影响范围极大（touches every module importing workflow types）
- 当前命名稳定且功能正确——`AgentProcedure` entity 已存在，只是 Rust 类型路径仍使用 `workflow::` 前缀
- 概念已正确（procedure = 单 Agent 顺序工作流），仅 Rust 类型名是 legacy
- 应作为独立 batch 执行，不与结构性封装边界修复混合

**计划**：Batch 6（命名与入口旧语义清理）统一处理

### 其他延期项

- Phase 6（稳定 Read Models）: `ProjectActiveAgentsView` / `RuntimeSessionTraceView` / unified `LifecycleRunView` builder
- Phase 7（命名清理）: `WorkflowContract` rename / shared-library legacy removal / route-local DTO 清理
- Phase 8（架构级验证）: schema assertion / E2E / 最终审计

## Implementation Rules

- 不以 grep 旧字段消失作为完成标准
- 不允许新增 route-local lifecycle/subject/agent/frame DTO
- 不允许 command path 读取 read-model view 后再写事实源
- 不允许业务模块直接构造 RuntimeSession launch payload
- 不允许前端从 global lifecycle store 拼装 project runtime truth
- 每个新增 service 必须说明自己拥有的事实源、不变量、事务边界或外部依赖隔离价值
