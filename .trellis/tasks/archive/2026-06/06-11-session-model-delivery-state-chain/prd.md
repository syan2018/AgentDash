# AgentRun 工作台与投递链路彻底纠偏

## Goal

把当前交互入口从 Session-first 工作台收敛为 AgentRun Workspace。用户可以继续把交互理解为“会话”，但页面、路由、控制状态、模型选择、投递命令、frame 提交和 HookRuntime 目标都以 AgentRun/AgentFrame 为主体；RuntimeSession 只承担 delivery trace、投递适配和仓储记录职责。

本父任务覆盖以下故障族：

- Project Agent draft 进入正式运行后模型选择器丢失有效 executor/provider/model 状态。
- transport 层出现 `failed to fetch` 后，重试可能触发新的 turn/frame。
- connector 尚未 accepted 时 current frame 已被推进，失败后留下半物化状态。
- HookRuntime 缓存仍指向旧 frame，触发 `Hook runtime target mismatch`。
- 前端存在 `SessionPage` 和 `/session/:id` 交互页面，使 RuntimeSession 被误用为工作台状态主体。
- `SessionMeta` 仍承担工作台标题、侧栏列表、运行状态判断和 runtime-control shell 职责，使 RuntimeSession meta 与 AgentRun Workspace projection 混在一起。

## Confirmed Facts

- `packages/app-web/src/App.tsx` 注册 `/session/new` 和 `/session/:sessionId`，加载 `SessionPage`。
- `packages/app-web/src/pages/SessionPage.tsx` 中正式 runtime 的 `taskExecutorSummary` 为 `null`，聊天区收到的 `agentDefaults` 是 `draftProjectAgent?.executor ?? taskExecutorSummary`。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx` 的 `initialExecutorSource` 只在首次挂载读取 `agentDefaults`。
- `packages/app-web/src/features/executor-selector/model/useExecutorConfig.ts` 使用全局 localStorage key，并且 `hydrate` 只覆盖非空字段。
- `packages/app-web/src/generated/workflow-contracts.ts` 的 `AgentRunMessageRequest` 没有 `client_command_id`。
- `packages/app-web/src/generated/project-agent-contracts.ts` 的 `CreateProjectAgentSessionRequest` 没有 `client_command_id`。
- `crates/agentdash-application/src/workflow/agent_message.rs` 把 `SessionLaunchService::launch_command` 的错误统一包装成 `WorkflowApplicationError::Internal`，会丢失 `ConnectorError::InvalidConfig` 的 400 语义。
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs` 先 materialize AgentRun/RuntimeSession，再投递首条消息；首条消息失败后仅按空 session/run 清理。
- `crates/agentdash-application/src/session/launch/orchestrator.rs` 在 `ConnectorStarter::start` 前写入新的 AgentFrame revision 并更新 current frame。
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs` 命中 cached HookRuntime 后直接返回；`crates/agentdash-application/src/session/hooks_service.rs` 随后用 current target 校验并抛出 mismatch。
- `crates/agentdash-api/src/routes/sessions.rs` 的 runtime-control 已能返回 run、agent、frame_runtime、execution_profile，但前端没有将其投影为 AgentRun Workspace 状态源。
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs` 已能在动态 provider catalog 下对缺失模型选择返回 `ConnectorError::InvalidConfig`，当前问题是错误语义和 frame 提交边界没有在 AgentRun command 层闭环。
- 用户已确认采用彻底迁移方案：移除 `/session` 交互入口，canonical route 使用完整 AgentRun identity。
- `SessionMeta` 当前包含 title/title_source、last_event_seq、last_delivery_status、last_turn_id、last_terminal_message、executor_session_id；其中 event seq 与 executor session follow-up 属于 RuntimeSession trace，工作台标题/列表状态应投影到 AgentRun Workspace。

## Requirements

- 建立 AgentRun Workspace 作为交互工作台主体，canonical route 为 `/agent-runs/:runId/:agentId`，draft route 为 `/agent-runs/new`。
- Route identity 使用完整 `run_id + agent_id`，因为 LifecycleRun 是运行账本和拓扑容器，一个 run 可以包含多个 LifecycleAgent/AgentRun；workspace 需要明确锚定 current frame、execution profile、delivery runtime ref 和 pending command 的 agent 归属。
- 移除前端 `SessionPage` 作为交互页面；如果保留 RuntimeSession trace 可视化，使用只读 RuntimeSession Trace 命名和独立入口。
- 建立 AgentRun workspace API/DTO，以 run_id + agent_id 解析 delivery RuntimeSession、current AgentFrame、execution profile、actions、pending messages 和 trace metadata。
- 收束 `SessionMeta` 语义：RuntimeSession meta 只表达 trace/delivery metadata；工作台 shell、侧栏列表、title/status、last user-visible activity 由 AgentRun Workspace projection 提供。
- Project Agent draft materialization 使用 AgentRun 语义 endpoint，并返回可直接进入 AgentRun Workspace 的 refs/state。
- `start_draft` 和 `send_next` 请求携带 `client_command_id`，服务端记录命令收据、request digest、accepted refs 和 terminal failure。
- 同一 command scope + `client_command_id` 重试时复用既有结果；payload digest 不一致时返回明确 conflict。
- connector `InvalidConfig` 保持 BadRequest 语义，并且不推进 turn/current frame。
- Launch accepted boundary 以 `connector.prompt` 返回 `ExecutionStream` 为准；current frame 写入、用户消息事件、turn started、command receipt accepted 在 accepted commit 中完成。
- HookRuntime cache 以当前 HookControlTarget 校验后复用；target 变化时重建并替换。
- 模型选择器从 AgentRun/AgentFrame execution profile 或 draft ProjectAgent executor 水合；localStorage 只作为个人最近选择，不覆盖 workspace 权威状态。

## Child Tasks

- `06-11-agentrun-runtime-trace-meta-convergence`: 拆分 RuntimeSession trace meta 与 AgentRun Workspace shell/list/status projection。
- `06-11-agentrun-workspace-api-contract`: 建立 AgentRun workspace API/DTO 和 command route 语义。
- `06-11-agentrun-delivery-command-receipts`: 建立 `client_command_id` 幂等收据和失败恢复。
- `06-11-launch-frame-hook-atomicity`: 修复 launch accepted boundary、current frame 提交和 HookRuntime target refresh。
- `06-11-agentrun-workspace-frontend-route-state`: 迁移前端路由/页面主体和模型状态水合。

## Acceptance Criteria

- [ ] 前端 App route 中没有 `/session/new` 或 `/session/:sessionId` 交互路由。
- [ ] 交互工作台入口使用 `/agent-runs/:runId/:agentId`，Project Agent draft 使用 `/agent-runs/new`。
- [ ] 代码中 `SessionPage` 不再作为交互工作台组件存在；触达文件中的工作台状态命名使用 AgentRun Workspace 语义。
- [ ] `SessionMeta` 不再作为交互工作台标题、侧栏列表或 command 状态的 public fact；RuntimeSession trace 页面只读展示 trace metadata。
- [ ] Project Agent draft 首条消息 accepted 后进入对应 AgentRun Workspace，模型选择器保持可用并显示有效 provider/model/thinking。
- [ ] `send_next` 在 transport failure 后使用同一 `client_command_id` 恢复或重试，不创建额外 turn/frame。
- [ ] 缺失必需模型选择时返回明确 400/BadRequest，且 current frame、turn、command receipt accepted 状态不推进。
- [ ] connector start/preparation failure 后 current frame 保持失败前值。
- [ ] HookRuntime stale frame cache 在下一次 dispatch 前被刷新，不出现正常 frame 切换后的 `Hook runtime target mismatch`。
- [ ] 所有 child task 的 artifacts 写明依赖、交付物和独立验收，且 `task.py validate` 通过。

## Out Of Scope

- 重写 LifecycleRun、LifecycleAgent、AgentFrame 的领域模型。
- 改变 RuntimeSession 作为 delivery trace 和事件仓储记录的底层职责。
- 为旧 `/session` 交互路由提供长期兼容入口。
