# 云端能力对齐

## 目录对应

| 能力 | AgentDash | multica | 观察 |
| --- | --- | --- | --- |
| API router | `crates/agentdash-api/src/routes.rs` | `server/cmd/server/router.go`, `server/internal/handler` | AgentDash router 集中注册；multica handler 文件更细且测试密集 |
| 业务服务 | `crates/agentdash-application` | `server/internal/service/task.go`, listeners/scheduler | multica task lifecycle service 是最值得细读的云端状态机 |
| 事件系统 | `stream.rs`, session stream, relay registry | `server/internal/events`, `server/internal/realtime`, `server/pkg/protocol/events.go` | multica 业务事件命名和 scope WS hub 清晰 |
| 数据查询 | repository traits + sqlx impl | `server/pkg/db/queries/*.sql`, `generated/*.go` | multica 查询文件即产品行为索引 |
| 迁移 | `crates/agentdash-infrastructure/migrations` | `server/migrations/*.up.sql` | multica 迁移数量多，体现产品迭代线索 |
| 权限/身份 | auth middleware、project grants、workspace bindings | workspace membership、PAT、invitation、middleware | multica 团队协作权限更产品化 |
| 统计/用量 | 部分 task/session 状态 | task_usage、runtime_usage、daily rollup | multica 对运行成本和活跃度更可观测 |

## 关键差异

AgentDash 云端是“编排内核”：Project/Story/Task/Session/Workflow/Lifecycle/VFS/Hook/Plugin 共同构成一个可扩展执行平台。multica 云端是“协作平台”：Workspace/Issue/Comment/Agent/Runtime/Inbox/Autopilot 让 agent 像团队成员一样工作。

AgentDash 的优势：

- 明确 Rust crate 分层，domain/application/infrastructure/api 边界强。
- SessionHub + BackboneEnvelope 能表达细粒度执行过程。
- Workflow/Lifecycle/Hook/VFS 能支撑复杂编排。

multica 的优势：

- 业务事件与用户可见状态变化对齐，`issue:*`、`task:*`、`comment:*`、`inbox:*` 等命名直接。
- Issue/Comment/Inbox/Activity/Subscriber 形成完整协作闭环。
- `agent_task_queue`、`task_message`、usage rollup 支撑执行历史、成本、恢复。
- handler/service/query/test 覆盖密度高，很多边界 bug 已被沉淀成测试。

## 值得学习的机制

1. **领域事件前缀与用户感知粒度**  
   参考 `references/multica/server/pkg/protocol/events.go`。AgentDash 可在 session stream 之外建立 Project/Story/Task/Runtime/Inbox 业务事件层。

2. **Task lifecycle service**  
   参考 `references/multica/server/internal/service/task.go`。其 queued/dispatch/progress/completed/failed/cancelled、retry、workspace resolve、analytics、inbox 通知都在同一状态机周围闭环。

3. **Query 文件作为产品行为索引**  
   参考 `references/multica/server/pkg/db/queries/*.sql`。AgentDash repository 分层保留，但可为复杂列表/统计建立更显式的 SQL/测试覆盖。

4. **运行时用量与每日 rollup**  
   参考 `runtime_usage.sql`、`task_usage.sql`、runtime timezone rebuild。AgentDash 后续多 agent/多 backend 后会需要成本和活跃度统计。

5. **通知/活动作为一等云端资源**  
   参考 `handler/inbox.go`、`activity.go`、`subscriber.go`。AgentDash 可以把 task/session/workflow 状态变化转成用户可管理的 inbox/activity。

6. **Event bus/listener 与 realtime fanout 分层**  
   参考 `references/multica/server/internal/events/bus.go`、`references/multica/server/cmd/server/listeners.go`、`references/multica/server/internal/realtime/hub.go`。AgentDash 当前 `state_changes` 已能支撑 Project 级 SSE，但后续业务事件层应区分“领域事件产生”“权限过滤/订阅者解析”“实时 fanout/缓存失效”，避免把所有逻辑塞进 stream handler。

7. **Runtime SQL sweeper 的防竞态细节**  
   参考 `runtime.sql` 中 `TouchAgentRuntimeLastSeen`、`SelectStaleOnlineRuntimes`、`MarkRuntimesOfflineByIDs`。multica 的 stale predicate 二次校验、last_seen 热更新控制和 offline task fail 对 AgentDash 的 backend runtime health 设计有直接参考价值。

## 不应直接照搬

- 不把业务逻辑塞回 API handler；AgentDash 应继续用 application/domain 分层。
- 不用 workspace 直接替代 Project；两者租户语义不同。
- 不把 session/backbone 流降级成普通 WS invalidation。
- 不用 sqlc/generated model 取代 domain entity 和 repository trait。

## 后续正式任务候选

1. `feat(events): 建立 Project 级业务事件协议与前端同步策略`
2. `feat(runtime): backend/runtime health 与 last_seen sweeper`
3. `feat(task): 拆分 Task execution attempt 与任务执行日志投影`
4. `feat(actor): 统一 user/agent/system 行为主体模型`
5. `feat(collaboration): 引入 activity/inbox/subscriber 协作反馈模型`
6. `test(api): 为核心列表/统计查询建立 repository 层集成测试`
