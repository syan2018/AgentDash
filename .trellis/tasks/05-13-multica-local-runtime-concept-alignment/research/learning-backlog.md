# 可学习机制 Backlog

## P1 前置候选

### 0. Project 级业务事件协议

- 参考：`references/multica/server/pkg/protocol/events.go`、`server/internal/events/bus.go`、`server/cmd/server/listeners.go`
- AgentDash 对应：`crates/agentdash-api/src/stream.rs`、`state_changes`、`frontend/src/api/eventStream.ts`、`frontend/src/stores/eventStore.ts`
- 价值：把 Story/Task/Runtime/Activity/Inbox 等用户感知状态变化从 session/backbone 执行流中解耦，成为前端同步、通知、activity 的共同契约。
- 风险：不能替代 session stream；需要事件命名、权限过滤、payload 稳定性和 query invalidation 规范。
- 建议任务：`feat(events): 建立 Project 级业务事件协议与前端同步规范`

### 0.1 Actor / Activity Timeline 基础

- 参考：`references/multica/server/internal/handler/activity.go`、`comment.go`、`subscriber.go`、`inbox.go`
- AgentDash 对应：Story/Task state changes、SessionBinding、Workflow/Lifecycle terminal events、ProjectAgentLink
- 价值：统一 user/agent/system actor，让 Story/Task 拥有用户可读时间线，后续 Inbox/Notification 不再依赖用户盯 session stream。
- 风险：通知噪音与重复存储风险高；Activity 应是业务投影，不复制完整 session events。
- 建议任务：`feat(actor): 统一用户、Agent、System 行为主体模型`、`feat(collaboration): 建立 Story/Task 活动时间线`

## P1 候选

### 1. Runtime Health 与恢复状态机

- 参考：`references/multica/server/internal/daemon/daemon.go`、`server/pkg/db/queries/runtime.sql`、`server/internal/handler/runtime*.go`
- AgentDash 对应：`crates/agentdash-local/src/ws_client.rs`、`crates/agentdash-api/src/relay/registry.rs`
- 价值：将 online backend 从内存连接表升级为可观测、可恢复、可运营资源。
- 风险：需要梳理 backend/runtime/session/terminal 状态边界。
- 建议任务：`feat(runtime): 建立 backend runtime health 与 offline recovery`

### 2. Task Execution Attempt 与 Message Log

- 参考：`agent_task_queue`、`task_message.sql`、`server/internal/service/task.go`、daemon `executeAndDrain`
- AgentDash 对应：Task、SessionBinding、SessionHub event persistence
- 价值：让用户从 Task 视角查看每次执行、工具调用、失败原因、重试历史。
- 风险：不要重复存储 Backbone 全量事件；应设计投影或轻量索引。
- 建议任务：`feat(task): 任务执行尝试与执行日志投影`

### 3. Desktop Local Backend 管理

- 参考：`apps/desktop/src/main/daemon-manager.ts`、`cli-bootstrap.ts`、renderer daemon panel
- AgentDash 对应：桌面统一架构规划、`agentdash-local`
- 价值：让 desktop 成为 local backend 控制台，提升本机连接体验。
- 风险：开发期 `pnpm dev` 与用户桌面运行策略不同。
- 建议任务：`feat(desktop): local backend health/log/restart 控制台`

### 4. 前端 Server State 规范

- 参考：`packages/core/realtime/use-realtime-sync.ts`、各 domain `queries.ts`
- AgentDash 对应：`frontend/src/stores`、`services`、`api/eventStream.ts`
- 价值：减少 Zustand 存 server state 导致的漂移，方便 web/desktop 复用。
- 风险：一次性迁移成本高，应从 runtime/task/session list 等高变动数据开始。
- 建议任务：`refactor(frontend): 引入 query key 与实时 invalidation 规范`

## P2 候选

### 5. Inbox / Activity / Subscriber 协作闭环

- 参考：`handler/inbox.go`、`activity.go`、`subscriber.go`
- AgentDash 对应：Story/Task/Session/Workflow 状态变化
- 价值：让用户不必盯流式 session，AI 工作状态沉淀为通知和活动。
- 风险：需要统一 actor 模型，避免 UI 噪音。
- 建议任务：`feat(collaboration): activity 与 inbox 统一反馈模型`

### 6. Autopilot Failure Governance 对齐 Routine

- 参考：`autopilot.sql`、`autopilot_scheduler.go`、failure monitor
- AgentDash 对应：Routine、Workflow/Lifecycle trigger
- 价值：自动触发需要 run history、skip reason、失败率暂停。
- 风险：Routine 触发源更多，不能照搬 schedule-only 视角。
- 建议任务：`feat(routine): run history 与失败治理`

补充：AgentDash `RoutineExecution` 当前 completed 更接近“prompt 已派发”，不等于 Agent 已完整执行完成。正式治理前应先让 Routine run 与 session terminal 对齐，并加入 backend/agent/workspace 不可用时的 `skipped` 状态。

### 7. Provider 原生文件注入与 Skill Materialization

- 参考：`server/internal/daemon/execenv/*`、local skills、Codex home link
- AgentDash 对应：VFS materialization、Skill Asset、CODEX_HOME
- 价值：减少长 prompt 压力，让 provider 按原生机制读取规则和 skills。
- 风险：必须和 VFS/mount 权限保持一致，不能让物理文件绕过审计。
- 建议任务：`feat(vfs): provider-native instruction/skill materialization`

补充：Local Skill inventory/import 应作为独立知识资产闭环评估：本机 provider skills -> runtime inventory -> import -> Project Skill Asset -> Agent binding -> Session injection。

### 8. Usage / Runtime Activity Rollup

- 参考：`runtime_usage.sql`、`task_usage.sql`、daily rollup migration
- AgentDash 对应：Session/Task execution statistics
- 价值：支持 dashboard、成本、agent 活跃度、runtime 质量评价。
- 风险：早期 schema 未稳定，需选择最小可用聚合。
- 建议任务：`feat(analytics): runtime 与 agent execution usage rollup`

## 暂不建议转任务

- 直接迁移 multica Issue 模型：会冲突 AgentDash Story/Task/Session 语义。
- 用 sqlc 风格重写数据层：不符合 Rust 分层架构。
- 把 daemon poll claim 作为唯一执行路径：AgentDash relay/tool/session 主动下发仍是核心能力。
- 全量前端 monorepo package 重构：应由 desktop 需求逐步牵引。
