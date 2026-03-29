# Task 与 Session 运行职责解耦

## Goal

将 `Task` 从“隐式承担执行生命周期主语”的混合角色中解耦出来，明确为“面向用户的工作项容器、业务状态机、Session 启动策略载体与展示工具”；把真实运行时统一收敛到 `Session + Hook Runtime + TaskSessionRuntimeInputs`。

本次任务先完成：

1. 明确目标模型与跨层契约
2. 识别当前 Task/Session 双主语漂移点
3. 落下第一阶段实现切口：统一 Task 视角读取到的 Task Session 运行时输入与实际 dispatch 路径

## Problem Statement

当前系统已经出现明显的“双主语”问题：

- 底层执行、turn、stream、hook、tool gate、companion、runtime diagnostics 实际由 `Session` 驱动
- 上层应用服务、Task API、部分 lifecycle 控制逻辑仍把 `Task` 当作执行生命周期主语
- `GET /tasks/:id/session` 返回的上下文面与真实 dispatch 路径存在漂移
- `Task.status`、`SessionMeta.last_execution_status`、Hook / Workflow policy 之间仍缺少清晰分工：哪些是业务状态，哪些是运行时事实，当前没有拆干净

这会导致：

- 展示面不是“真实生效的运行时输入”
- 运行时状态与业务状态语义重复，且出现互相污染
- Task 的业务状态机和可操作性边界不清晰
- 后续 runtime/hook/workflow 演进继续被旧的 task-centric 运行时心智牵制

## Target Model

### Session 负责

- start / continue / cancel / resume
- turn lifecycle
- hook runtime / diagnostics / approvals / pending actions
- active workflow runtime
- runtime context / address space / mounts / effective tool surface
- 执行失败、恢复、中断、自动重试等真实运行时语义

### Task 负责

- 用户可见的工作项容器
- 独立业务状态机与用户态可操作性
- 归属关系：`project_id / story_id / workspace_id`
- Session 启动策略：context sources / executor preset / workspace binding / workflow binding
- 展示信息：标题、摘要、最近会话、验收结论
- 可由 Task Session 持有修改 Task 的 tool handle，再由 hook/policy 决定放行或阻挡
- 不再承担 Session 运行时真相，但可以保留自己的业务状态语义

### Workflow / Hook Runtime 负责

- 基于 session runtime signal 判断执行中的 stop gate / completion / phase advance
- 若消费 `Task.status`，应视其为显式业务信号，而不是隐式运行时真相

## Requirements

- `Task` 不再作为 Session 运行时控制的事实来源；所有真实执行状态以 Session 为准
- `Task.status` 可以保留独立业务语义，但 start / continue / cancel / resume 等运行时控制动作应优先依据 Session 执行状态
- Task 页面和 Task API 返回的运行时输入必须来自真实 dispatch 路径
- 统一 Task 视角与 Session 视角的上下文构建入口，消除双路径漂移
- 第一阶段不强行一次性改写全量状态机；优先收敛运行时输入，并为后续边界清理提供基础
- 在文档层明确新的 ownership model，避免后续实现继续回流到 task-centric

## Non-Goals

- 本次不直接完成全量 TaskStatus 枚举重构
- 本次不直接移除全部 task-centric API
- 本次不重做前端页面结构
- 本次不引入数据库兼容层或迁移兼容方案

## Scope

### In Scope

- 新增或更新 spec，定义 Task / Session / Hook Runtime 的 ownership model
- 梳理并统一 Task Session 运行时输入构建路径
- 重构 `GET /tasks/:id/session` 使用真实 dispatch 输入
- 让 lifecycle 控制逻辑逐步从 `task.status` 迁移到 `session execution state`

### Out of Scope

- TaskStatus 全量改名与前端全面适配
- `TaskLifecycleService` 的完整拆分重命名
- 全量移除 Workflow / Hook 对 `task_status` 的业务约束能力

## Acceptance Criteria

- [ ] 形成一份正式 PRD，明确目标模型、边界、分阶段计划和第一阶段改造目标
- [ ] 形成一份 research 输出，列出相关 spec、代码模式和首批修改文件
- [ ] 新的 Task Session 运行时输入构建入口可以同时服务 dispatch 路径与 `GET /tasks/:id/session`
- [ ] `GET /tasks/:id/session` 返回的 context/address_space/workflow 相关信息不再与 dispatch 路径漂移
- [ ] start / continue / cancel 等运行时控制不再把 `task.status` 当作唯一执行真相
- [ ] 相关实现和文档明确表达“Task 保留业务状态机，Session 承担运行时主体”

## Cross-Layer Contracts

### Contract A: Task Session 运行时输入单一来源

- 输入：Task / Story / Project / Workspace / workflow run / executor config
- 输出：统一的 `TaskSessionRuntimeInputs` 或等价结构
- 约束：Task 查询面与实际 dispatch 面必须共用同一构建逻辑

### Contract B: Ownership Model

- Task 保留策略、展示、独立业务状态机与用户态可操作性
- Session 承担运行时语义
- Hook / Workflow 消费 session runtime signal；若读取 task status，应显式作为业务约束

### Contract C: UI Observability

- 前端看到的是“真实生效的运行时输入”
- 不允许返回静态模板说明替代真实 runtime 数据

## Validation Matrix

### Good

- Task 查询接口与 dispatch 路径返回相同的 context/address_space/workflow surface
- active lifecycle mount、declared source warnings、effective mcp binding 在两条路径一致可见

### Base

- 没有 active lifecycle run 时，Task 查询面和 dispatch 面都稳定返回基础运行时输入

### Bad

- Task 查询面缺少 dispatch 真实会使用的 mount/context/warnings
- Task 查询面显示的 executor/workflow surface 与真实执行不一致
- 新增 helper 继续复制已有 builder 逻辑，形成第三条路径

## Technical Notes

- 第一阶段优先提炼统一的 Task Session 运行时输入 builder，并开始把 lifecycle 控制动作从 `task.status` 切到 `session execution state`
- 文档上同步修正 `.trellis/spec/project-overview.md` 中 Task 的定位
- 重点关注：
  - `crates/agentdash-application/src/task/gateway/turn_context.rs`
  - `crates/agentdash-application/src/task/context_builder.rs`
  - `crates/agentdash-api/src/routes/task_execution.rs`
  - `crates/agentdash-application/src/workflow/*`
  - `crates/agentdash-application/src/hooks/*`

## Implementation Plan

### Phase 1

- 统一 Task Session 运行时输入 builder
- 让 Task 查询面复用 dispatch 的真实构建逻辑
- 让 lifecycle 控制动作优先读取 Session 执行状态
- 补充 ownership model 文档

### Phase 2

- 拆分 `TaskLifecycleService` 中的 Task 业务状态投影与 Session 执行控制
- 梳理哪些 Task 状态迁移属于业务状态机，哪些只是运行时投影

### Phase 3

- 让 workflow completion / hook rules 在运行时判断上优先使用 session runtime signal
- 禁止 hook/workflow 内建 `task_status` 等 task-specific 业务约束；若 Task 需要这些语义，应由 Task 自己消费 hook/workflow 结果

### Phase 4

- 为 task session 持有 task tool handle / hook gate 的模型补齐正式边界
- 把 `TaskLifecycleService` 重构为 Task 策略层 + Session 执行层
- 清理遗留 task-centric API 心智
