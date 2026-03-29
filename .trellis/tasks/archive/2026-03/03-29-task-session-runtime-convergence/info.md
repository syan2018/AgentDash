# Research Output

## Relevant Specs

- `.trellis/spec/project-overview.md`: Task/Session 边界的根文档，需要持续与本次 ownership model 对齐
- `.trellis/spec/backend/index.md`: 后端边界总览，明确 Task 仍是云端实体，但 Session / Hook Runtime 已成为真实运行时关键面
- `.trellis/spec/guides/cross-layer-thinking-guide.md`: 本次重构跨越 Task / Session / Hook / Workflow / API / Frontend 展示面，必须确保“展示的是实际生效运行时输入”，同时避免把 Task 业务状态误当执行真相
- `.trellis/spec/backend/execution-hook-runtime.md`: 已明确 session hook runtime 是正式运行时 surface，且前端应观察 `/api/sessions/{id}/hook-runtime`
- `.trellis/spec/backend/address-space-access.md`: 统一 mount/path/runtime tool 面的契约，适合约束 Task Session 运行时输入 builder 的 address space 部分
- `.trellis/spec/backend/quality-guidelines.md`: 明确 `SessionMeta.last_execution_status` 才是 session 执行状态真相，也要求 query path 与 bootstrap path 不要各自独立推导

## Code Patterns Found

- 统一 bootstrap/query plan 模式：
  `crates/agentdash-application/src/bootstrap_plan.rs`
  已提供 `build_bootstrap_plan` + `derive_session_context_snapshot`，目标正是统一 bootstrap path 与 query path
- Project/Story query path 基于统一 plan 生成展示面：
  `crates/agentdash-api/src/routes/project_sessions.rs`
  `crates/agentdash-api/src/routes/story_sessions.rs`
- Task dispatch path 独立构建真实 turn context：
  `crates/agentdash-application/src/task/gateway/turn_context.rs`
- Task query path 另起了一套 build 逻辑，已发生漂移：
  `crates/agentdash-application/src/task/context_builder.rs`
  `crates/agentdash-api/src/routes/task_execution.rs`
- Hook runtime 当前真实观察面以 Session 为中心，但仍会把 task_status 打进 metadata/policies：
  `crates/agentdash-application/src/hooks/provider.rs`
  `crates/agentdash-application/src/hooks/rules.rs`
- Task lifecycle 控制动作仍有一部分依赖 `task.status == Running` 判断“是否正在执行”，这是当前实现里最直接的职责混用点：
  `crates/agentdash-application/src/task/service.rs`

## Files to Modify

- `crates/agentdash-application/src/task/gateway/turn_context.rs`
  抽取可复用的 Task Session 运行时输入构建逻辑，避免仅 dispatch path 独享真实面
- `crates/agentdash-application/src/task/context_builder.rs`
  改为复用统一 builder，或直接并入新的 Task Session 运行时输入模块
- `crates/agentdash-api/src/routes/task_execution.rs`
  让 `GET /tasks/{id}/session` 返回与 dispatch path 一致的运行时输入
- `crates/agentdash-application/src/task/service.rs`
  先把 start / continue / cancel 这些 runtime control 的判定依据迁到 Session 执行状态
- `crates/agentdash-application/src/hooks/provider.rs`
  已清理 `task_status` 注入到 hook snapshot / metadata 的路径；后续只保留 owner 绑定信息
- `crates/agentdash-application/src/hooks/rules.rs`
  已移除基于 `active_task_status` 的 stop/completion gate，后续只保留 session/workflow runtime signal
- `crates/agentdash-application/src/workflow/builtins/trellis_dev_task.json`
  已移除 `task_status_in` / `deny_task_status_transition` 等 task-specific contract，后续继续清理 task-specific locator 命名
- `.trellis/spec/project-overview.md`
  需要把 Task 定位修正为“用户态工作项 + 业务状态机 + Session 策略壳”

## Phase 1 Slice

第一阶段只做“统一 Task Session 运行时输入 builder + 把 lifecycle 控制判定切到 Session 执行状态”，不立刻全量重写 TaskStatus 与 workflow completion。

目标：

- dispatch path 与 query path 共用同一份 Task Session 运行时输入构建逻辑
- Task 查询接口返回的 address_space / context_snapshot / workflow 相关信息与实际执行一致
- start / continue / cancel 等控制动作不再把 `task.status` 当成 Session 运行时事实
- hook/workflow 不再内建 `task_status`、`ActiveTaskMeta`、`TaskStatusIn`、`DenyTaskStatusTransition`
- 为后续梳理 Task 业务状态机与 owner 绑定层的剩余边界打底
