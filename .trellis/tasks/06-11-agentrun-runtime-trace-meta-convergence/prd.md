# AgentRun trace meta 与工作台 shell 收束

## Goal

将 `SessionMeta` 从交互工作台事实源中拆出。RuntimeSession 仍保留 trace/delivery 所需 metadata；AgentRun Workspace 负责用户可见工作台 shell，包括标题、列表项、投递状态、当前 command 能力和最后活动时间。

这样做可以避免 route 已迁移到 AgentRun 后，侧栏、runtime-control、标题编辑、状态判断仍从 RuntimeSession meta 反推业务状态。

## Confirmed Facts

- `SessionMeta` 当前包含 title/title_source、last_event_seq、last_delivery_status、last_turn_id、last_terminal_message、executor_session_id。
- `crates/agentdash-api/src/routes/sessions.rs` 的 `get_session_runtime_control` 返回 `session_meta: SessionShellDto`，并用 `meta.last_delivery_status` 参与 running 判定。
- `get_project_sessions` 从 `session_core.list_sessions()` 出发，经过 RuntimeSessionExecutionAnchor 过滤 Project 后生成 `ProjectSessionListEntry`。
- `ProjectSessionListEntry` 当前以 `runtime_session_id`、`title`、`delivery_status` 为核心，前端 `SessionShortcutList` 点击后导航到 `/session/:id`。
- `SessionPage` 用 `runtimeControl.session_meta.title` 作为运行态页面标题，并监听 `session_meta_updated` 事件刷新标题。
- `RuntimeTraceLaunchState` 只需要 `SessionMeta.executor_session_id` 与 `last_event_seq`，用于 executor follow-up 和 repository rehydrate。
- `LifecycleAgent.bootstrap_status` 已取代原 `SessionMeta.bootstrap_state`。

## Requirements

- 定义 RuntimeSession trace metadata 与 AgentRun Workspace shell 的边界。
- `SessionMeta` 保留 trace/delivery ledger 字段：runtime session id、event seq、executor_session_id、trace title provenance、terminal trace summary。
- AgentRun Workspace 提供 public shell：display title、title source、delivery status、last turn id、last activity/update time、delivery runtime ref。
- Project sidebar/list 入口从 AgentRun Workspace 或 AgentRun list projection 出发，不再以 RuntimeSession list/meta 作为入口事实源。
- Runtime-control 的 command enablement 以 AgentRun/AgentFrame/command receipt/active turn 投影为准；`SessionMeta.last_delivery_status` 只作为 trace fallback，不作为 workspace authoritative status。
- 标题编辑如保留，目标应是 AgentRun Workspace title，而不是 RuntimeSession trace title。
- RuntimeSession trace 页面可以只读展示 trace metadata 和事件流，不参与 workspace identity。

## Acceptance Criteria

- [ ] 父任务/API contract child 明确 `RuntimeSessionTraceMeta` 与 `AgentRunWorkspaceShell` DTO 边界。
- [ ] `ProjectSessionListEntry` 的替代目标命名和数据源已确定为 AgentRun-oriented list/shortcut projection。
- [ ] `SessionMeta` 不再作为 AgentRun Workspace title/status/list 的 authoritative public fact。
- [ ] launch/repository rehydrate 对 `executor_session_id` 与 `last_event_seq` 的 trace 需求被保留。
- [ ] 相关 spec 更新点列入父任务最终验收。

## Out Of Scope

- 删除 RuntimeSession event/projection/lineage 存储。
- 重写 connector executor_session follow-up 机制。
