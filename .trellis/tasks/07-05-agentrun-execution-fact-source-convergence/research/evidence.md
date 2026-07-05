# Evidence

## Runtime State Split

- `crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:225` reads `inspect_session_execution_state(runtime_session_id)`.
- `crates/agentdash-application-agentrun/src/agent_run/presentation_read_model.rs:227` computes `delivery_running` as `session_meta.last_delivery_status == ExecutionStatus::Running || matches!(execution_state, SessionExecutionState::Running { .. })`.
- This allows stale session metadata to mark runtime control as running even when the actual inspected execution state is terminal, interrupted, or idle.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:161` resolves workspace execution state through `inspect_session_execution_state`.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:70` derives delivery status directly from `SessionExecutionState`.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:63` checks command availability from inspected execution state.
- `crates/agentdash-application-runtime-session/src/session/hub_support.rs:483` maps `SessionMeta.last_delivery_status == Running` without an in-memory runtime entry to `SessionExecutionState::Interrupted`, so runtime-session core already distinguishes stale summary from active running.
- `crates/agentdash-application-runtime-session/src/session/core.rs:34` and `runtime_control.rs:137` use `last_delivery_status == Running` for startup recovery. That usage is internal recovery metadata and should not be treated as a public active-state source.
- `crates/agentdash-api/src/routes/sessions.rs:156` loads runtime control through `presentation_read_model_query.session_runtime_control`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1275` AgentRun scoped runtime stream resolves the AgentRun delivery RuntimeSession, while `lifecycle_agents.rs:1119` AgentRun scoped runtime control delegates to `sessions::load_session_runtime_control_view`. Therefore stale presentation read model state can surface under the AgentRun scoped route.
- Architectural implication: the AgentRun scoped route should not expose a RuntimeSession-owned control plane as user-facing state. RuntimeSession can provide execution inspection and trace diagnostics, but AgentRun must own the public state snapshot.

## Composer Helper Path

- `packages/app-web/src/features/agent-run-workspace/model/conversationCommandState.ts:255` stores `conversation.execution.reason` as `helperText`.
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:693` selects `submitCommand.unavailable_reason` only when submit is disabled.
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:697` uses `commandState.helperText` when submit is enabled.
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:690` allows submit during `RunningActive`; this is intended because new user input goes through mailbox. Thus a visible "正在执行中" helper is not proof that submit is disabled.
- `packages/app-web/src/features/session/ui/SessionChatView.tsx:374` calls `onTurnEnd` when durable terminal events are observed.
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:69` plans workspace refresh for turn end.
- If the helper remains running after terminal, the failure is either missed terminal side effect, stale backend read model, or workspace refresh race.

## Feed And Cursor Path

- `packages/app-web/src/features/session/model/useSessionStream.ts:234` fetches AgentRun conversation feed when an AgentRun stream target resets.
- `packages/app-web/src/features/session/model/useSessionStream.ts:242` sets `lastAppliedSeq` to `runtime_replay_start_seq`.
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts:662` drops durable events whose `event_seq <= lastAppliedSeq`.
- `crates/agentdash-application-agentrun/src/agent_run/conversation_feed.rs:143` currently returns `runtime_replay_start_seq: 0`.
- `packages/app-web/src/features/session/model/agentRunConversationFeed.ts:354` assigns synthetic event seqs to feed events from `feed.head_event_seq`, then reduces them into ordinary display entries.
- The stream hook resets durable `lastAppliedSeq` to `runtime_replay_start_seq`, so durable cursor is currently protected, but feed entries still look like normal stream entries to downstream aggregation.

## Fork Display Seed Path

- `packages/app-web/src/features/session/model/roundActions.ts:36` builds fork point from the completed round final agent reply `MessageRef`.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:145` sends that `fork_point_ref` to the AgentRun fork API.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:330` resolves message-ref fork points to a parent event seq and requires the referenced turn to be completed.
- `crates/agentdash-application-runtime-session/src/session/branching.rs:467` materializes child initial projection from `parent_context.messages`.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs:439` builds that parent context via `ContextProjector::build_model_context`.
- `ContextProjector` is a model-context projection path. It may restore active compaction segments and suffix events, and for fork initial projection `branching.rs:489` stores `base_head_event_seq: Some(0)` with child projection head `head_event_seq: 1`.
- This can be correct for model launch context, but it is not an explicit guarantee of UI-visible parent transcript history. The fork display bug should be tested against compaction/projection cases and simple raw-history cases separately.

## Thinking Indicator Path

- `crates/agentdash-application-runtime-session/src/session/eventing.rs:1540` classifies `ProviderAttemptStatus` as ephemeral; `eventing.rs:2646` tests it as live-only.
- `crates/agentdash-api/src/routes/sessions.rs:1016` sends durable backlog, then connected with `ephemeral_epoch`, then ephemeral backlog.
- `packages/app-web/src/features/session/model/streamTransport.ts:188` receives the epoch and `useSessionStream.ts:286` resets the ephemeral cursor on epoch change.
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts:189` updates provider waiting state from `provider_attempt_status`.
- `packages/app-web/src/features/session/model/sessionStreamReducer.ts:646` keeps ephemeral events on a separate seq lane from durable events.
- `packages/app-web/src/features/session/ui/SessionMessageCard.tsx:79` renders provider waiting as “正在思考”.
- Provider waiting failure should therefore be checked as a stream/identity/ephemeral delivery issue, not patched by static UI state.

## Planning Implication

The minimal correct fix is not a UI-only patch. The architecture needs one AgentRun execution snapshot for public state, an explicit fork display-history seed separate from child runtime durable events, verified terminal-triggered workspace refresh, and a final cleanup pass for stale status references.

## AgentRun Frontend Exposure Audit Targets

- `packages/app-web/src/services/agentRunRuntime.ts:63` exposes `fetchAgentRunRuntimeControl` as `Promise<SessionRuntimeControlView>` for an AgentRun scoped path. This couples AgentRun frontend runtime state to the generic RuntimeSession control contract.
- `packages/app-web/src/services/lifecycle.ts:75` exposes a second `fetchAgentRunRuntimeControl` wrapper that returns the same `SessionRuntimeControlView`.
- `packages/app-web/src/generated/workflow-contracts.ts:279` defines `SessionRuntimeControlView` with `runtime_session_ref`, `session_meta`, and `control_plane`; `:281` defines `SessionShellDto.last_delivery_status`. Generated contracts reflect backend exposure and should be regenerated after source DTO cleanup.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:508` maps `workspaceControl.delivery_trace_meta` into `workspaceRuntimeData.sessionMeta`, and `:516` copies `delivery_status` into `last_delivery_status`. This can reintroduce RuntimeSession-derived status into AgentRun workspace UI even after backend command policy is fixed.
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:140` derives `deliveryTraceSessionId` from `workspace.delivery_trace_meta.runtime_session_ref.runtime_session_id`. This is acceptable only as a stream target / trace handle and should not feed public status.
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:96` reads generic session runtime control through `fetchSessionRuntimeControl`. The implementation needs to keep this as a diagnostic-only surface when used outside AgentRun, and prevent it from becoming an AgentRun state dependency.
- `crates/agentdash-contracts/src/runtime/workflow.rs:1611` defines `SessionRuntimeControlView`; `:1706` still exposes `delivery_trace_meta` on `AgentRunWorkspaceView`. These source contracts are the backend cleanup anchors.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs:1156` returns `SessionRuntimeControlView` for the AgentRun scoped runtime-control handler, and `:1172` delegates to `sessions::load_session_runtime_control_view`. This is the backend API boundary that should become an AgentRun snapshot projection or diagnostic wrapper.
