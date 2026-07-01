# 收束 AgentRun backend 执行落点选择

## Goal

让 AgentRun 每一轮执行都拥有明确、可审计、可持久化的 backend execution placement 决策。用户本轮显式选择 backend 后，系统必须在授权范围内执行，并把该选择作为后续无额外指令时的默认 backend；所有授权、路由、workspace/root、执行器不可用等异常都必须在前端显著可见。

## Background

- backend selection 已是 session launch planning 的既有抽象：`LaunchPlanningInput.backend_selection` 在 [command.rs](crates/agentdash-application-ports/src/launch/command.rs:39) 定义，项目规范也明确它属于 launch planning input（[session-startup-pipeline.md](.trellis/spec/backend/session/session-startup-pipeline.md:40)）。
- 当前用户入口没有承载 backend selection。`CreateProjectAgentRunRequest` 只包含 input、client command、executor config 和 subject ref（[project_agent.rs](crates/agentdash-contracts/src/agent/project_agent.rs:61)）；`AgentRunComposerSubmitRequest` 只包含 input、client command、command、executor config 和 delivery intent（[run_mailbox.rs](crates/agentdash-contracts/src/agent/run_mailbox.rs:194)）。
- 前端提交 payload 同样只发送 input / executor config / delivery intent（[useAgentRunWorkspaceCommands.ts](packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:238)、[useAgentRunWorkspaceCommands.ts](packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:252)）。
- mailbox 当前只持久化 `payload_json` 和 `executor_config_json`，没有 launch planning facts（[mod.rs](crates/agentdash-domain/src/agent_run_mailbox/mod.rs:390)）；消费时固定传 `LaunchPlanningInput::default()`（[message_delivery.rs](crates/agentdash-application-agentrun/src/agent_run/message_delivery.rs:52)）。
- planner 当前在无显式输入时会从 runtime backend anchor 推导 workspace binding，否则退到 auto idle（[planner.rs](crates/agentdash-application-runtime-session/src/session/launch/planner.rs:316)）；显式 fixed backend 只校验在线 executor，不校验 ProjectBackendAccess（[backend_execution_placement.rs](crates/agentdash-application-runtime-session/src/backend_execution_placement.rs:154)）。
- 前端 composer 已有提交异常展示链路：提交失败会进入 `sendError`（[SessionChatView.tsx](packages/app-web/src/features/session/ui/SessionChatView.tsx:479)），但顶部错误横幅用 `truncate` 渲染（[SessionChatView.tsx](packages/app-web/src/features/session/ui/SessionChatView.tsx:660)），无法完整显示长错误。
- mailbox 行当前以固定高度和 `truncate` 展示 preview / status（[MailboxMessageRow.tsx](packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:278)、[MailboxMessageRow.tsx](packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:301)），不能作为完整诊断内容的唯一承载。

## Requirements

### R1. 每轮执行都支持 backend placement 决策

- AgentRun draft start 和后续 composer submit 都必须能携带本轮 backend selection。
- selection 属于 launch planning facts，不属于 `LaunchCommand` source identity 或 executor config。
- steer active turn 不改变 backend placement；它继续投递到当前 active turn 已绑定的 backend。

### R2. explicit backend selection 是用户本轮显式选择

- `explicit` 表示用户本轮明确选择某个 backend 执行。
- explicit backend 必须在当前 Project 的 active `ProjectBackendAccess` 范围内。
- explicit backend 必须能够承载当前 workspace/root；不能把来自 backend A 的 VFS default mount/root 发送给 backend B。
- explicit selection 被接受后，必须持久化为该 AgentRun 后续无额外指令时的默认 backend preference。

### R3. 默认行为 sticky 到最近一次成功 explicit backend

- 后续用户消息没有携带新的 backend selection 时，应优先使用该 AgentRun 最近一次成功 explicit backend preference。
- 如果没有 sticky explicit preference，才使用 workspace binding 推导；再没有可用 workspace binding 时，才在授权范围内 auto idle。
- sticky preference 失效、backend 离线、grant 被撤销、workspace/root 不可用时，系统必须拒绝或要求用户重新选择，不能静默回退到其它 backend。

### R4. auto idle 和 workspace binding 均受授权范围约束

- auto idle 的候选集必须限定为当前 Project active backend grants 里的在线可用 executor。
- workspace binding backend 也必须同时拥有 active ProjectBackendAccess。
- 所有选择路径都必须创建可审计 backend execution lease，并记录 selection mode、backend id、root/workspace facts 和失败原因。

### R5. mailbox 与 command receipt 保留 planning facts

- AgentRun mailbox message 必须持久化 backend selection/planning facts，使 queued、retry、manual resume、scheduler 消费时保持原始用户意图。
- backend selection 必须进入 command receipt digest，避免同一 `client_command_id` 不同 backend selection 被错误判定为重复命令。
- 消息被消费并 accepted 后，系统再更新 sticky explicit backend preference；未 accepted 的失败选择不得污染默认 preference。

### R6. 前端必须显著感知所有异常

- draft start、composer submit 的同步失败必须通过 composer 的错误区域展示短摘要，并同步弹出外层通知或错误详情面板展示完整错误内容。
- queued mailbox 后续消费失败、backend placement 失败、授权失败、workspace/root mismatch、backend executor unavailable、lease/relay 启动失败必须在 AgentRun workspace 里可见，并同步弹到外层通知面，不能只留在日志、隐藏字段、截断 preview 或截断 inline banner 里。
- inline 区域允许做短摘要，但必须提供不截断的完整错误查看入口；长错误必须保留 backend 返回的关键上下文、错误码和可操作建议。
- 错误文案要指出用户可操作的方向，例如重新选择 backend、授权 backend、绑定 workspace、检查 backend 在线状态。
- 前端应展示当前默认 backend / 本轮选择 / 不可用状态，避免用户不知道下一轮会落到哪里。

## Acceptance Criteria

- [ ] Draft AgentRun start request 和 AgentRun composer submit request 均支持 backend selection，并完成 Rust contract -> generated TypeScript 更新。
- [ ] Mailbox message 持久化 backend selection/planning facts，queued message 消费时仍使用提交时选择。
- [ ] 成功 accepted 的 explicit selection 会更新 AgentRun sticky default；后续无 selection 的用户轮次默认使用该 backend。
- [ ] explicit、workspace binding、auto idle 都只允许当前 Project active backend grants 内的 backend。
- [ ] selected backend 与 VFS default mount/root 不一致时，launch 被拒绝或重新解析到同 backend 的 workspace binding；不得跨 backend 发送 root。
- [ ] backend placement 失败会阻断本轮执行并生成用户可见错误，包含授权、离线、无 executor、workspace/root 不匹配等场景。
- [ ] 前端 composer 和 mailbox/workspace 状态均能显著展示 backend placement 相关错误，并通过外层通知或详情面板完整展示未截断错误内容。
- [ ] 单元测试覆盖 selection 序列化、mailbox 持久化、sticky default、授权过滤、root/backend mismatch、auto idle 授权候选过滤。
- [ ] 前端测试覆盖 backend selection payload、错误展示、sticky default 状态展示。
- [ ] `pnpm run contracts:check`、后端相关测试、前端相关测试通过。

## Out Of Scope

- 不做旧字段兼容或旧数据回退逻辑；项目未上线，migration 直接把当前模型收束到正确状态。
- 不新增跨 Project runner 授权治理能力；本任务只消费现有 `ProjectBackendAccess` active grants。
- 不改变 active turn steer 的 backend；steer 继续绑定当前运行 turn。
