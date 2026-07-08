# 实施计划

## 0. Research 收敛

- 保留三份 reference 调研文档和 `research/reference-synthesis.md`，作为本任务设计依据。
- 实现前按 synthesis 校准 MVP：手动入口、running/idle 分流、durable request、compact-only turn 属于本任务；overflow retry、model downshift、split-turn summary 属于后续优化。

## 1. Contracts 与命令可用性

- 在 contract enum 中增加 `ConversationCommandKind::CompactContext`，生成 TypeScript 类型。
- 在 AgentRun conversation snapshot 中增加 `CompactContext` command，command id 为 `compact_context`。
- 在 workspace command policy 中增加 stale guard 校验。
- 在 command receipt kind 中增加 `context_compact`，补数据库 check constraint migration。
- 在 AgentRun application 内部定义 runtime command fulfillment decision：`ScheduleForNextTurn`、`LaunchMaintenanceTurn`、`Reject`；该 decision 不进入 API request DTO，也不由 frontend/route handler 维护。

## 2. Manual compaction request 存储

- 新增 domain model/repository port：manual context compaction request。
- 新增 Postgres migration 与 repository 实现。
- 增加 claim/consume/complete/noop/fail 方法，保证每个 session 同时只有一个 requested manual request。

## 3. AgentRun command service 与 API

- 新增 `AgentRunContextCompactionCommandService`。
- 复用 delivery runtime selection 和 command receipt claim。
- 新增或复用 AgentRun internal runtime command fulfillment service，由 AgentRun 内部读取 execution state 并完成分流。
- `ScheduleForNextTurn` decision 创建 `next_turn` request，返回 `scheduled_next_turn`。
- `LaunchMaintenanceTurn` decision 创建 `compact_only` request，并调用 runtime port 启动 compact-only turn。
- `Reject` decision 返回 blocked/failed command result，不让 API/frontend/route handler 或 compaction 末梢自行处理非法状态。
- 新增 API route `POST /agent-runs/{run_id}/agents/{agent_id}/runtime/context/compact`。
- 新增 response DTO 与 contracts 映射。

## 4. Runtime session compact-only

- 扩展 launch source/modifier，表达 `ContextCompaction` / `CompactOnly`。
- launch preparation/commit 对 compact-only 不写 `UserInputSubmitted`，只写可观测控制事件。
- connector start path 将 compact-only mode 传给 agent loop。
- turn terminal 正常 completed，保证 control plane 状态回到 idle。

## 5. Agent compaction preflight

- 从 `stream_assistant_response` 抽出 shared compaction preflight。
- 普通 provider 请求复用 preflight 后继续 streaming。
- 新增 compact-only entrypoint，只执行 preflight 并结束。
- 为 `CompactionParams` / `CompactionResult` / `AgentEvent::ContextCompacted` 补 provenance metadata：`trigger`、`reason`、`phase`、`strategy`、`implementation`、`request_id`。
- mapper 透传 provenance 字段到 `context_compacted` payload。
- 调整 summary prompt：summary 生成只总结、不继续对话；summary 内容覆盖目标、进展、决策、约束、文件/工具状态、错误修复、待办和下一步。
- 为 summarizer 输入加 bounded fact 策略，避免大 tool result / attachment 原文无限展开。

## 6. Manual request 消费与结果回写

- TurnPreparer 或 HookRuntime 注入当前 turn 的 pending manual request。
- `evaluate_compaction` 优先使用 manual request，跳过 hook preset threshold。
- `after_compaction` / no-op / failure 将 request 标记为 completed/noop/failed。
- request result 中记录 compaction id、turn id、边界引用或错误。
- compact-only turn 标记为不可 steer；用户新输入走 mailbox/下一正常 turn，不进入 compact-only turn。

## 7. Frontend

- generated contracts 更新后新增 `compactAgentRunContext` service。
- 在 `SessionProjectionView` / `SessionProjectionViewPanel` 增加 compact command props。
- `ContextUsageRing` 将 AgentRun command state 传入 projection panel。
- 点击按钮调用 API，按 outcome 显示轻量状态；projection 仍由事件刷新。
- 压缩完成后 context usage 展示 projection estimate，并和 provider-verified usage 区分。

## 8. 验证

- Rust targeted tests：command availability、command service、request store、manual preflight、compact-only loop、eventing metadata、summary prompt、resume/fork checkpoint、token usage stale prevention。
- Frontend targeted tests：service path、panel action、disabled state。
- 合同生成检查。
- 手动跑一次 dev 环境：空闲立即压缩与运行中排队压缩各验证一次。
