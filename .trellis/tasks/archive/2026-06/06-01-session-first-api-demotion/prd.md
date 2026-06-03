# Session First API Demotion

## 目标

删除或降级 session-first API / DTO / repository 主路径，收尾 `LifecycleRun.session_id`、`SessionBinding*`、owner tree、step vocabulary 等遗留控制面入口。

## 依赖

- 父任务：`06-01-session-lifecycle-control-plane-refactor`
- 依赖：`06-01-session-lifecycle-target-anchors-schema`
- 依赖：`06-01-lifecycle-dispatch-service`
- 依赖：`06-01-agent-frame-construction-migration`
- 依赖：`06-01-task-subject-execution-migration`
- 依赖：`06-01-workflow-agent-assignment-migration`
- 依赖：`06-01-frontend-actor-subject-views`

## 蓝图阶段

- 推进：`target-state-blueprint.md` B7 Legacy API And Field Removal。
- 退出贡献：保留 session-first、binding、step 或 single-graph 语义的 legacy fields/APIs/DTOs 被删除或降级为 trace adapters。

## 重构模式

- 采用父任务 `target-state-blueprint.md` 中的 breaking-mode 约束。
- 删除旧 API 和 DTO shapes，而不是维护 compatibility endpoints。
- 如果旧 caller 仍依赖被移除字段，更新或打断该 caller，不恢复字段。

## 需求

- 删除 `LifecycleRunRepository::list_by_session` 主路径。
- 删除 route-local `SessionBindingOwnerResponse`、`SessionBindingResponse`、`binding_id` 主字段。
- 删除或降级 `ListSessionsQuery.owner_type/owner_id`。
- 删除 `LifecycleRun.session_id` 对外 contract 暴露。
- 删除或降级 `LifecycleRun.lifecycle_id` 作为唯一 graph 指针的 contract 暴露；对外改为 run 内 graph instances。
- session API 保留为 RuntimeTrace API：event、projection、lineage、debug、cancel/fork 等 trace 操作。

## 交付物

- 删除或降级 session-first APIs / DTOs / repository path。
- public contracts 不再暴露 `LifecycleRun.session_id` 和唯一 graph pointer 作为业务主路径。
- session routes 明确为 RuntimeTrace routes。
- workflow/task/story/project routes 使用 subject / agent / run contracts。

## 不承担

- 不创建 replacement compatibility endpoint。
- 不通过 read view 回填 command input。
- 不在清理阶段重新设计 dispatch / frame / assignment。

## 验收标准

- [ ] `rg -n "LifecycleRun\\.session_id|list_by_session|SessionBinding|binding_id|owner_type|owner_id|active_step_key|lifecycle_step_key|runsBySessionId"` 不再命中目标事实源路径。
- [ ] top-level lifecycle run contract 不再暴露单一 workflow graph 指针作为唯一执行图。
- [ ] session route 不返回 business owner truth。
- [ ] workflow/task/story/project route 使用 subject / agent / run contracts。
- [ ] API/generated/frontend 类型同步完成。
