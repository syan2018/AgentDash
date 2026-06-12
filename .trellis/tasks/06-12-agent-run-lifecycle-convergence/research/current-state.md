# 当前状态证据

## 历史任务

- `.trellis/tasks/06-11-agentrun-workspace-api-contract/` 已定义 AgentRun workspace API：`GET /agent-runs/{run_id}/agents/{agent_id}/workspace`、messages、steering、pending、cancel，以及 ProjectAgent `/agent-runs` materialization endpoint。
- `.trellis/tasks/06-11-agentrun-workspace-frontend-route-state/` 已把前端交互入口迁到 `/agent-runs/new` 与 `/agent-runs/:runId/:agentId`，并让 draft start accepted 后跳转正式 AgentRun route。
- `.trellis/tasks/06-11-agentrun-delivery-command-receipts/` 已定义 command receipt 幂等和 duplicate/retry 语义。
- `.trellis/tasks/06-11-agentrun-runtime-trace-meta-convergence/` 已将 RuntimeSession trace meta 与 AgentRun workspace shell 分边界。

## 后端启动链路

- `crates/agentdash-application/src/workflow/project_agent_run_start.rs`
  - `ProjectAgentRunStartService::start_run` 校验 input/client command，claim `project_agent_start` receipt。
  - 通过 `LifecycleDispatchService::launch_agent` materialize `LifecycleRun` / `LifecycleAgent` / `AgentFrame` / `RuntimeSession`。
  - 之后调用 `bind_project_agent_to_lifecycle_agent` 写入 `LifecycleAgent.project_agent_id`。
  - 再通过 `AgentRunMessageService::dispatch_user_message` 投递首条消息。
  - 首条消息失败且 runtime session 无事件时，会删除 anchor、runtime session 与 run。

- `crates/agentdash-application/src/workflow/dispatch_service.rs`
  - `dispatch_common` 处理 graph-backed dispatch：resolve workflow graph，创建/复用 run，确保 orchestration，创建 agent/runtime/frame，写 `RuntimeSessionExecutionAnchor::new_orchestration_dispatch`，并发送 `NodeStarted` runtime event。
  - `dispatch_graphless` 处理 graphless dispatch：创建 graphless run / agent / frame / runtime session，写普通 `RuntimeSessionExecutionAnchor::new_dispatch`。
  - `create_initial_frame` 和 `create_graphless_initial_frame` 均使用 `AgentFrameBuilder::new_launch_anchor`，表达 launch evidence revision。

- `crates/agentdash-application/src/session/launch/orchestrator.rs`
  - `SessionLaunchOrchestrator::launch` 从 `SessionMeta` 与 runtime command store 读取 trace facts。
  - 调用 construction provider 生成 `FrameLaunchEnvelope`。
  - envelope 后续进入 connector start、stream ingestion、turn commit 与 pending frame commit。

- `crates/agentdash-application/src/workflow/frame_construction/mod.rs`
  - `construct_launch_envelope` 通过 `RuntimeSessionExecutionAnchor` 反查 agent/run/current frame。
  - frame surface ready 且 prompt lifecycle 为 plain 时直接复用 frame surface。
  - 其他情况进入 `classify::route_and_compose`。

- `crates/agentdash-application/src/workflow/frame_construction/classify.rs`
  - 当前分类顺序为 companion hint、ProjectAgent、lifecycle node、existing frame surface。
  - ProjectAgent identity 已优先于 orchestration anchor，防止 ProjectAgent explicit lifecycle 被 lifecycle node composer 抢占。

- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs`
  - ProjectAgent composer 读取 ProjectAgent preset、project workspace、subject context，并合并 executor config。
  - 当前会解析 active workflow projection，并传入 owner bootstrap composer 以保留 lifecycle mount。

## 前端交互链路

- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx`
  - draft 页面由 project id + project agent key 进入。
  - `start_draft` 调用 `createProjectAgentRun`，传 `client_command_id`、input、executor_config。
  - accepted 后跳转 `/agent-runs/{runId}/{agentId}`，后续 command 使用 run/agent public identity。
  - executor selector state key 包含 draft key 或 AgentRun frame id。

- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts`
  - 通过 `fetchAgentRunWorkspace(runId, agentId)` 获取 workspace。
  - 从 `delivery_runtime_ref` 取 runtime session id，再解析 `session_runtime` VFS surface。
  - state 中保存 workspace、runtime session id、runtime surface、frame runtime 和 error。

- `packages/app-web/src/pages/AgentRunWorkspacePage.chatControlState.ts`
  - draft 状态暴露 `start_draft`。
  - workspace ready 且 actions.send_next enabled 时暴露 `send_next`。
  - running 且 enqueue enabled 时暴露 `enqueue`，steer 可作为 secondary action。
  - cancel 可用性来自 workspace actions。

## 关键架构缺口

- 后端启动入口共享 run / agent / frame / runtime session / anchor，但缺少一个显式 launch plan contract 统一表达 owner、composer、active workflow、cleanup 和前端 readiness。
- `RuntimeSessionExecutionAnchor` 是稳定反查索引，但当前 frame construction 仍使用它参与 composer 决策；这会让 owner identity 与 workflow node binding 混在同一判断层。
- ProjectAgent start 的 application tests 使用 fake delivery，覆盖 receipt/cleanup，但不覆盖真实 `SessionLaunchService -> FrameConstructionService` 路径。
- 前端 workspace 状态已经 AgentRun 化，但仍要额外解析 `session_runtime` surface 并把 projection status、control plane、actions 拼成聊天可用状态；后端应提供更强的 launch/control readiness 语义。
