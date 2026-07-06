# Agent 生命周期模块边界维护

## Goal

将 Agent 生命周期、Hook 生效面、等待闭环、终态收敛、投影刷新和前端状态消费收束到稳定模块边界，减少 companion / wait / terminal / hook 场景特解继续扩张的空间。

本任务延续 [review.md](./review.md) 的结论，从架构审查进入实施准备：保留 `LifecycleGate` 作为 wait fact owner，保留 `AgentRunDeliveryBinding` 作为用户可见 running / terminal fact owner，保留 `AgentRunMailbox` 作为 wake / continuation delivery owner，保留 `AgentRun` / `LifecycleAgent` / `AgentFrame` 作为 Hook policy、runtime surface 和 control-plane effect 的业务 owner，保留 `RuntimeSession` / `BackboneEnvelope` 只作为 journal、trace evidence 和 observable stream。

## Background

已确认的事实源和风险：

- `crates/agentdash-domain/src/workflow/wait_obligation.rs` 将 wait declaration 写入 `LifecycleGate.payload_json`，没有独立 durable aggregate。
- `crates/agentdash-application-workflow/src/gate/wait_obligation.rs` 当前 service 名称通用，但实现仍 hardcode `companion_result` 和 companion parent delivery intent。
- `crates/agentdash-api/src/agent_run_terminal_control.rs` 已经先收敛 `AgentRunDeliveryBinding`，再将 terminal event 映射为 wait producer terminal event。
- `crates/agentdash-application-runtime-session/src/session/terminal_effects.rs` 当前持有 `runtime_session_terminal_effects` durable replay outbox，其中 `hook_effects`、`hook_auto_resume`、`session_terminal_callback` 都是 AgentRun / Hook control-plane effect，不应继续归属 RuntimeSession 层。
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` 当前有 `MailboxStateChanged` 与 `SessionMetaUpdate`，缺少通用 control-plane projection invalidation event。
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts` 当前通过多个 companion-specific `session_meta_update.key` 触发 workspace refresh。
- `packages/app-web/src/features/session/model/useTerminalStore.ts` 当前 terminal event 去重按 event seq，缺少 stream identity scope。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` 当前在 API route 层追加 exec terminal waiting rows，说明 exec/wait activity projection 仍未归到 AgentRun control-plane owner。
- `crates/agentdash-application/src/companion/notifications.rs`、`crates/agentdash-workspace-module/src/workspace_module/surface.rs` 等路径仍通过 `SessionMetaUpdate` 自由 key 触发业务刷新。

## First Principles

- `RuntimeSession` 只保存会话 journal、connector trace、event replay cursor 和可观测 stream。它不能拥有 Hook policy、terminal effect、workspace title、waiting item、mailbox wake、AgentRun running state 或前端业务刷新规则。
- Hook 对 `AgentRun` 生效：Hook runtime target 是 `run_id + agent_id + frame_id`，`delivery_runtime_session_id` 只是当前投递 trace evidence。Hook effect、Hook auto-resume 和 Hook runtime projection 都归 AgentRun control-plane。
- Terminal event 是 evidence，不是业务终态 owner。用户可见 running / terminal 由 `AgentRunDeliveryBinding` 收敛，后续 wait/gate/lifecycle/mailbox/hook side effect 都从 AgentRun control-plane effect executor 推进。
- Frontend 只消费后端 snapshot 和 typed projection invalidation。任何 `session_meta_update.key` allowlist、companion-specific refresh key、route-level waiting 拼接都属于第二事实源，应在本任务中清掉。

## Requirements

### R1. Wait/Gate 边界

- 保留 `LifecycleGate.payload_json` 的灵活 payload 形态，不新增 wait declaration 独立表。
- 在 domain 层提供 typed gate wait payload envelope，统一维护 `schema_version`、`wait_source`、`expected_result`、`terminal_policy`、`wake_target` 的 parse / serialize / validation。
- 将现有 `wait_obligation` 语义收束为 gate producer terminal convergence；generic service 里不出现 companion-specific intent / delivery 类型名。
- repository 层保留 JSONB 查询实现，但查询语义必须由 typed helper 驱动，避免业务层散落 JSON path 判断。
- companion 仍可使用 companion-specific request / response payload，但只能作为 adapter source identity 和 display metadata。

### R2. AgentRun-Owned Control Effects

- 将 `runtime_session_terminal_effects` 迁移为 AgentRun/control-plane owned outbox，例如 `agent_run_control_effects`，不再以 RuntimeSession 命名或作为 RuntimeSession repository 的业务副作用表。
- typed effect payload 必须以 `run_id + agent_id + frame_id` 为业务 scope，并保留 `delivery_runtime_session_id`、`turn_id`、`terminal_event_seq` 作为 trace evidence。
- 扩展 typed effect kind / payload executor，将 terminal 后继 control-plane effect 拆为幂等步骤：AgentRun delivery convergence、wait producer terminal convergence、Lifecycle terminal convergence、mailbox wake delivery、hook effects、hook auto-resume delivery、hook runtime projection invalidation。
- RuntimeSession terminal event 继续作为 evidence；用户可见状态仍由 `AgentRunDeliveryBinding` 表达，Hook 生效面仍由 AgentRun / AgentFrame control target 表达。
- boot reconcile、durable replay、terminal callback failure retry 必须复用同一 typed AgentRun control effect executor，不走 RuntimeSession-owned callback fanout 或 companion/gate 私有补偿路径。

### R3. Projection Event

- 在 Backbone `PlatformEvent` 增加 `ControlPlaneProjectionChanged`。
- 事件 payload 使用 generated TS contract 输出，至少表达 `projection`、`reason`、`run_id`、`agent_id`，并支持可选 `frame_id`、`gate_id`、`mailbox_message_id`、`delivery_runtime_session_id`。
- projection taxonomy 至少覆盖 `workspace`、`agent_run_list`、`mailbox`、`waiting`、`delivery`、`hook_runtime`、`resource_surface`、`title`。
- companion result、mailbox state、wait resolved、delivery terminal、hook runtime change、workspace module presentation、capability/context frame change、title/list refresh 等路径统一 emit 通用 projection invalidation event。
- 删除 companion-specific projection / refresh 旧事件路径；需要展示的 companion request / result UI 也应消费通用 projection 或后端 snapshot，不保留 legacy `companion_*` session meta event 作为第二条事件协议。
- `SessionMetaUpdate` 不再承担 AgentRun workspace/control-plane refresh；仅允许作为 RuntimeSession trace/journal 内部遥测存在，且不能被 AgentRun workspace model 当作业务刷新入口。

### R4. Session Residue Excision

- 移除 `RuntimeSession` 层对业务 effect replay 的 ownership：`SessionTerminalEffectStore`、`TerminalEffectType::{HookEffects, HookAutoResume, SessionTerminalCallback}`、background replay worker、bootstrap wiring 迁到 AgentRun control effect 端口和实现。
- 移除 `SessionTerminalCallback` composite fanout 作为业务收敛协调点；RuntimeSession terminal processor 只把 terminal evidence 交给 AgentRun control effect intake。
- 移除 `controlPlaneModel` 对 `session_meta_update.key`、`companion_*`、`mailbox_state_changed` 等自由事件名的业务 refresh allowlist。
- 移除 `append_exec_terminal_waiting_items` 这类 API route-level waiting projection 拼接；exec waiting rows 由 AgentRun wait/activity projection 或 terminal activity projection 进入 workspace snapshot。
- 移除 terminal store 裸 event seq 去重；事件幂等 key 必须包含 AgentRun journal / RuntimeSession stream identity。
- 清理 `companion`、`workspace_module`、`hook/runtime context` 等已确认路径对 `SessionMetaUpdate` 自由 key 的业务刷新依赖，统一走 `ControlPlaneProjectionChanged`。

### R5. Frontend Boundary

- `controlPlaneModel` 以 `ControlPlaneProjectionChanged` 为 workspace / mailbox / wait / hook runtime refresh 入口。
- companion-specific `session_meta_update.key` refresh 分支删除；前端不再维护 companion refresh allowlist。
- `SessionCompanionRequestCard` 提交响应后只触发 snapshot refresh；“已回应/已结束”从 workspace waiting projection 或后端 gate/result 投影读取。
- terminal store event dedup 使用 stream identity + event seq，避免跨 AgentRun journal event seq 冲突。
- AgentRun workspace command availability、cancel、waiting、running helper 继续从 `AgentRunWorkspaceView.conversation` / `mailbox.waiting_items` / `shell` 读取。

## Acceptance Criteria

- [ ] Gate wait payload 的 parse / serialize / invalid payload diagnostic 有单元测试覆盖。
- [ ] Producer terminal convergence 不再暴露 companion-specific generic service / intent 名称。
- [ ] Companion child terminal without result 会 resolved gate，并向 parent mailbox 投递一次 wake。
- [ ] Normal companion result 与 producer terminal race 时，已有 result 不被 terminal convergence 覆盖。
- [ ] AgentRun-owned control effect outbox replay 重复执行不会重复写 delivery binding、gate、mailbox wake 或 hook auto-resume envelope。
- [ ] Backbone generated TS 包含 `ControlPlaneProjectionChanged`。
- [ ] Frontend refresh plan 对通用 projection event 有测试覆盖。
- [ ] Companion-specific projection / refresh 旧事件路径已删除，前端不再依赖 `companion_*` session meta event。
- [ ] AgentRun workspace refresh 不再依赖 `SessionMetaUpdate` / `MailboxStateChanged` 自由事件名。
- [ ] API route 层不再临时追加 exec waiting rows；workspace waiting projection 由 AgentRun wait/activity owner 提供。
- [ ] Terminal store scoped dedup 有测试覆盖。
- [ ] `cargo test -p agentdash-application-workflow` 通过。
- [ ] `cargo test -p agentdash-application-agentrun` 通过。
- [ ] `cargo test -p agentdash-application-runtime-session` 通过。
- [ ] `cargo test -p agentdash-api` 通过。
- [ ] `pnpm run contracts:check` 通过且 generated drift 被纳入提交。

## Out Of Scope

- 不新增 wait declaration 独立表。
- 不做旧 API / 旧 payload 兼容层。
- 不将 Mailbox 并入 Backbone；Mailbox 继续作为 durable command / wake / continuation 信道。
- 不保留 companion-specific projection event 兼容分支。

## Open Questions

无阻塞问题。当前默认决策：灵活 payload + typed envelope；Hook / terminal / wait / mailbox 后继副作用迁移到 AgentRun control-effect outbox；新增通用 projection invalidation event，并删除旧 companion-specific 与 SessionMetaUpdate-based projection / refresh 事件路径。
