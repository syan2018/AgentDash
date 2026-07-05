# AgentRun 会话执行事实源收束与分支回放修复

## Goal

修复 AgentRun 会话分支后历史内容概率性丢失、会话完成后仍显示运行中、以及 provider waiting 阶段“正在思考”提示不出现的问题，并把 AgentRun / RuntimeSession / conversation feed / 前端 command state 的运行态事实源收束到同一条链路。

用户可见目标是：AgentRun 完成态、可分支态、可发送态、stream replay、分支历史展示和思考提示必须一致；任何一个界面入口都不应因陈旧 RuntimeSession 元数据、旧 conversation snapshot 或错误的 fork seed 得出不同结论。对外状态必须完整收束到 AgentRun 这一层，RuntimeSession 只作为 AgentRun 内部 execution / event / recovery 输入存在。本任务也是本轮 Agent 相关重构的收口审计：AgentRun 前端和公开 API 不应继续消费 RuntimeSession 派生状态作为用户可见事实源。

## Background

- 用户报告：父会话实际已执行完成，但会话底部仍提示“会话正在进行中”；此时 fork 可以触发，但 fork 后新会话有概率缺失前序会话内容。
- 用户报告：原先运行时会出现的“正在思考”提示当前不会触发。
- 当前证据显示，AgentRun workspace conversation / command 已主要从 `SessionExecutionState` 推导，而 runtime control read model 仍会把 `last_delivery_status == running` 合并成 active running，形成 read surface 分歧。
- 当前证据显示，AgentRun workspace 前端仍存在把 `delivery_trace_meta` 包装成 `SessionShellDto.last_delivery_status` 的路径，AgentRun scoped runtime control 也仍返回 `SessionRuntimeControlView`，说明 RuntimeSession 派生状态尚未从 AgentRun UI contract 中隔离。
- fork child 首屏 inherited 内容当前来自父 RuntimeSession 的 model-context projection；这不等同于稳定的 UI 可见 transcript slice，遇到 compaction / projection head / fork point 差异时可能表现为概率性缺失。
- 本项目仍处预研阶段，不需要保留错误兼容路径；数据库字段和 API 形态可以按正确模型重构，但迁移需要处理干净。

## Requirements

- R1: AgentRun 是用户可见会话状态的一等事实源；所有对外状态面，包括 conversation snapshot、workspace state、command availability、runtime control、fork readiness、composer helper，都必须从 AgentRun 层导出的 execution snapshot / display snapshot 读取。
- R2: RuntimeSession 只能作为 AgentRun 内部的 execution inspection、event replay、ephemeral stream 和 recovery metadata 输入；不能继续作为对外 UI / API 状态事实源，也不能让 RuntimeSession summary 字段直接决定 AgentRun 是否 running / cancellable / submittable。
- R3: AgentRun 对外运行态必须只由当前 runtime execution inspection 收束后的 AgentRun execution snapshot 导出，不能让陈旧的 RuntimeSession summary 字段参与 active/running/cancelling 判定。
- R4: 会话分支必须绑定到明确 lineage 切片；fork 后 UI 继承内容必须来自稳定的父会话可见 transcript 切片，而不是偶然等同于模型上下文的投影。
- R5: fork 后子会话首屏必须稳定展示父分支点前的可见上下文，并继续接收子 runtime 的 user / assistant / tool / provider events。
- R6: “正在思考”必须由 provider waiting ephemeral event 驱动，且不受 durable replay cursor、重连、fork seed 或 synthetic feed event seq 影响。
- R7: 前端底部 helper、发送按钮、取消按钮、状态条和 fork 标识必须来自同一 command / execution snapshot；terminal durable event 必须触发 workspace conversation snapshot refresh，使 helper 不停留在旧 running reason。
- R8: 清理迁移过程中残留的废弃 active-state 读取路径、仅测试伪状态、重复 runtime-control 分支和无法再成为事实源的字段使用。
- R9: 为 stale status、fork replay、provider waiting、command availability 增加回归测试。
- R10: 最终收口必须审计所有 AgentRun scoped API、前端 service、workspace state、composer/helper/status/fork UI 对 RuntimeSession facts 的引用。`SessionRuntimeControlView`、`SessionShellDto.last_delivery_status`、`delivery_trace_meta.delivery_status`、`runtime_session_ref` 等只能作为 AgentRun 内部传输柄或诊断信息存在；用户可见状态必须来自 AgentRun execution / display snapshot。

## Acceptance Criteria

- [ ] 当 `runtime_sessions.last_delivery_status` 仍为 `running` 但 `inspect_session_execution_state` 返回 completed / idle / interrupted 时，AgentRun conversation snapshot、AgentRun scoped runtime control、workspace command availability 和前端 composer 均表现为非运行中。
- [ ] AgentRun scoped API 不再直接暴露 RuntimeSession control plane 作为用户状态；若保留 `/runtime/control`，它必须是 AgentRun execution snapshot 的投影或诊断包装，而不是 RuntimeSession 自行判定的状态面。
- [ ] 已完成父会话可以 fork；fork 后子会话首屏稳定展示父分支点前的历史内容，不出现空白或只剩新消息的结果。
- [ ] fork seed 不依赖会被 compaction / projection head 改写的模型上下文偶然形态；父会话 UI 可见历史切片和子 runtime durable event replay 有明确边界。
- [ ] 子会话 durable event 从自身 runtime cursor 开始 replay；父 seed 不进入 durable `lastAppliedSeq` 去重链路。
- [ ] provider `connected_waiting_first_delta` ephemeral event 能稳定触发“正在思考”，且不会被 durable cursor、重连或 fork seed 跳过。
- [ ] 前端 `SessionChatComposer` 在运行中、完成态、可发送态下展示的 helper 文案与命令可用性一致，turn terminal 后不会停留在旧 running reason。
- [ ] 全仓搜索确认 public active/running 判断不再依赖 `last_delivery_status == Running` 这类陈旧 summary 判断。
- [ ] 全仓搜索确认 AgentRun 前端和公开 API 没有把 `SessionRuntimeControlView`、`SessionShellDto`、`delivery_trace_meta`、`runtime_session_ref`、`session_meta.last_delivery_status` 当作用户可见运行态事实源；保留项必须被收束为 service 内部传输柄或显式诊断 contract。
- [ ] Rust fmt、Rust clippy、相关 Rust 测试、前端相关单测通过；若全仓测试存在与本任务无关的已知失败，需要在收尾说明中列明。

## Scope Notes

- 允许调整数据库迁移、模型字段和前后端 API 形态，目标是删掉错误事实源和错误组合方式。
- 本任务应优先修复事实源、workspace refresh 和 fork display seed，再做 UI 文案或组件细节收口。
- `last_delivery_status` 可以继续服务 RuntimeSession 启动恢复或历史 summary；它不能参与用户可见 active-state 决策，也不能让 RuntimeSession 越过 AgentRun 直接给 UI 提供运行态事实。
- RuntimeSession id / trace meta 可以作为 stream 连接和诊断定位使用，但不能在 AgentRun UI 层被重新解释成运行态、可发送态、可取消态或分支态。

## Evidence

- `crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:225` 读取 `inspect_session_execution_state` 后，又在 `:227` 将 `session_meta.last_delivery_status == Running` 合并进 `delivery_running`。
- `crates/agentdash-api/src/routes/sessions.rs:156` 的 `load_session_runtime_control_view` 调用 `presentation_read_model_query.session_runtime_control`；AgentRun `/runtime/control` 也复用该通用 session control 视图，因此 stale meta 会影响 AgentRun scoped runtime control。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:161`、`workspace/projection.rs:70`、`workspace/command_policy.rs:63` 已有仅由 `inspect_session_execution_state` / `SessionExecutionState` 推导 workspace conversation 和 command 的路径，说明 workspace 与 runtime control 可得出不同 running 结论。
- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.ts:255` 将 `conversation.execution.reason` 作为 helper；`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:697` 在 submit enabled 时仍优先显示该 helper，所以“会话正在进行中”可以是 stale conversation snapshot，不一定是 submit disabled。
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:374` 会在 turn terminal live event 后调用 `onTurnEnd`；`features/agent-run-workspace/model/controlPlaneModel.ts:69` 计划刷新 workspace state。若 UI 仍卡 running，需要检查 terminal event 是否进入 AgentRun scoped stream side effect，或刷新后后端仍返回 running。
- `crates/agentdash-application-runtime-session/src/session/branching.rs:467` fork child 初始 projection 使用 `parent_context.messages`，而 `eventing.rs:439` 的 parent context 来自 `build_agent_context_envelope`，即模型上下文投影，不是专门为 UI 历史构造的父 transcript slice。
- `packages/app-web/src/features/session/model/useSessionStream.ts:234` fork / AgentRun target 首次加载 conversation feed，并在 `:242` 把 `lastAppliedSeq` 重置为 `runtime_replay_start_seq`。
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts:662` durable event 以 `event_seq <= lastAppliedSeq` 去重，因此 inherited seed 与 child runtime event 必须明确分 lane。
- `packages/app-web/src/services/agentRunRuntime.ts:63` 的 `fetchAgentRunRuntimeControl` 返回 `SessionRuntimeControlView`，说明 AgentRun scoped frontend service 仍以 RuntimeSession control view 作为 contract。
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:508` 至 `:516` 将 `workspaceControl.delivery_trace_meta` 重新包装为 `sessionMeta.last_delivery_status`，这是需要收束的 AgentRun UI 暴露路径。
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:96` 通过 `fetchSessionRuntimeControl` 读取通用 session control；实现阶段需区分通用诊断入口与 AgentRun 用户状态入口，避免复用到 AgentRun 状态面。

## Open Questions

- 当前没有阻塞规划的问题；实现阶段需要通过测试确认 fork display seed 应直接复用父 runtime transcript 事件切片，还是在后端新增专门的 display projection。
