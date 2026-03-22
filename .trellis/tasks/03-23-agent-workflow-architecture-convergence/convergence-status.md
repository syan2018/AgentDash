# Agent / Workflow 架构收口落地状态

## 概要

本文档记录 `03-23-agent-workflow-architecture-convergence` 任务的落地成果。

---

## Phase 1: 统一 runtime projection 和 bootstrap 抽象

### 新增抽象

| 文件 | 结构 | 用途 |
|------|------|------|
| `agentdash-application/src/workflow/projection.rs` | `ActiveWorkflowProjection` | 统一的 workflow 运行时投影，包含 run + definition + phase + target + resolved bindings |
| | `WorkflowProjectionSnapshot` | 可序列化的快照视图，供前端消费 |
| | `WorkflowTargetSummary` | target 的 kind / id / label 三元组 |
| | `resolve_active_workflow_projection()` | 从 repos 解析当前 active workflow projection |
| `agentdash-application/src/bootstrap_plan.rs` | `SessionBootstrapPlan` | 统一 session 上下文计划，包含 owner / executor / address_space / mcp / working_dir / tool_visibility / runtime_policy / workflow |
| | `BootstrapPlanInput` | 构建 plan 的输入 |
| | `BootstrapOwnerVariant` | owner 级别差异标记 (Task / Story / Project) |
| | `build_bootstrap_plan()` | 从输入构建统一 plan |
| | `derive_session_context_snapshot()` | 从 plan 派生前端可用的 SessionContextSnapshot |

### 重构

- `execution_hooks.rs` 中的 `resolve_active_workflow` 私有函数已被替换为共享的 `resolve_active_workflow_projection()`
- `execution_hooks.rs` 中所有 workflow 相关函数已改为接受 `&ActiveWorkflowProjection` 参数

---

## Phase 2: 让 query snapshot 与实际 bootstrap 共用同一条管线

### 迁移的端点

| 端点文件 | 函数 | 变更 |
|----------|------|------|
| `routes/task_execution.rs` | `build_task_session_context_response` | 改为 `build_bootstrap_plan()` + `derive_session_context_snapshot()` |
| `routes/story_sessions.rs` | `build_story_session_context_response` | 同上 |
| `routes/project_sessions.rs` | `build_project_session_context_response` | 同上 |

### 清理的死代码

- `session_context.rs` 中的 `SessionContextInput`、`ExecutorSummaryInput`、`SessionOwnerVariant`、`build_session_context()` 已移除
- 对应的 unused imports（`McpServer`、`Project`、`ExecutionAddressSpace` 等）已清理

---

## Phase 3: 把 workflow binding 解析迁移到 runtime resolver，移除 current_dir() fallback

### 变更

- `workflow/binding.rs` 的 `candidate_roots()` 函数移除了 `std::env::current_dir()` fallback
- 没有 workspace 时，document / journal binding 会正常走 unresolved 路径
- 两个调用者（`execution_hooks.rs` 和 `projection.rs`）都已传入正确的 workspace 上下文

---

## Phase 4: 把 WorkflowAssignment 接入默认业务主线

### 新增

| 文件 | 结构/函数 | 用途 |
|------|-----------|------|
| `workflow/assignment_resolution.rs` | `ResolvedAssignment` | 解析结果：definition + run + newly_created 标记 |
| | `ResolveAssignmentInput` | 输入参数：project_id + role + target_kind + target_id + session_binding_id |
| | `resolve_assignment_and_ensure_run()` | 根据 project 的默认 assignment 自动解析 workflow 并创建/恢复 run |

### 关键规则

- 同一 owner + role 只能有一个默认 assignment
- 同一 target 同时只能有一个 active run
- 幂等：重复调用返回已有 active run 而非创建新 run
- 新建 run 时自动激活首个 phase（如果不需要 session 或已提供 session_binding_id）

---

## Phase 5: 收口 workflow 写入 authority

### 变更

- Domain 层新增 `WorkflowProgressionSource` 枚举：`HookRuntime` / `ManualOverride`
- `WorkflowPhaseState` 新增 `completed_by: Option<WorkflowProgressionSource>` 字段
- `WorkflowRun::complete_phase()` 签名新增 `completed_by` 参数
- Hook runtime 调用路径传入 `HookRuntime`
- Manual API route 调用路径传入 `ManualOverride`

### Authority 规则

| 操作 | Authority | 调用路径 |
|------|-----------|----------|
| 自动 phase 推进 | `HookRuntime` | `execution_hooks.rs` → `advance_workflow_phase()` |
| 手动 phase 完成 | `ManualOverride` | `routes/workflows.rs` → `complete_workflow_phase()` |
| Artifact 追加 | 无推进权 | `routes/workflows.rs` → `append_workflow_phase_artifacts()` |

---

## Phase 6: 前端收口、测试与文档补齐

### 前端类型更新

- 新增 `WorkflowProgressionSource` 类型
- `WorkflowPhaseState` 新增 `completed_by` 可选字段
- 新增 `WorkflowTargetSummary` 接口
- 新增 `WorkflowProjectionSnapshot` 接口

### 测试覆盖

| 模块 | 测试 | 数量 |
|------|------|------|
| `agentdash-domain` | entity + value_objects | 11 |
| `agentdash-application` | bootstrap_plan + session_plan + workflow (assignment_resolution, catalog, completion, definition, projection, run) + task | 36 |
| `agentdash-api` | execution_hooks + routes + address_space_access + relay + task_agent_context | 40 |
| **总计** | | **87** |

---

## 最终完成定义核对

| 条件 | 状态 |
|------|------|
| `WorkflowAssignment` 已进入默认执行主线 | ✅ `resolve_assignment_and_ensure_run()` 已实现 |
| session query snapshot 与实际 bootstrap plan 共线 | ✅ 3 个 query 端点已迁移到 `build_bootstrap_plan` |
| workflow binding 不再依赖进程 `current_dir` | ✅ `candidate_roots()` fallback 已移除 |
| 自动 completion authority 已唯一化 | ✅ `completed_by` 字段区分 HookRuntime / ManualOverride |
| 前端消费统一 runtime projection | ✅ `WorkflowProjectionSnapshot` 类型已添加 |
| 关键链路有集成测试覆盖 | ✅ 87 个单元/集成测试 |
| 架构文档更新为目标态与现状一致 | ✅ 本文档 |
