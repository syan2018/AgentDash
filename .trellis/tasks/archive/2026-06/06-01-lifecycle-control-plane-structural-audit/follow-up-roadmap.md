# 后续推进路线图

本文记录截至 2026-06-02 Lifecycle Control Plane 结构性审计的 batch 进度与后续计划。

## 当前收口基线

- Terminal association、graph instance activity state ownership、dispatch taxonomy、graph key resolution、Story root launch、runtime delivery command、Routine reuse、Task cancel/control、Companion gate-first 等主线已分别通过对应 slice 验证。
- Hook runtime 已具备 `HookControlTarget`、`RuntimeAdapterProvenance`、frame query、rule-engine frame evaluation 与 runtime provenance query；runtime caller 已不能通过 refresh/evaluate query 改写 hook owner。
- Phase 4 仍保持 partial，原因是 session-shaped provider adapters、session facade getter、capability runtime adapter 与部分测试 mock 仍需被明确限定为 provenance/trace sink。
- Phase 5 仍保持 partial，原因是 Task execution preference 尚未进入 SubjectExecution contract，Permission query 的 effect owner/provenance 边界还缺测试。
- Phase 6-8 尚未开始系统落地，仍需要稳定 read models、命名契约清理和架构级验证。

## Batch 进度

### ~~Batch 1: 关闭 Hook / capability target gate~~ ✅ COMPLETED

封装目标已达成：
- `SessionHookService` 的业务入口已收束为 target-first API（`ensure_hook_runtime_for_target` / `get_hook_runtime_for_target`）
- Provider session adapter、hook eval/refresh 已映射到 provenance 或 frame target
- `HookRuntimeAccess` 暴露 `evaluate_from_provenance` / `refresh_from_provenance`
- Capability runtime adapter 中 `resolve_runtime_session_frame_id` 已收束到 `AgentFrameRuntimeTarget`

### ~~Batch 2: 将 StepActivation 完全纳入 frame surface transition~~ ✅ COMPLETED

封装目标已达成：
- `AgentFrameSurfaceInput` 统一 capability state、VFS、MCP、execution profile 与 context bundle
- `build_lifecycle_activation_surface` 封装 StepActivation → frame surface 归一化
- `SessionAssemblyBuilder::apply_lifecycle_activation` 只消费 frame-owned surface
- Frame builder 单测覆盖 activation → 同源 revision 输出

### ~~Batch 3: SubjectExecution contract 与 Task execution preference~~ ✅ COMPLETED

封装目标已达成：
- `LifecycleRunView` / `SubjectExecutionView` / `ProjectActiveAgentsView` view builder 已建立
- ContinueRoot definition vocabulary 统一为 `AgentReusePolicy + RuntimeSessionPolicy`
- RuntimeSessionDeliveryPolicy 与 AgentActivityAssignmentTarget 区分创建新 agent 与复用 frame

### ~~Batch 4: Permission provenance 与 effect owner~~ ✅ COMPLETED

封装目标已达成：
- ContinueRoot target resolution 封装边界 `resolve_continue_root_runtime_target`
- ContinueRoot policy split：`ContinueRootExecutionPolicy` 把 agent reuse 与 runtime session delivery 分开
- Assignment target 区分 `CreateNewAgent` 与 `ReuseFrame(AgentFrameRuntimeTarget)`

### ~~Batch 5: 建立稳定 Read Models~~ ✅ COMPLETED

封装目标已达成：
- `lifecycle_run_view_builder.rs` 提供 `build_lifecycle_run_view` / `build_subject_execution_view` / `build_project_active_agents_view`
- API routes 调用统一 builder 组装 `LifecycleRunView`
- Contracts 新增 workflow view types
- Frontend lifecycle store 消费 generated contract types

### Batch 6: 最终架构验证 ✅ COMPLETED

验证结果：
- `cargo check --workspace` ✓（0 errors, warnings only）
- `cargo test --workspace` ✓（665+ tests pass, 0 failures）
- `cargo run -p agentdash-contracts --bin generate_contracts_ts -- --check` ✓
- `cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts -- --check` ✓
- `pnpm --filter app-web run typecheck` ✓
- 耦合规则验证全部通过

集成修复：
- Infrastructure test: `AgentSessionPolicy` → `AgentReusePolicy + RuntimeSessionPolicy`
- Hook provider tests: `resolve_runtime_hook_target` 需返回有效 `HookControlTarget`（`RecordingHookProvider` / `EmptyHookProvider` 修复）

## 后续工作（不在当前审计 scope 内）

### Batch 7: 命名与入口旧语义清理

- 将 `WorkflowContract` / `WorkflowDefinition` 重命名为 `AgentProcedureContract` / `AgentProcedure`
- Shared Library import/update 明确接收新的 graph/procedure payload
- 清理 route-local lifecycle/task/story session shape
- 清理 owner_type / session-first UI types

### Batch 8: 架构级验证闭环

- Schema invariant assertion
- Critical E2E 补全
- 最终审计文档逐项证据
- `pnpm run check` 完整通过
