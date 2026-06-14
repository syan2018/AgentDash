# Research: AgentRun delivery idempotency, launch accepted boundary, hook runtime target refresh

- Query: AgentRun message 投递幂等、Launch accepted boundary、HookRuntime target refresh 的当前代码路径与实施风险。
- Scope: internal
- Date: 2026-06-11

## Findings

### Files Found

- `crates/agentdash-application/src/workflow/agent_message.rs` — AgentRun message use case；从 delivery RuntimeSession 反查 run/agent/frame 后调用 session launch。
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs` — ProjectAgent 首轮启动；先 materialize lifecycle/runtime，再投递首条 message，失败后尝试清理空 session/run。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` — 当前 `/sessions/{runtime_session_id}/messages` API 入口；做 permission check 后调用 `AgentRunMessageService`。
- `crates/agentdash-api/src/routes/project_agents.rs` — 当前 `/projects/{id}/agents/{project_agent_id}/sessions` 首轮创建入口；调用 `ProjectAgentSessionStartService`。
- `crates/agentdash-application/src/session/launch/orchestrator.rs` — launch 主阶段编排；construction、planning、preparation、connector start、commit、stream attach。
- `crates/agentdash-application/src/session/launch/connector_start.rs` — `connector.prompt(...)` accepted 边界；成功返回 `ExecutionStream` 后生成 `ConnectorAcceptedTurn`。
- `crates/agentdash-application/src/session/launch/commit.rs` — accepted 后提交 user input / turn started / context frames / runtime command applied / session meta。
- `crates/agentdash-application/src/session/launch/preparation.rs` — accepted 前准备 `ExecutionContext`、hook runtime、pending runtime context application、ContextFrame 队列。
- `crates/agentdash-application/src/session/launch/command.rs` — `LaunchSource::LifecycleAgentUserMessage` 和 source reason 定义。
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs` — hub 内从 delivery RuntimeSession 懒重建 hook runtime、emit hook trigger、runtime context update injection。
- `crates/agentdash-application/src/session/hooks_service.rs` — frame target 优先的 hook runtime service；校验 runtime session + frame target 一致。
- `crates/agentdash-spi/src/session_persistence.rs` — session persistence ports；包含 runtime command record/outbox 类型，但没有 user delivery command receipt。
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` — PostgreSQL `SessionRuntimeCommandStore` 实现。
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` — runtime command row 解码和 delivery/frame transition 一致性校验。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` — 当前 schema；已有 `agent_frame_transitions` 与 `session_runtime_commands`。
- `packages/app-web/src/generated/workflow-contracts.ts` — 前端生成的 `AgentRunMessageRequest`，当前无 `client_command_id`。
- `packages/app-web/src/generated/project-agent-contracts.ts` — 前端生成的 `CreateProjectAgentSessionRequest`，当前无 `client_command_id`。
- `crates/agentdash-application/src/session/core.rs` — RuntimeSession meta 创建、查询、执行状态推导和 interrupted recovery 的应用服务入口。
- `crates/agentdash-application/src/session/eventing.rs` — runtime event 持久化入口；通过 `SessionEventStore::append_event` 投影更新 session meta，并处理 source title 回写。
- `crates/agentdash-application/src/session/hub_support.rs` — `TurnStarted` / `turn_terminal` envelope 构造和 `SessionMeta -> SessionExecutionState` 投影。
- `crates/agentdash-application/src/session/runtime_control.rs` — 启动恢复时把 stale running meta 标记为 interrupted。
- `crates/agentdash-application/src/session/turn_processor.rs` — terminal event 持久化与 active turn cleanup 顺序。
- `crates/agentdash-application/src/session/title_service.rs` — 用户手动 session title 更新。
- `crates/agentdash-api/src/routes/sessions.rs` — session shell / runtime-control / list DTO 对 `SessionMeta` 的 API 投影。
- `.trellis/tasks/06-11-session-model-delivery-state-chain/prd.md` — 父任务已要求 `client_command_id`、command receipt、digest conflict 与 retry reuse。
- `.trellis/tasks/06-11-session-model-delivery-state-chain/design.md` — 父任务已定义 canonical route 与 command receipt 行为草案。
- `.trellis/tasks/06-11-session-model-delivery-state-chain/review.md` — 已有 review 记录指出两个 request DTO 无 `client_command_id`，且 `session_runtime_commands` 不是 user delivery receipt。

### Current AgentRun Message Delivery Path

Current API path is still runtime-session scoped:

- `crates/agentdash-api/src/routes/lifecycle_agents.rs:28` registers `POST /sessions/{runtime_session_id}/messages`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:53` receives `AgentRunMessageRequest`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:107` creates `AgentRunMessageLaunchDeliveryPort`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:117` calls `AgentRunMessageService::dispatch_user_message`.

The use case resolves control-plane refs before launch:

- `crates/agentdash-application/src/workflow/agent_message.rs:15` defines `AgentRunMessageCommand`; it currently carries `delivery_runtime_session_id`, `input`, optional `executor_config`, optional `identity`, but no idempotency key.
- `crates/agentdash-application/src/workflow/agent_message.rs:41` defines `AgentRunMessageDeliveryPort`.
- `crates/agentdash-application/src/workflow/agent_message.rs:75` converts delivery into `LaunchCommand::lifecycle_agent_user_message_input`.
- `crates/agentdash-application/src/workflow/agent_message.rs:76` calls `SessionLaunchService::launch_command`.
- `crates/agentdash-application/src/workflow/agent_message.rs:115` is the main dispatch entry.
- `crates/agentdash-application/src/workflow/agent_message.rs:131` resolves run/agent/frame by `RuntimeSessionExecutionAnchor`.
- `crates/agentdash-application/src/workflow/agent_message.rs:136` delegates actual connector launch.
- `crates/agentdash-application/src/workflow/agent_message.rs:154` begins `resolve_control_plane`.
- `crates/agentdash-application/src/workflow/agent_message.rs:205` validates the resolved frame belongs to the resolved agent.

ProjectAgent first message path materializes before delivery:

- `crates/agentdash-api/src/routes/project_agents.rs:245` receives `CreateProjectAgentSessionRequest`.
- `crates/agentdash-api/src/routes/project_agents.rs:268` calls `ProjectAgentSessionStartService::start_session`.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:127` is the service entry.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:183` calls `LifecycleDispatchService::launch_agent`.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:185` extracts `delivery_runtime_ref`.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:194` binds `project_agent_id` to `LifecycleAgent`.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:226` dispatches first message through `AgentRunMessageService`.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:289` begins cleanup for failures.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:298` only cleans if `meta.last_event_seq == 0`; once an event exists, cleanup does not remove the materialized run/session.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:306` deletes the empty runtime session.
- `crates/agentdash-application/src/workflow/project_agent_session_start.rs:307` deletes the lifecycle run.

Implication for idempotency: `ProjectAgentSessionStartService` must be part of the receipt boundary, not only `AgentRunMessageService`, because retries after transport failure may happen after lifecycle/runtime materialization but before the client receives `ProjectAgentSessionStartResult`.

### Current DTO / Contract State

- `crates/agentdash-contracts/src/workflow.rs:748` defines `AgentRunMessageRequest`.
- `crates/agentdash-contracts/src/workflow.rs:750` has `input`.
- `crates/agentdash-contracts/src/workflow.rs:753` has `executor_config`.
- `crates/agentdash-contracts/src/project_agent.rs:69` defines `CreateProjectAgentSessionRequest`.
- `crates/agentdash-contracts/src/project_agent.rs:71` has `input`.
- `crates/agentdash-contracts/src/project_agent.rs:74` has `executor_config`.
- `crates/agentdash-contracts/src/project_agent.rs:77` has `subject_ref`.
- `packages/app-web/src/generated/workflow-contracts.ts:35` mirrors `AgentRunMessageRequest`.
- `packages/app-web/src/generated/workflow-contracts.ts:39` shows the generated request fields are `input` and optional `executor_config`.
- `packages/app-web/src/generated/project-agent-contracts.ts:13` mirrors `CreateProjectAgentSessionRequest`.
- `packages/app-web/src/generated/project-agent-contracts.ts:17` shows `input`, optional `executor_config`, optional `subject_ref`.

No current request contract carries `client_command_id`; durable user delivery idempotency requires DTO/API/use-case/store changes.

### Launch Accepted Boundary

The launch stage pipeline follows the documented accepted boundary for most persisted facts:

- `crates/agentdash-application/src/session/launch/orchestrator.rs:44` claims the prompt before launch.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:69` reads requested runtime commands before construction/planning.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:90` builds frame construction.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:346` starts `TurnPreparer`.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:366` starts `ConnectorStarter`.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:381` commits only after `ConnectorStarter` returns accepted.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:389` attaches stream ingestion after commit.
- `crates/agentdash-application/src/session/launch/connector_start.rs:24` starts with `PreparedTurn`.
- `crates/agentdash-application/src/session/launch/connector_start.rs:31` calls `connector.prompt(...)`.
- `crates/agentdash-application/src/session/launch/connector_start.rs:46` clears turn/hook on connector prompt error.
- `crates/agentdash-application/src/session/launch/connector_start.rs:64` returns `ConnectorAcceptedTurn { prepared, stream }` only after prompt success.
- `crates/agentdash-application/src/session/launch/commit.rs:25` begins accepted commit.
- `crates/agentdash-application/src/session/launch/commit.rs:35` commits accepted launch events.
- `crates/agentdash-application/src/session/launch/commit.rs:115` builds `UserInputSubmitted`.
- `crates/agentdash-application/src/session/launch/commit.rs:130` builds `TurnStarted`.
- `crates/agentdash-application/src/session/launch/commit.rs:155` marks pending runtime commands applied.

Important risk: `SessionLaunchOrchestrator` currently writes an initial `AgentFrame` revision before connector accepted:

- `crates/agentdash-application/src/session/launch/orchestrator.rs:286` begins the "initial capability state write" block before `TurnPreparer` and `ConnectorStarter`.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:302` creates `AgentFrameBuilder`.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:311` persists the frame revision.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:324` updates `LifecycleAgent.current_frame_id`.

This block has higher implementation risk than the normal commit path because connector setup can fail after frame/current-frame mutation but before user input / `TurnStarted` are committed. If command receipt semantics define "accepted" as connector accepted, receipt finalization and any new durable accepted refs should not observe this pre-accepted frame write as proof of accepted delivery.

### Runtime Command / Session Command Store Reuse

There is an existing durable runtime command outbox, but it is scoped to runtime context/capability delivery, not user message idempotency:

- `crates/agentdash-spi/src/session_persistence.rs:354` defines `RuntimeCommandStatus` with `Requested`, `Applied`, `Failed`.
- `crates/agentdash-spi/src/session_persistence.rs:385` defines `RuntimeCommandRecord`.
- `crates/agentdash-spi/src/session_persistence.rs:409` defines `RuntimeDeliveryCommandKind`.
- `crates/agentdash-spi/src/session_persistence.rs:415` defines `RuntimeDeliveryCommand`.
- `crates/agentdash-spi/src/session_persistence.rs:422` only constructs `pending_runtime_context`.
- `crates/agentdash-spi/src/session_persistence.rs:841` defines `SessionRuntimeCommandStore`.
- `crates/agentdash-spi/src/session_persistence.rs:842` exposes `upsert_runtime_delivery_command`.
- `crates/agentdash-spi/src/session_persistence.rs:848` exposes `list_requested_runtime_commands`.
- `crates/agentdash-spi/src/session_persistence.rs:852` exposes `mark_runtime_commands_applied`.
- `crates/agentdash-spi/src/session_persistence.rs:853` exposes `mark_runtime_commands_failed`.

PostgreSQL behavior:

- `crates/agentdash-infrastructure/migrations/0001_init.sql:28` creates `agent_frame_transitions`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:629` creates `session_runtime_commands`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:630` primary id column is `id`; no `client_command_id`, `scope`, or request digest column exists.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:640` requires `frame_transition_id`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1172` indexes `frame_transition_id`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1174` indexes `(session_id, status)`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1176` indexes `(status, updated_at_ms)`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1225` links runtime commands to `agent_frame_transitions`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:1273` links runtime commands to `sessions`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:660` implements `SessionRuntimeCommandStore`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:667` validates delivery and frame transition match.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:676` marks older requested commands for the same session + phase_node as failed.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:705` upserts `agent_frame_transitions` by transition id.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:733` allocates a fresh runtime command id.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:750` inserts a new `session_runtime_commands` row.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:774` lists requested commands by session/status.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:805` marks commands applied.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:813` marks commands failed.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:136` decodes runtime command rows.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:153` rejects mismatch between delivery payload, command row, and frame transition.

Conclusion: `session_runtime_commands` can inform state machine naming and accepted-after-connector discipline, but should not be reused directly for user delivery idempotency unless its schema is generalized. It currently assumes every command references an `agent_frame_transitions` row and only serializes `RuntimeDeliveryCommandKind::PendingRuntimeContext`.

The only existing `idempotency_key` in current migrations belongs to removed/legacy activity execution claims:

- `crates/agentdash-infrastructure/migrations/0001_init.sql:8` has `activity_execution_claims.idempotency_key`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:814` makes that key unique.
- `crates/agentdash-infrastructure/migrations/0004_orchestration_runtime_convergence.sql` drops `activity_execution_claims`.

That table is not reusable for AgentRun message delivery.

### HookRuntime Target Refresh Path

Hook runtime is now frame-target-aware but cached by delivery runtime session:

- `crates/agentdash-application/src/session/hooks_service.rs:32` ensures hook runtime by target through delivery runtime session id.
- `crates/agentdash-application/src/session/hooks_service.rs:37` validates cached/rebuilt runtime target.
- `crates/agentdash-application/src/session/hooks_service.rs:57` reloads hook runtime for a session id.
- `crates/agentdash-application/src/session/hooks_service.rs:77` resolves runtime hook target from provider.
- `crates/agentdash-application/src/session/hooks_service.rs:89` loads frame snapshot.
- `crates/agentdash-application/src/session/hooks_service.rs:111` builds `AgentFrameHookRuntime`.
- `crates/agentdash-application/src/session/hooks_service.rs:137` resolves hook runtime during launch.
- `crates/agentdash-application/src/session/hooks_service.rs:147` reloads on owner bootstrap or missing runtime.
- `crates/agentdash-application/src/session/hooks_service.rs:161` refreshes existing runtime on subsequent turn.
- `crates/agentdash-application/src/session/hooks_service.rs:185` validates target mismatch.
- `crates/agentdash-application/src/session/hooks_service.rs:223` builds runtime and validates frame/anchor ownership.
- `crates/agentdash-application/src/session/hooks_service.rs:278` delegates `resolve_runtime_hook_target`.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:190` lazily rebuilds hook runtime for delivery session.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:213` resolves hook target.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:217` loads frame snapshot.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:231` builds hook runtime.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:238` caches runtime if absent.

Risk: `resolve_hook_runtime` refreshes an existing runtime on subsequent turns but does not re-resolve the target before refresh. If `LifecycleAgent.current_frame_id` changes between turns, the cached `AgentFrameHookRuntime` can remain bound to the old frame. `ensure_hook_runtime_for_target` detects mismatch when the caller already has a target, but the launch path itself uses session-level `resolve_hook_runtime`, so target refresh should be reviewed there.

Runtime context update path is intentionally not an immediate live notification to Agent:

- `crates/agentdash-application/src/session/hub/hook_dispatch.rs:160` collects runtime context update injections from current hook snapshot.
- `crates/agentdash-application/src/session/launch/preparation.rs:202` triggers `HookTrigger::SessionStart` only for owner bootstrap.
- `crates/agentdash-application/src/session/launch/preparation.rs:215` is the `SessionStart` trigger.
- `crates/agentdash-application/src/session/launch/preparation.rs:277` emits pending action frames.
- `crates/agentdash-application/src/session/launch/preparation.rs:281` dedupes `ContextFrame`s for turn context.

### SessionMeta Runtime Trace Responsibilities

Current `SessionMeta` mixes shell identity, trace-head projection, delivery state cache, connector follow-up, and title metadata:

- `crates/agentdash-spi/src/session_persistence.rs:304` defines `SessionMeta`.
- `crates/agentdash-spi/src/session_persistence.rs:305` stores runtime session id.
- `crates/agentdash-spi/src/session_persistence.rs:306` stores title.
- `crates/agentdash-spi/src/session_persistence.rs:308` stores `title_source`.
- `crates/agentdash-spi/src/session_persistence.rs:309` stores created timestamp.
- `crates/agentdash-spi/src/session_persistence.rs:310` stores updated timestamp.
- `crates/agentdash-spi/src/session_persistence.rs:312` stores `last_event_seq`.
- `crates/agentdash-spi/src/session_persistence.rs:314` stores `last_delivery_status`.
- `crates/agentdash-spi/src/session_persistence.rs:316` stores `last_turn_id`.
- `crates/agentdash-spi/src/session_persistence.rs:318` stores `last_terminal_message`.
- `crates/agentdash-spi/src/session_persistence.rs:320` stores `executor_session_id`.

Schema matches this shape:

- `crates/agentdash-infrastructure/migrations/0001_init.sql:657` creates `sessions`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:662` defines `last_event_seq`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:663` defines `last_delivery_status`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:664` defines `last_turn_id`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:665` defines `last_terminal_message`.
- `crates/agentdash-infrastructure/migrations/0001_init.sql:666` defines `executor_session_id`.

Creation paths initialize these as runtime-session trace defaults:

- `crates/agentdash-application/src/session/core.rs:54` creates ad-hoc session meta.
- `crates/agentdash-application/src/session/core.rs:60` starts `last_event_seq` at `0`.
- `crates/agentdash-application/src/session/core.rs:61` starts `last_delivery_status` as `Idle`.
- `crates/agentdash-application/src/session/core.rs:62` starts `last_turn_id` as `None`.
- `crates/agentdash-application/src/session/core.rs:63` starts `last_terminal_message` as `None`.
- `crates/agentdash-application/src/session/core.rs:64` starts `executor_session_id` as `None`.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:67` creates RuntimeSession meta for lifecycle dispatch.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:73` initializes its trace head.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:74` initializes delivery status.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:75` initializes last turn.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:76` initializes terminal message.
- `crates/agentdash-application/src/workflow/dispatch_service.rs:77` initializes executor follow-up id.

Write/update paths:

- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:216` implements `save_session_meta`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:229` keeps `last_event_seq` monotonic with `GREATEST`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:230` only lets `last_delivery_status` overwrite when the incoming `last_event_seq` is not stale.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:235` applies the same non-stale guard to `last_turn_id`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:240` applies the same non-stale guard to `last_terminal_message`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:245` always writes `executor_session_id` from the explicit meta save path.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:319` implements `append_event`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:332` increments `sessions.last_event_seq`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:347` projects envelope facts into meta fields.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:383` updates `sessions` from the event projection.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:388` writes projected delivery status.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:389` writes projected `last_turn_id`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:390` clears or writes `last_terminal_message`.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:395` writes projected `executor_session_id`.

Event projection logic:

- `crates/agentdash-infrastructure/src/persistence/session_core.rs:659` describes envelope-to-session projection.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:670` starts `projection_from_envelope`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:686` maps `TurnStarted` to `running` and clears terminal message.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:690` maps `TurnCompleted` to completed/failed/interrupted.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:701` stores terminal status.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:702` stores provider turn error message.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:704` maps generic error events to failed.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:708` maps `ExecutorSessionBound` into `executor_session_id`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:713` maps `SessionMetaUpdate(key="turn_terminal")` into terminal status/message.
- `crates/agentdash-application/src/session/hub_support.rs:46` constructs `TurnStarted`.
- `crates/agentdash-application/src/session/hub_support.rs:90` constructs `SessionMetaUpdate(key="turn_terminal")`.
- `crates/agentdash-application/src/session/turn_processor.rs:124` builds terminal notification after stream terminal/cancel.
- `crates/agentdash-application/src/session/turn_processor.rs:132` persists terminal notification.
- `crates/agentdash-application/src/session/turn_processor.rs:141` clears active turn even when terminal persist fails.
- `crates/agentdash-application/src/session/turn_processor.rs:174` notes `executor_session_id` sync is handled by `append_event` projection, not direct processor writes.

Accepted commit path still directly saves a running meta snapshot after persisting accepted launch events:

- `crates/agentdash-application/src/session/launch/commit.rs:35` persists accepted launch events.
- `crates/agentdash-application/src/session/launch/commit.rs:58` applies turn-start meta.
- `crates/agentdash-application/src/session/launch/commit.rs:65` saves session meta.
- `crates/agentdash-application/src/session/launch/commit.rs:147` sets `last_delivery_status = Running`.
- `crates/agentdash-application/src/session/launch/commit.rs:148` sets `last_turn_id`.
- `crates/agentdash-application/src/session/launch/commit.rs:149` clears terminal message.

This is partially redundant with the `TurnStarted` event projection. Because `save_session_meta` has stale-event guards, it is usually safe, but it is an implementation hotspot when introducing command receipts: command accepted state should come from the launch accepted/commit boundary, while `SessionMeta` should remain only a RuntimeSession trace-head cache.

Runtime reads:

- `crates/agentdash-application/src/session/core.rs:30` recovers sessions whose meta status is `Running`.
- `crates/agentdash-application/src/session/core.rs:150` inspects execution state.
- `crates/agentdash-application/src/session/core.rs:154` first checks in-memory runtime registry.
- `crates/agentdash-application/src/session/core.rs:165` falls back to persisted meta when no active runtime exists.
- `crates/agentdash-application/src/session/hub_support.rs:324` derives `SessionExecutionState` from meta fields.
- `crates/agentdash-application/src/session/hub_support.rs:331` matches `last_delivery_status`.
- `crates/agentdash-application/src/session/hub_support.rs:333` requires `last_turn_id` for completed.
- `crates/agentdash-application/src/session/hub_support.rs:340` requires `last_turn_id` for failed.
- `crates/agentdash-application/src/session/hub_support.rs:346` carries `last_terminal_message` for failed.
- `crates/agentdash-application/src/session/hub_support.rs:352` treats stale `Running` without live runtime as interrupted.
- `crates/agentdash-application/src/session/runtime_control.rs:117` scans all meta rows on startup.
- `crates/agentdash-application/src/session/runtime_control.rs:120` filters `Running`.
- `crates/agentdash-application/src/session/runtime_control.rs:127` uses `last_turn_id` or generates a recovery turn id.
- `crates/agentdash-application/src/session/runtime_control.rs:131` writes an interrupted terminal event.

Launch reads:

- `crates/agentdash-application/src/session/launch/orchestrator.rs:48` reads `SessionMeta` at launch start.
- `crates/agentdash-application/src/session/launch/orchestrator.rs:88` converts it into `RuntimeTraceLaunchState`.
- `crates/agentdash-application/src/session/types.rs:107` says launch consumes runtime trace facts.
- `crates/agentdash-application/src/session/types.rs:110` includes `executor_session_id`.
- `crates/agentdash-application/src/session/types.rs:111` includes `last_event_seq`.
- `crates/agentdash-application/src/session/types.rs:123` maps `SessionMeta` to `RuntimeTraceLaunchState`.
- `crates/agentdash-application/src/session/types.rs:139` notes `LifecycleAgent.needs_bootstrap()` already replaced old `SessionMeta.bootstrap_state`.
- `crates/agentdash-application/src/session/types.rs:152` uses `last_event_seq > 0` to detect cold-start rehydrate.
- `crates/agentdash-application/src/session/launch/planner.rs:102` resolves prompt lifecycle from runtime trace state plus live runtime and agent bootstrap state.
- `crates/agentdash-application/src/session/launch/planner.rs:195` resolves follow-up session.
- `crates/agentdash-application/src/session/launch/planner.rs:203` uses `runtime_trace_state.executor_session_id`.
- `crates/agentdash-application/src/session/launch/planner.rs:208` labels that follow-up source as `SessionMeta`.

API / view reads:

- `crates/agentdash-api/src/routes/sessions.rs:160` builds runtime-control view.
- `crates/agentdash-api/src/routes/sessions.rs:161` loads meta.
- `crates/agentdash-api/src/routes/sessions.rs:173` returns unbound trace view using meta when no control-plane anchor exists.
- `crates/agentdash-api/src/routes/sessions.rs:255` inspects runtime execution state.
- `crates/agentdash-api/src/routes/sessions.rs:259` treats either meta running or runtime running as delivery running.
- `crates/agentdash-api/src/routes/sessions.rs:340` returns `SessionRuntimeControlView`.
- `crates/agentdash-api/src/routes/sessions.rs:555` builds project session list entries.
- `crates/agentdash-api/src/routes/sessions.rs:561` exposes meta delivery status for unbound trace sessions.
- `crates/agentdash-api/src/routes/sessions.rs:599` exposes meta delivery status for anchored sessions.
- `crates/agentdash-api/src/routes/sessions.rs:618` converts meta to `SessionShellDto`.
- `crates/agentdash-api/src/routes/sessions.rs:625` exposes `last_event_seq`.
- `crates/agentdash-api/src/routes/sessions.rs:626` exposes `last_turn_id`.
- `crates/agentdash-api/src/routes/sessions.rs:627` exposes `last_delivery_status`.
- `crates/agentdash-contracts/src/workflow.rs:712` defines `SessionShellDto`.
- `crates/agentdash-contracts/src/workflow.rs:718` includes `last_event_seq`.
- `crates/agentdash-contracts/src/workflow.rs:721` includes `last_turn_id`.
- `crates/agentdash-contracts/src/workflow.rs:722` includes `last_delivery_status`.
- `crates/agentdash-contracts/src/workflow.rs:873` comments `AgentRunView.last_delivery_status` as agent latest execution status.
- `crates/agentdash-application/src/workflow/lifecycle_run_view_builder.rs:368` currently leaves `AgentRunView.last_delivery_status` as `None`.

Title writes:

- `crates/agentdash-application/src/session/launch/deps.rs:235` derives local auto title from first prompt.
- `crates/agentdash-application/src/session/launch/deps.rs:242` writes derived title through `update_session_meta`.
- `crates/agentdash-application/src/session/eventing.rs:153` handles source session title projection.
- `crates/agentdash-application/src/session/eventing.rs:183` validates incoming source title against `executor_session_id` when present.
- `crates/agentdash-application/src/session/eventing.rs:201` writes source title.
- `crates/agentdash-application/src/session/title_service.rs:18` handles user manual title.
- `crates/agentdash-application/src/session/title_service.rs:25` writes manual title via `update_session_meta`.

### SessionMeta Migration Assessment

Recommended ownership split:

| Field / concept | Keep as RuntimeSession trace metadata? | New owner / projection note |
| --- | --- | --- |
| `id` | Yes | RuntimeSession shell id; AgentRun should reference it through `RuntimeSessionExecutionAnchor`, not replace it. |
| `created_at` / `updated_at` | Yes | RuntimeSession trace/list ordering. Agent/Lifecycle timestamps already live on `LifecycleAgent`/`LifecycleRun`; do not reuse session timestamps as business lifecycle timestamps. |
| `last_event_seq` | Yes | Event-log head and projection checkpoint. Branching and rollback use it as session trace head. It is not an AgentRun command state. |
| `executor_session_id` | Yes | Connector-native follow-up / restore handle. It is runtime transport metadata, not AgentFrame capability/context and not LifecycleAgent status. |
| `title` / `title_source` | Mostly yes | Today title is a session shell/list concern and source title is tied to executor session. If AgentRun workspace becomes primary UX, add an AgentRun display projection that may copy or derive title from session, but keep executor source title on RuntimeSession trace metadata. |
| `last_delivery_status` | Keep only as denormalized RuntimeSession event projection | Do not use as command receipt authority. AgentRun command receipt should own user command status/accepted result. AgentRun/Lifecycle view can project latest runtime delivery status from receipt or latest RuntimeSession trace, but `LifecycleAgent.status` remains lifecycle status. |
| `last_turn_id` | Keep only as trace-head shortcut | Command receipt should store accepted `turn_id` for the specific command. `last_turn_id` is useful for session shell/recovery but not sufficient for idempotent retry. |
| `last_terminal_message` | Keep only as trace-head terminal summary | Command receipt may store terminal failure/accepted failure for the command; LifecycleAgent should not own provider terminal text unless a lifecycle policy materializes it into a run/agent outcome. |
| running/idle action state | Derived, not owned by SessionMeta | Runtime-control should derive from runtime registry + meta trace + command receipts + lifecycle agent status. Meta alone is stale after process crash and is already treated as interrupted when no live runtime exists. |

Specific migration conclusions:

1. `last_delivery_status`, `last_turn_id`, and `last_terminal_message` should remain as RuntimeSession trace-head caches because they are derived from `session_events` and power unbound trace display, restart recovery, session list, and `SessionExecutionState` fallback. They should not be the source of truth for AgentRun command idempotency.

2. AgentRun command receipt must store command-scoped status and accepted refs. The receipt should not infer "same command already accepted" from `SessionMeta.last_turn_id`, because `last_turn_id` is only the latest turn for the runtime session and can change after subsequent messages, steering, auto-resume, or recovery terminal events.

3. `executor_session_id` must stay with RuntimeSession trace metadata for now. It is consumed by launch planning as `RuntimeTraceLaunchState.executor_session_id` to choose follow-up session and avoid repository rehydrate. Moving it to `AgentFrame` would mix connector transport continuation with capability/context surface. Moving it to `LifecycleAgent` would make one agent-level field ambiguous if multiple runtime traces exist over time.

4. `last_event_seq` must stay in RuntimeSession meta because `append_event` increments it atomically with event insert, and branch/rollback/projection code uses it as the session event-log head. AgentRun receipt can reference event seqs but should not own the event head.

5. `title` / `title_source` are the most product-facing legacy shape. They may remain on RuntimeSession for trace shell, but AgentRun workspace should introduce a display/title projection if the canonical page is run/agent-scoped. That projection can prefer user-set AgentRun title, source session title, or project agent metadata, but source-title validation still depends on `executor_session_id` in RuntimeSession meta.

6. `SessionMeta.bootstrap_state` is already gone from the current model; `LifecycleAgent.needs_bootstrap()` is the bootstrap authority. This is aligned with AgentRun/AgentFrame migration and should be preserved.

7. `AgentRunView.last_delivery_status` exists in contracts but is currently unpopulated. It is a better landing point for an AgentRun-level display projection than overloading `SessionMeta`: fill it from the agent's delivery runtime trace or latest command receipt when implementing AgentRun workspace, while keeping `SessionShellDto.last_delivery_status` as runtime trace status.

8. `SessionRuntimeControlView.session_meta` should remain for trace shell and event-stream continuity, but action enablement should gradually depend on explicit sources: runtime registry active turn, command receipt in-flight status, `LifecycleAgent.status`, current AgentFrame presence, and connector capabilities. The current code already partially does this by combining meta and runtime inspection.

9. Direct `save_session_meta` in `TurnCommitter` is a redundancy worth tightening. Accepted commit already persists `TurnStarted`, and event projection updates meta. Keeping the direct save is tolerable because of stale-event guards, but a cleaner accepted-boundary implementation would make event append the canonical state transition and reserve direct meta update for title-only changes.

10. `SessionMeta` should not absorb new command receipt fields such as `client_command_id`, request digest, accepted result, or duplicate conflict state. Those are command facts, not runtime trace facts.

### Suggested Implementation Entries

1. Add a dedicated user delivery command receipt, not a reuse of `session_runtime_commands`.

   Recommended shape:

   - Application port near session persistence or workflow delivery boundary, e.g. `AgentRunDeliveryCommandStore` / `SessionUserDeliveryCommandStore`.
   - PostgreSQL table with `scope`, `client_command_id`, `request_digest`, `status`, request refs, accepted refs, terminal error, timestamps.
   - Unique key on `(scope, client_command_id)`.
   - Statuses should distinguish at least `received`, `accepted`, `failed`; optionally `in_progress` if the implementation needs a claimed state.
   - Store result refs for both scopes: `agent_run_message` returns runtime_session_id/turn_id/run_id/agent_id/frame_id/frame_revision; `project_agent_start` additionally returns project_agent/session start refs.

2. Put the receipt boundary around the current use-case entries:

   - `AgentRunMessageService::dispatch_user_message` for existing session message.
   - `ProjectAgentSessionStartService::start_session` for first-message start.
   - API routes should pass `client_command_id` from DTO and return existing accepted result on duplicate same digest.

3. Treat `connector.prompt` success as accepted for receipt finalization:

   - The existing `SessionLaunchService::launch_command` only returns `turn_id` after commit/attach path completes enough to return from orchestrator.
   - If receipt needs an exact accepted marker, a lower-level hook or richer outcome around `ConnectorAcceptedTurn`/`TurnCommitter` may be needed.
   - Avoid marking receipt accepted before `ConnectorStarter::start` returns.

4. Review or relocate the pre-accepted initial AgentFrame write:

   - `SessionLaunchOrchestrator` writes a frame/current_frame before connector accepted.
   - If this is a true launch fact, move it behind accepted commit or make it explicitly compensatable on connector failure.
   - If it must remain pre-accepted for connector context, receipt recovery must not infer accepted delivery solely from latest frame/current_frame.

5. Refresh hook runtime target on subsequent turns:

   - In `SessionHookService::resolve_hook_runtime`, compare existing `control_target.frame_id` with current `resolve_runtime_hook_target(...)`.
   - If changed, reload instead of `refresh_from_provenance`.
   - Keep `validate_hook_runtime_target` semantics for explicit target callers.

6. Keep `SessionMeta` as a narrow RuntimeSession trace-head projection while adding AgentRun-level projections:

   - Do not put `client_command_id` or request digest on `SessionMeta`.
   - Keep `last_event_seq`, `executor_session_id`, and trace terminal summary on RuntimeSession.
   - Store command-scoped accepted result and retry/recovery state in the new receipt table.
   - Populate AgentRun display status from receipt/runtime trace projection instead of making `LifecycleAgent.status` mean delivery status.
   - Prefer event projection over direct meta mutation for turn start/terminal state; keep direct meta mutation for title/user shell edits.

### Implementation Risk Files

- `crates/agentdash-application/src/workflow/project_agent_session_start.rs` — high risk because lifecycle/runtime materialization happens before first message; idempotent retry must recover already-created refs.
- `crates/agentdash-application/src/workflow/agent_message.rs` — medium/high risk because it is the existing message dispatch use case and currently has no receipt/digest boundary.
- `crates/agentdash-application/src/session/launch/orchestrator.rs` — high risk because it defines accepted-stage ordering and currently mutates AgentFrame before accepted.
- `crates/agentdash-application/src/session/launch/connector_start.rs` — medium risk but clean accepted boundary.
- `crates/agentdash-application/src/session/launch/commit.rs` — medium risk because user input/turn started/runtime command applied are committed here.
- `crates/agentdash-application/src/session/core.rs` — medium risk because execution-state fallback and interrupted recovery depend on `SessionMeta`.
- `crates/agentdash-application/src/session/eventing.rs` — medium risk because `append_event` projection and source-title writes are the canonical meta update path.
- `crates/agentdash-application/src/session/hub_support.rs` — medium risk because it maps meta status to `SessionExecutionState`.
- `crates/agentdash-api/src/routes/sessions.rs` — medium risk because runtime-control actions and session shell DTO expose meta status.
- `crates/agentdash-application/src/session/hooks_service.rs` — medium risk for stale target refresh.
- `crates/agentdash-application/src/session/hub/hook_dispatch.rs` — medium risk for lazy rebuild and delivery-session adapter behavior.
- `crates/agentdash-spi/src/session_persistence.rs` — medium risk if adding a new session persistence port.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` — medium risk if receipt store is implemented alongside session persistence.
- `crates/agentdash-infrastructure/migrations/` — medium risk; new table migration needed if using dedicated receipts.
- `crates/agentdash-contracts/src/workflow.rs` and `crates/agentdash-contracts/src/project_agent.rs` — required DTO changes for `client_command_id`.
- `packages/app-web/src/generated/*` — generated outputs should follow contract generation, not manual edits.

### Suggested Verification Commands

- `cargo test -p agentdash-application workflow::agent_message`
- `cargo test -p agentdash-application workflow::project_agent_session_start`
- `cargo test -p agentdash-application session::launch`
- `cargo test -p agentdash-application session::hub`
- `cargo test -p agentdash-application session::core`
- `cargo test -p agentdash-application session::eventing`
- `cargo test -p agentdash-application session::runtime_control`
- `cargo test -p agentdash-infrastructure persistence::postgres::session_repository`
- `cargo test -p agentdash-spi session_persistence`
- `pnpm test -- lifecycle`
- For migration readiness after adding a table: run the existing backend migration/readiness test target used by the repo, or at minimum `cargo test -p agentdash-infrastructure migration`.

## Related Specs

- `.trellis/spec/backend/session/architecture.md` — states `RuntimeSession` is delivery/trace substrate, accepted stages, runtime anchor lookup, and runtime command outbox boundaries.
- `.trellis/spec/backend/session/session-startup-pipeline.md` — defines `LaunchCommand -> SessionConstructionPlan -> LaunchPlan -> PreparedTurn -> ConnectorAcceptedTurn -> CommittedTurn -> AttachedTurn`; says `connector.prompt` returning `ExecutionStream` is accepted boundary and accepted-after facts include user message, `TurnStarted`, context/capability projection event, runtime command applied, and title derivation.
- `.trellis/spec/backend/session/runtime-execution-state.md` — relevant for runtime registry / active turn / terminal effects / runtime command store behavior.
- `.trellis/spec/backend/hooks/architecture.md` — defines hook provider/runtime snapshot flow and turn-start consumption rule.
- `.trellis/spec/backend/hooks/execution-hook-runtime.md` — states `resolve_runtime_hook_target` maps runtime session to frame target and runtime context update enters turn-start queue.
- `.trellis/spec/backend/repository-pattern.md` — session runtime persistence does not go through `RepositorySet`; `SessionPersistence` and session runtime stores are SPI ports with infrastructure adapters.
- `.trellis/spec/backend/database-guidelines.md` — relevant for adding PostgreSQL migration and schema readiness checks.

## External References

- No external references were needed. Findings are based on local code, migrations, generated contracts, and Trellis specs.

## Caveats / Not Found

- `task.py current --source` returned no active task for this subagent session, so the explicitly supplied parent task path was used as the research output location.
- No existing durable user delivery receipt table was found.
- No existing `client_command_id` field was found in `AgentRunMessageRequest` or `CreateProjectAgentSessionRequest`.
- `session_runtime_commands` exists but is specialized for pending runtime context/frame transition delivery; it requires `frame_transition_id` and only has `RuntimeDeliveryCommandKind::PendingRuntimeContext`.
- Existing in-memory `pending_queue` is not a durable idempotency mechanism.
- Current tests cover basic dispatch and connector failure cleanup, but not duplicate client command recovery, digest conflict, or transport failure after accepted delivery.
- `SessionMeta` remains a legacy-shaped aggregate name, but its current surviving runtime fields mostly behave as denormalized RuntimeSession trace metadata. The unsafe part is not the fields themselves; it is using them as AgentRun command authority.
- `AgentRunView.last_delivery_status` is present but currently unpopulated, so AgentRun workspace status will need a new projection source.
