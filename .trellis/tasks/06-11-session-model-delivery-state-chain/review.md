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

## Final Review Gate

### Scope Closure

- Interaction workspace routes are `/agent-runs/new` and `/agent-runs/:runId/:agentId`.
- `SessionPage`, `SessionShortcutList`, `active-session-list`, `ProjectSessionList*`, and project-scoped `/projects/{id}/sessions` workspace list are removed from the public workspace chain.
- Project workspace list uses `AgentRunWorkspaceListView` from `/projects/{project_id}/agent-runs`; list title/status/activity come from AgentRun workspace shell.
- AgentRun message, steering, pending queue and cancel public commands are scoped by `run_id + agent_id`.
- Session-scoped public command routes `/sessions/{runtime_session_id}/messages`, `/steering`, `/pending-messages`, and `/cancel` are removed.
- ProjectAgent draft start public command uses `/projects/{project_id}/agents/{project_agent_id}/agent-runs` and `ProjectAgentRunStart*` contracts.
- Frontend cancel control uses AgentRun command identity through `/agent-runs/{run_id}/agents/{agent_id}/cancel`.
- `session_meta_updated` no longer overwrites AgentRun workspace title; it only refreshes AgentRun workspace projection.
- `SessionMeta` and `SessionRuntimeControlView` remain only for RuntimeSession trace/control inspection, feed rendering, debug/recovery, and delivery-runtime internals.

### Failure Chain Coverage

- `client_command_id` is required on ProjectAgent start and AgentRun message/steering/pending commands.
- Durable AgentRun delivery command receipts cover duplicate accept, digest conflict, and terminal failure retry.
- AgentRun message command digest is stable across accepted frame advancement, and accepted refs record the post-accept current AgentFrame.
- Frontend keeps an in-flight command id for the same submitted payload so transport retry does not become a new user command.
- AgentFrame construction produces a pending frame/envelope; persistence and current-frame advancement happen at accepted commit boundary.
- HookRuntime cache refresh checks the current `HookControlTarget` and replaces stale cached runtimes instead of surfacing normal frame transitions as target mismatch.

### Verification Evidence

- `pnpm run contracts:check`
- `pnpm --filter app-web run typecheck`
- `pnpm --filter app-web run lint`
- `pnpm --filter app-web exec vitest run src/features/agent/agent-tab-view.test.ts src/services/lifecycle.test.ts src/features/agent-run-workspace/model/workspaceCommandState.test.ts`
- `cargo check -p agentdash-application -p agentdash-api -p agentdash-contracts`
- `cargo clippy -p agentdash-application -p agentdash-api -p agentdash-contracts -- -D warnings -A clippy::uninlined_format_args -A clippy::too_many_arguments -A clippy::type_complexity -A clippy::large_enum_variant`
- `cargo test -p agentdash-application current_frame -- --nocapture`
- `cargo test -p agentdash-application connector_setup_failure_leaves_current_frame_unchanged -- --nocapture`
- `cargo test -p agentdash-application accepted_turn_commits_agent_frame_revision_and_current_frame -- --nocapture`
- `cargo test -p agentdash-application planner_invalid_config_leaves_current_frame_unchanged -- --nocapture`
- `cargo test -p agentdash-application dispatch_records_frame_after_delivery_acceptance -- --nocapture`
- `cargo test -p agentdash-application duplicate_dispatch_returns_existing_receipt_without_delivery -- --nocapture`
- `cargo test -p agentdash-application hook_runtime_target_switch_replaces_stale_cached_runtime -- --nocapture`
- `pnpm run migration:guard`
- `python ./.trellis/scripts/task.py validate ./.trellis/tasks/06-11-session-model-delivery-state-chain`
- `python ./.trellis/scripts/task.py validate` for all five child tasks
- Residue scans cover removed workspace route/page/list and removed session-scoped public command routes.
- Final review gate sub-agent reported no blockers after rechecking ProjectAgent AgentRun route, session cancel removal, post-accept frame receipt refs, and pending-frame construction boundary.

### Remaining Session Concepts

The remaining Session/RuntimeSession names are intentional runtime substrate terms:

- event feed and `session_meta_update` rendering;
- runtime trace projection and trace permission lookup;
- connector/relay/executor live session protocol;
- local runtime session control internals used behind AgentRun command routes.

These do not act as user-facing workspace identity or public command scope.
