# Live Review Notes

## Frontend Findings

- `packages/app-web/src/App.tsx` lazy-loads `SessionPage` and registers `/session/new` plus `/session/:sessionId`.
- `packages/app-web/src/pages/SessionPage.tsx` derives draft state from query params when there is no runtime session id, then navigates to `/session/${response.runtime_session_id}` after Project Agent start.
- `SessionPage` sets `taskExecutorSummary = null`, and passes `agentDefaults={draftProjectAgent?.executor ?? taskExecutorSummary}` to `SessionChatView`.
- `SessionChatView` reads `agentDefaults` into `initialExecutorSource` with an empty dependency list.
- `useExecutorConfig` initializes from `initialSource` or global localStorage and `hydrate` only writes non-empty fields.
- `useExecutorDiscoveredOptions` returns early when executor is empty, so an empty executor state leaves the model selector with no models.
- `InlineModelSelector` is only disabled by readonly state. The observed “cannot select” symptom is consistent with missing discovered options rather than a disabled button.
- Existing runtime-control response already contains `run`, `agent`, `frame_runtime`, and `frame_runtime.execution_profile`; frontend does not project this into a workspace executor source.

## Backend Findings

- `AgentRunMessageRequest` and `CreateProjectAgentSessionRequest` have no `client_command_id`.
- `AgentRunMessageLaunchDeliveryPort` wraps all `launch_command` errors as `WorkflowApplicationError::Internal`, which loses `ConnectorError::InvalidConfig` mapping to HTTP 400.
- `ProjectAgentSessionStartService` materializes AgentRun/RuntimeSession before dispatching the first message; first-message failure cleanup is best-effort.
- `session_runtime_commands` exists, but it represents pending runtime context/frame transition commands and is not a user delivery idempotency receipt.
- `SessionLaunchOrchestrator` writes initial capability state to a new AgentFrame and updates `LifecycleAgent.current_frame_id` before turn preparation and connector start.
- `TurnCommitter` correctly writes user input and turn started after connector accepted; the early frame write is the boundary violation.
- `PiAgentConnector` already returns `ConnectorError::InvalidConfig` for missing model selection under dynamic provider catalog.
- `ensure_hook_runtime_for_delivery_session` returns cached HookRuntime without checking current HookControlTarget. The outer target validation then throws `Hook runtime target mismatch` when the cached frame differs from current frame.

## Planning Impact

- The fix must include route/page conceptual migration, not only a model selector patch.
- The canonical workspace route should use run_id + agent_id, with RuntimeSession resolved as delivery ref.
- Backend command idempotency must be durable because transport failure can occur after server acceptance.
- Launch accepted boundary and HookRuntime target refresh are independent backend corrections and should not wait for frontend route migration.
- Current specs still describe AgentRun delivery/control commands as session-scoped `/sessions/{runtime_session_id}/...` routes in `backend/session/runtime-execution-state.md`; final implementation should update the spec to the AgentRun Workspace public identity after API contract lands.
- Cross-layer contract rules require new workspace/command DTOs in `agentdash-contracts` with generated TypeScript output; frontend should consume generated DTOs directly, not hand-write long-lived wire aliases.
- `SessionMeta` should remain a RuntimeSession trace-head projection for `last_event_seq`, `executor_session_id`, terminal trace summary and source trace title; it should not receive `client_command_id`, request digest, retry/conflict state, or AgentRun workspace shell fields.
- `AgentRunView.last_delivery_status` already exists but is currently unpopulated; AgentRun Workspace/list status should use an AgentRun-facing delivery summary or command receipt projection rather than `ProjectSessionListEntry.delivery_status`.
- `ProjectSessionListEntry` and `SessionShortcutList` are current workspace-entry residues: they use `runtime_session_id`, `SessionMeta.title`, and `delivery_status` for sidebar navigation to `/session/:id`.
- `TurnCommitter` directly saves `SessionMeta` as running after accepted commit while event projection also updates meta from `TurnStarted`; this is tolerable for current trace-head behavior but should not become command receipt authority.
