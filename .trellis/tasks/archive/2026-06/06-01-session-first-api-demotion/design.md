# Session First API Demotion 设计

## 目标

删除或降级保留 session-first、binding、step 或 single-graph 语义的 legacy fields / APIs / DTOs / repository 主路径，收尾整轮控制面重构。保留的 session route 明确为 RuntimeTrace API。

## 蓝图阶段

推进 `target-state-blueprint.md` B7 Legacy API And Field Removal。

## 存量结构分析

### 需要删除的 API routes

| Route | 当前作用 | 处置 |
| --- | --- | --- |
| `GET /stories/{id}/sessions` | session-binding-based story session list | 删除；替代为 `GET /stories/{id}/runs` (已有) + subject view |
| `POST /stories/{id}/sessions` | 创建 story session binding | 删除；替代为 dispatch intent |
| `GET /stories/{id}/sessions/{session_id}` | session binding detail + context | 删除；替代为 run/agent/frame view |
| `DELETE /stories/{id}/sessions/{session_id}` | unbind story session | 删除；替代为 association 管理 |
| `LifecycleRunRepository::list_by_session` | 从 session 反查 run | 删除主路径；只保留 `AgentFrameRepository::find_by_runtime_session` 用于 trace 反查 |

### 需要删除的 DTOs / Response types

| DTO | 当前作用 | 处置 |
| --- | --- | --- |
| `SessionBindingResponse` | story/project session list item | 删除；替代为 `SubjectExecutionView` / `LifecycleSubjectAssociationDto` |
| `SessionBindingOwnerResponse` | 旧 binding owner shape | 删除 |
| `StorySessionDetailResponse` | session detail with VFS/runtime/context | 删除；替代为 `AgentFrameRuntimeView` |
| `CreateStorySessionRequest` | 创建 story session 请求 | 删除；替代为 `ExecutionIntent` |
| `UnboundBindingResponse` | unbind 响应 | 删除 |
| `TaskSessionResponse` / `TaskExecutionResult.session_id` | task session 返回 | 已在 task migration 中处理 |
| `StoryRunOverviewDto.session_id` | run overview 中的 session_id | 删除字段 |

### 需要删除的 fields / columns

| Field | 位置 | 处置 |
| --- | --- | --- |
| `LifecycleRun.session_id` | domain entity / DB column / DTO | 删除列（migration）；删除 entity 字段 |
| `LifecycleRun.lifecycle_id` 作为唯一 graph pointer | DTO 暴露 | 对外改为 `graph_instances[]`；内部保留为 root graph backfill ref |
| `ListSessionsQuery.owner_type` / `owner_id` | session query API | 删除或降级为 project_id-only filter |
| `binding_id` in any response | route-local binding identity | 全面删除 |
| `EffectiveSessionContract.active_step_key` | step vocabulary | 改为 `active_activity_key` 或删除 |
| `LifecycleExecutionEntry.step_key` | 旧 step execution log | 改为 `activity_key` |

### 需要保留为 trace adapter 的路径

| 路径 | 保留原因 | 约束 |
| --- | --- | --- |
| `GET /sessions/{id}` | RuntimeTrace 视图入口 | 不返回 business owner truth |
| `GET /sessions/{id}/events` | event stream / SSE polling | 保留为 trace |
| `POST /sessions/{id}/cancel` | 取消 runtime session | 保留；cancel 通过 agent/frame 路由 |
| `POST /sessions/{id}/fork` | fork runtime session | 保留为 trace 操作 |
| `GET /sessions/{id}/projection` | context projection | 保留为 trace |
| `GET /sessions/{id}/lineage` | session lineage | 保留为 trace；不推断 ownership |
| `AgentFrameRepository::find_by_runtime_session` | trace 反查 | 仅用于 `RuntimeSession → AgentFrame → LifecycleAgent → LifecycleRun` |

### 重命名规划

| 当前名称 | 目标名称 | 范围 |
| --- | --- | --- |
| `story_sessions.rs` route file | 删除整个文件 | API routes |
| `SessionBindingResponse` | 删除 | contracts / API DTO |
| `StorySessionDetailResponse` | 删除 | contracts / API DTO |
| `session_construction::build_session_context_plan` | 仅保留 trace adapter 调用 | API internal |
| `LifecycleRunLink` → `LifecycleSubjectAssociation` | 已在 target-anchors-schema 完成 | 此任务验证无残留 |
| `StoryRunOverviewDto` | → `LifecycleRunView` | 已在 frontend-views 完成 | 此任务验证无残留 |

## 不变量

- `rg "LifecycleRun\.session_id|list_by_session|SessionBinding|binding_id|owner_type|owner_id|active_step_key|lifecycle_step_key|runsBySessionId"` 不再命中目标事实源路径。
- top-level lifecycle run contract 不暴露单一 workflow graph 指针作为唯一执行图。
- session route 不返回 business owner truth。
- workflow / task / story / project route 使用 subject / agent / run contracts。
- API / generated / frontend 类型同步完成。

## 断裂点

- 删除 `story_sessions.rs` 后，任何依赖 `/stories/{id}/sessions` 的前端路径将 404。前端应已在 `frontend-actor-subject-views` 中迁移。
- 删除 `LifecycleRun.session_id` 列需要 DB migration；在此之前所有读写路径必须已不依赖此列。
- `list_by_session` 删除后，任何残余的 session-first run lookup caller 将编译失败。
