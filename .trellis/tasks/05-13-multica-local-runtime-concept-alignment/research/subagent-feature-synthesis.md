# Subagent Feature Synthesis

本文件汇总四个只读 subagent 的对比结论：`research_cloud_data_events`、`research_local_daemon`、`research_frontend_desktop`、`research_product_automation`。主会话只吸收结论，不原样堆叠 subagent 输出。

## 总体判断

AgentDash 的强项在执行平台内核：Rust 分层架构、VFS、SessionHub、Backbone/session event、Pi Agent Loop、Hook Runtime、Workflow/Lifecycle DAG、MCP/Plugin 扩展。multica 的强项在产品闭环与运行时运营：Workspace/Issue 协作模型、Agent 作为 actor、runtime/daemon health、task queue/message、Inbox/Activity/Subscriber、desktop 管 daemon、React Query server-state 纪律。

因此当前项目最值得学习的不是替换底层执行架构，而是在 AgentDash 现有内核之上补齐“可运营、可恢复、可协作、可诊断”的产品层与投影层。

## P1 值得优先吸收的 Feature

### 1. Runtime Health 与 offline recovery

- multica 参考：`references/multica/server/pkg/db/queries/runtime.sql`、`references/multica/server/internal/daemon/daemon.go`、`references/multica/server/internal/handler/runtime.go`。
- AgentDash 对应：`crates/agentdash-api/src/relay/registry.rs`、`crates/agentdash-api/src/relay/ws_handler.rs`、`crates/agentdash-local/src/ws_client.rs`。
- 学习点：把 online backend 从内存连接表升级为持久 runtime 资源，记录 `status`、`last_seen_at`、owner、version、capabilities、health、offline reason。
- 预期收益：API 重启、网络分区、本机进程 crash 后仍能收敛；前端能展示可诊断 runtime 状态；desktop 能与 cloud runtime 形成双通道状态。
- 改造风险：不能把 relay registry 变成 DB 热路径；应保持“内存连接表 + 持久 runtime 投影”分层。

### 2. ExecutionAttempt 与 MessageLog 投影

- multica 参考：`references/multica/server/internal/service/task.go`、`references/multica/server/pkg/db/queries/task_message.sql`、`references/multica/server/migrations/026_task_messages.up.sql`。
- AgentDash 对应：`crates/agentdash-domain/src/task`、`crates/agentdash-domain/src/session_binding`、`crates/agentdash-application/src/session`。
- 学习点：保留 AgentDash Task 作为业务工作项，但新增执行尝试/执行日志投影；一个 Task 可有多次 start、continue、rerun、retry。
- 预期收益：Task 页能展示每次执行状态、失败原因、session、artifact、usage 和工具摘要，用户不必进入原始 session stream 翻找。
- 改造风险：Backbone/session event 仍是执行事实源，ExecutionAttempt 只应做查询/展示投影，不重复持久化完整事件。

### 3. Project 级业务事件协议

- multica 参考：`references/multica/server/pkg/protocol/events.go`、`references/multica/server/internal/events/bus.go`、`references/multica/server/cmd/server/listeners.go`。
- AgentDash 对应：`crates/agentdash-api/src/stream.rs`、`crates/agentdash-infrastructure/src/persistence/postgres/state_change_store.rs`、`frontend/src/api/eventStream.ts`、`frontend/src/stores/eventStore.ts`。
- 学习点：在 session/backbone 执行流之上建立用户感知的业务事件层，例如 `story:*`、`task:*`、`runtime:*`、`activity:*`、`inbox:*`。
- 预期收益：前端列表、通知、activity、runtime panel 可基于稳定业务事件同步，不需要解析执行内部事件。
- 改造风险：业务事件层不能替代 session stream；session/backbone 仍是执行事实源。

### 4. Actor / Activity / Inbox / Subscriber 协作闭环

- multica 参考：`references/multica/server/internal/handler/activity.go`、`comment.go`、`inbox.go`、`subscriber.go`，以及 `references/multica/server/pkg/db/queries/activity.sql`、`inbox.sql`、`subscriber.sql`。
- AgentDash 对应：Story/Task state changes、SessionBinding、Workflow/Lifecycle terminal events、Project grants、Agent/ProjectAgentLink。
- 学习点：统一 member/agent/system actor；把状态变化、评论、任务完成/失败、需要人工处理的 gate 投影为 timeline 与 inbox。
- 预期收益：用户不必盯 session；Agent 成为可见协作者；后续审计、通知、协作权限有统一基础。
- 改造风险：通知噪音高，需要 subscriber/watch 与 notification preference；Activity 不能复制完整 session event。

### 5. Desktop 作为 local backend 控制台

- multica 参考：`references/multica/apps/desktop/src/main/daemon-manager.ts`、`cli-bootstrap.ts`、`version-decision.ts`、`references/multica/apps/desktop/src/preload/index.ts`、`references/multica/apps/desktop/src/renderer/src/components/daemon-panel.tsx`。
- AgentDash 对应：未来 desktop/Tauri command 层、`crates/agentdash-local`、`frontend/src/components/layout/workspace-layout.tsx`、`frontend/src/stores/coordinatorStore.ts`。
- 学习点：desktop 不只是 web wrapper，而是本机能力控制台，负责 local backend 启停、health polling、日志 tail、profile 隔离、token sync、version mismatch 安全重启。
- 预期收益：本机连接失败不再是黑盒；用户可自助诊断 executor、MCP、accessible roots、版本问题。
- 改造风险：AgentDash local backend 承担 terminal/MCP/VFS/SessionHub，安全重启判定比 multica active task count 更复杂。

### 6. Frontend server-state query layer 与 realtime invalidation

- multica 参考：`references/multica/packages/core/query-client.ts`、`references/multica/packages/core/realtime/use-realtime-sync.ts`、`references/multica/packages/core/runtimes/queries.ts`。
- AgentDash 对应：`frontend/src/stores/storyStore.ts`、`projectStore.ts`、`coordinatorStore.ts`、`activeSessionsStore.ts`、`frontend/src/api/eventStream.ts`。
- 学习点：远端实体归 query/cache；Zustand 主要保留 tab、draft、filter、局部 UI 状态；业务事件只做 invalidation 或局部 patch。
- 预期收益：减少手工 patch 与 store 漂移；为 web/desktop 共享状态打基础。
- 改造风险：不要一次性大迁移；先从 backend/runtime list、project sessions、story/task list 试点，session feed 保留专用 reducer。

## P2 值得吸收的 Feature

### 7. Routine admission skip 与 failure governance

- multica 参考：`references/multica/server/internal/service/autopilot.go`、`references/multica/server/pkg/db/queries/autopilot.sql`、`references/multica/server/migrations/079_autopilot_run_skipped_status.up.sql`。
- AgentDash 对应：`crates/agentdash-domain/src/routine/entity.rs`、`crates/agentdash-application/src/routine/executor.rs`。
- 学习点：Routine 触发前检查 runtime/backend/workspace/agent 是否可用，不可用时记录 skipped；基于真实 terminal run 做失败率自动暂停。
- 预期收益：自动化不会在 local backend 离线时制造无效 session 或无限失败。
- 改造风险：AgentDash 当前 Routine completed 更接近 prompt 派发成功，不等于 Agent 完整执行终态，需要先对齐 terminal 语义。

### 8. Provider-native instruction / skill materialization

- multica 参考：`references/multica/server/internal/daemon/execenv/execenv.go`、`references/multica/server/internal/handler/runtime_local_skills.go`。
- AgentDash 对应：VFS materialization、Skill Asset、CODEX_HOME、executor adapters。
- 学习点：把规则、skills、provider native config 写到 `AGENTS.md`、`CLAUDE.md`、`GEMINI.md`、`CODEX_HOME` 等 provider 原生发现路径，并保留 manifest。
- 预期收益：减少长 prompt 压力，让 provider 按自身机制工作；本机 skills 可盘点并导入为 Project Skill Asset。
- 改造风险：必须保持 VFS 权限、审计与 Context Inspector 可解释性，不能让物理文件成为隐式副作用。

### 9. Materialized workdir lifecycle 与 GC meta

- multica 参考：`references/multica/server/internal/daemon/gc.go`、`references/multica/server/internal/daemon/execenv/execenv.go`、`references/multica/server/internal/daemon/repocache/cache.go`。
- AgentDash 对应：`crates/agentdash-local/src/materialization.rs`、`crates/agentdash-application/src/vfs/materialization.rs`。
- 学习点：为物化目录记录 owner/session/task/tool_call/ttl/last_access，使用 active root 防止执行中被清理。
- 预期收益：VFS materialization 从缓存能力升级为可治理 workdir 生命周期。
- 改造风险：AgentDash 物化服务 MCP/tool/shell 多种场景，owner ref 不能只绑定 Task。

### 10. Provider adapter 稳定性矩阵

- multica 参考：`references/multica/server/pkg/agent`、`stderr_tail.go`、各 provider adapter test、daemon `executeAndDrain`。
- AgentDash 对应：`crates/agentdash-executor/src/connectors/codex_bridge.rs`、`executor_session.rs`、Pi Agent connector。
- 学习点：统一覆盖 session id、usage、stderr tail、drain timeout、cancel、timeout、poisoned output、resume fallback。
- 预期收益：执行失败可解释，恢复逻辑更稳。
- 改造风险：不要替换 Backbone envelope；学习 adapter checklist 和测试矩阵即可。

## 不应直接照搬

- 不把 AgentDash Task 改成 multica `agent_task_queue`；应新增 execution attempt 投影。
- 不把 Workflow/Lifecycle 简化成 Autopilot；Routine 学治理，Lifecycle 保持 DAG/Hook 能力。
- 不把 local backend 改成 daemon poll claim 唯一路径；AgentDash relay push、MCP、terminal、VFS、SessionHub 是核心能力。
- 不用 physical worktree 替代 VFS；只学习 workdir 生命周期、GC、provider-native 文件布局。
- 不用 sqlc/generated row 替代 Rust domain/repository/application 分层；只学习 SQL 查询覆盖、索引、rollup 和测试密度。
- 不复制 multica 为旧 web/desktop contract 保留的兼容 fallback；AgentDash 当前预研未上线，应设计最正确的新 contract。

## 后续正式任务建议

### P1

1. `feat(runtime): 建立 backend runtime health 与 offline recovery`
2. `feat(task): 任务执行尝试与执行日志投影`
3. `feat(events): 建立 Project 级业务事件协议与前端同步规范`
4. `feat(actor): 统一用户、Agent、System 行为主体模型`
5. `feat(collaboration): 建立 Story/Task 活动时间线`
6. `refactor(frontend): 引入 server-state query key 与实时 invalidation 规范`

### P2

1. `feat(desktop): local backend health/log/restart 控制台`
2. `feat(routine): 对齐执行终态并加入触发准入跳过`
3. `feat(vfs): materialized workdir lifecycle 与 GC meta`
4. `feat(vfs): provider-native instruction/skill materialization`
5. `feat(notification): 建立 Story/Task 收件箱通知闭环`
6. `test(executor): provider adapter 行为矩阵`

### P3

1. `feat(skill): 支持本机 Skill 盘点与导入`
2. `feat(analytics): runtime 与 agent execution usage rollup`
3. `perf(session): streaming markdown block memo 与高频事件渲染优化`
4. `feat(desktop): workspace/project scoped app tabs`

## 需要回填到现有文档的重点

- `cloud-capability-map.md`：补充 event bus/listener、subscriber、runtime SQL sweeper 的细节。
- `local-daemon-comparison.md`：补充 runtime_gone、slot-before-claim、mid-flight session pinning、poisoned session、activeEnvRoots。
- `desktop-local-integration.md`：补充 profile 隔离、token sync、version mismatch defer restart、daemon IPC bridge、日志 tail。
- `learning-backlog.md`：把业务事件协议、Actor/Activity、ExecutionAttempt、Runtime Health 升为前置 P1。
- `concept-map.md`：强调 Routine completed 与真实 agent terminal 尚未对齐；Task execution attempt 应为投影而非事实源。
