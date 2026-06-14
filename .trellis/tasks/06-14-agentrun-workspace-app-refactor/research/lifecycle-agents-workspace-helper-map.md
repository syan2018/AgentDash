# Research: lifecycle_agents workspace helper map

- Query: 映射 `crates/agentdash-api/src/routes/lifecycle_agents.rs` 中 AgentRun workspace projection / command policy 相关旧 helper，标注 Phase 3 query service 与 Phase 4 command policy 的迁移方向和验收 grep 关键词。
- Scope: internal
- Date: 2026-06-14

## Findings

### Files Found

- `.trellis/workflow.md` - Trellis 流程要求 research 输出必须持久化，in-progress 阶段 subagent 不再派发其它 implement/check agent。
- `.trellis/tasks/06-14-agentrun-workspace-app-refactor/prd.md` - 任务目标要求 API 只保留鉴权、HTTP 参数解析、contract DTO mapping 与错误映射。
- `.trellis/tasks/06-14-agentrun-workspace-app-refactor/design.md` - 设计目标明确新增 `AgentRunWorkspaceQueryService` 与 `AgentRunWorkspaceCommandPolicy`，API 映射 application read model / conflict。
- `.trellis/tasks/06-14-agentrun-workspace-app-refactor/implement.md` - Phase 3 下沉 query assembly，Phase 4 下沉 stale guard / command availability / replacement command。
- `.trellis/spec/backend/architecture.md` - 后端分层 invariant：API 层负责鉴权、请求/响应 DTO 和错误映射；业务编排进入 application 层。
- `.trellis/spec/backend/error-handling.md` - application 到 API 保留结构化错误语义，API 负责 HTTP 状态码与响应体映射。
- `.trellis/spec/backend/session/runtime-execution-state.md` - AgentRun workspace shell / command surface 由 AgentRun 控制面事实投影生成，RuntimeSession trace metadata 只作为引用。
- `.trellis/spec/backend/session/agentrun-mailbox.md` - composer submit / mailbox command 走 command receipt -> mailbox envelope -> scheduler outcome；route-local 分支不是权威路径。
- `.trellis/spec/backend/workflow/architecture.md` - AgentRun conversation snapshot 以 run / agent / current frame / delivery anchor / runtime execution state / mailbox / model config / resource surface 生成 command view。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - Rust contract DTO 与 generated TypeScript 是 wire source，API route 负责把 application/domain model 映射为 contract DTO。
- `.trellis/spec/frontend/state-management.md` - 前端命令 authority 来自后端 `AgentConversationSnapshot.commands` / stale guard；控制命令携带 stale guard，由后端精确校验。
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - 当前旧路径集中点，route 内联 workspace view assembly、control/actions 派生、stale guard、command availability 和 conflict response。
- `crates/agentdash-application/src/workflow/conversation_snapshot.rs` - application 已有 conversation snapshot、command id、stale guard 和 snapshot id 生成逻辑。
- `crates/agentdash-application/src/workflow/agent_run_mailbox.rs` - application 已有 mailbox command receipt、message policy、scheduler outcome 和 steering 支持检查。
- `crates/agentdash-application/src/workflow/mod.rs` - 当前未导出 `agent_run_workspace` 模块；`conversation_snapshot` 和 `agent_run_mailbox` 已导出。
- `crates/agentdash-application/src/session/types.rs` - `SessionExecutionState` 枚举是 workspace projection / command policy 的核心输入。
- `crates/agentdash-contracts/src/workflow.rs` - `AgentRunWorkspaceView`、conversation command、precondition、runtime command state 等 wire DTO 定义。
- `crates/agentdash-api/src/routes/sessions.rs` - RuntimeSession detail route 也复用部分 mailbox mapper，但它是 runtime-session identity 入口，不应误删为 AgentRun workspace 旧路径。
- `crates/agentdash-api/src/rpc.rs` - `ApiError::ConflictWithCode` 与 `ApiErrorWithCode` 是 HTTP error response mapper。

### Current API Helper Inventory

#### Context / query assembly

- `AgentRunContext` stores `run`, `agent`, `delivery_runtime_session_id` in API (`lifecycle_agents.rs:53`).
- `resolve_agent_run_context` parses route IDs, loads project permission, checks agent/run ownership, and resolves delivery runtime (`lifecycle_agents.rs:568`).
- `delivery_runtime_session_for_agent_run` scans execution anchors and chooses latest anchor by `updated_at` (`lifecycle_agents.rs:606`).
- `build_agent_run_workspace_view` assembles the whole `AgentRunWorkspaceView`: meta, frame/VFS, lifecycle run view, subject associations, execution state, supports steering, control plane, actions, mailbox state/messages, model config, resource diagnostics, conversation snapshot, shell title/status and DTO refs (`lifecycle_agents.rs:624`).
- `resolve_workspace_title` chooses ProjectAgent name fallback or `AgentRun {agent_id}` (`lifecycle_agents.rs:924`).
- `select_workspace_shell_title` chooses delivery `SessionMeta` title/source over workspace fallback (`lifecycle_agents.rs:941`).
- `agent_run_workspace_list_entry` derives list item fields from a full workspace contract view (`lifecycle_agents.rs:972`).

Migration direction:

- Move business assembly to Phase 3 `AgentRunWorkspaceQueryService`.
- API should still parse path/current user and enforce Project permission before query, but latest delivery anchor, current frame/runtime surface, execution state, mailbox facts, model facts, resource diagnostics, shell title/source and list entry projection should be application-owned read model facts.
- API mapper can convert application refs/title/source/status fields to `AgentRunWorkspaceView` / `AgentRunWorkspaceListEntry` contract DTOs.

#### Control plane / action projection

- `build_agent_run_workspace_view` derives `delivery_running`, `delivery_running_active`, `delivery_starting_claimed`, `delivery_cancelling`, `terminal_agent`, `has_frame`, `has_delivery_runtime`, and `supports_steering` inside the route (`lifecycle_agents.rs:694`).
- `control_plane` is selected from terminal agent, missing delivery runtime, missing frame, cancelling, running/starting, ready (`lifecycle_agents.rs:723`).
- `actions` enables/disables `submit_message` and `cancel` based on delivery runtime, frame, terminal agent, running/cancelling state (`lifecycle_agents.rs:758`).
- `enabled_action` / `disabled_action` construct contract availability DTOs (`lifecycle_agents.rs:1327`).
- `is_terminal_agent_status` is a route-local string helper (`lifecycle_agents.rs:1875`), duplicated conceptually in application mailbox (`agent_run_mailbox.rs:1891`).

Migration direction:

- Move availability semantics to Phase 3 application projection (`AgentRunWorkspaceProjection`), returning an application action set/control model.
- API may keep a mapper equivalent to `enabled_action` / `disabled_action` only if it maps an application availability model to contract DTO, not if it decides availability.
- Terminal-agent status classification should be an application/domain helper used by projection and policy, not a route-local string match.

#### State code / delivery status / turn id

- `conversation_state_code` maps `SessionExecutionState` to `ready | starting_claimed | running_active | cancelling | completed | failed | interrupted` for conflict detail (`lifecycle_agents.rs:1844`).
- `workspace_delivery_status` maps `SessionExecutionState` plus terminal agent status to shell `delivery_status` (`lifecycle_agents.rs:1879`).
- `execution_state_turn_id` maps running/cancelling/interrupted/completed/failed turn ids to shell `last_turn_id` (`lifecycle_agents.rs:1908`).
- `execution_state_active_turn_id` maps only running/cancelling turn ids to active turn id for command guard detail (`lifecycle_agents.rs:1856`).
- Application `conversation_snapshot.rs` already has related code: `execution_state_snapshot_code` (`conversation_snapshot.rs:570`) and `active_turn_id` (`conversation_snapshot.rs:635`).

Migration direction:

- Move these derivations to application projection/policy types.
- API should not match on `SessionExecutionState` to decide workspace shell/control/command state.
- Risk: `execution_state_active_turn_id` in API omits completed/failed turn ids, while application `conversation_snapshot` stale guard currently includes completed/failed turn ids via `active_turn_id`. Phase 4 should intentionally align "active turn" vs "last visible turn" semantics so stale guard does not reject a command generated by the same snapshot shape.

#### Runtime command state

- `runtime_command_state_dto` maps `SessionExecutionState` to `RuntimeSessionCommandStateDto` status / turn / message (`lifecycle_agents.rs:1411`).
- `agent_run_message_command_response` uses it for mailbox command responses (`lifecycle_agents.rs:1128`).
- `AgentRunMailboxCommandResult` currently returns `runtime_state: Option<SessionExecutionState>` from application (`agent_run_mailbox.rs:90`).
- Contract DTO is `RuntimeSessionCommandStateDto` (`workflow.rs:1271`).

Migration direction:

- The current `SessionExecutionState` -> command status/message derivation should move to an application model such as `AgentRunWorkspaceCommandState`.
- API can keep a narrow mapper from application command state model to `RuntimeSessionCommandStateDto`.
- If mailbox service continues returning `SessionExecutionState` temporarily, this helper is still an API mapper, but it remains an old business derivation to remove during Phase 3/4 convergence.

#### Mailbox DTO projection adjacent to workspace

- `mailbox_message_view` maps domain `AgentRunMailboxMessage` to `MailboxMessageView` and derives `can_delete`, `can_promote`, `can_reorder`, `can_recall` (`lifecycle_agents.rs:1001`).
- `mailbox_message_visible` hides dispatched/steered/deleted messages (`lifecycle_agents.rs:1044`).
- `mailbox_state_view` derives paused/can_resume/user preference state (`lifecycle_agents.rs:1055`).
- `build_agent_run_mailbox_view` separately assembles mailbox route response (`lifecycle_agents.rs:1071`).
- `sessions.rs` imports `mailbox_message_view`, `mailbox_message_visible`, and `mailbox_state_view` for RuntimeSession detail (`sessions.rs:17`, `sessions.rs:295`).

Migration direction:

- For AgentRun workspace Phase 3, mailbox visibility/can_* facts are application projection semantics and should be produced by query service or mailbox projection model.
- Pure enum/field conversion to contract can stay in API mapper.
- Because `sessions.rs` reuses these functions for RuntimeSession detail, deleting or moving them requires either a shared application mapper/model or a separate runtime-control mapper. Do not treat all references as AgentRun workspace old path without checking the route owner.

#### Resource diagnostics / model config / conversation snapshot input

- `workspace_resource_diagnostics` and `lifecycle_resource_surface_diagnostics` check for `lifecycle_vfs` mount and emit `ConversationDiagnosticView` (`lifecycle_agents.rs:891`).
- `build_agent_run_workspace_view` assembles `ConversationModelConfigInput` and calls application `ConversationModelConfigResolver` (`lifecycle_agents.rs:807`, `lifecycle_agents.rs:823`).
- It then assembles `AgentConversationSnapshotInput` and calls `AgentConversationSnapshotResolver` (`lifecycle_agents.rs:831`).
- `conversation_snapshot.rs` already derives execution status, command availability, keyboard map, command ids, stale guard and snapshot id in application (`conversation_snapshot.rs:269`, `conversation_snapshot.rs:388`, `conversation_snapshot.rs:510`, `conversation_snapshot.rs:551`).

Migration direction:

- Phase 3 query service should own input assembly and resource diagnostic facts.
- Existing `AgentConversationSnapshotResolver` can remain application-owned; API should stop constructing its input directly.
- Contract DTO mapping can remain in API if application model is not already contract-shaped.

### Phase 4 Command Policy Inventory

#### Route-local command precondition enum

- `AgentRunCommandPrecondition` has variants for delete, promote, resume, cancel and stores `AgentRunCommandPreconditionView` (`lifecycle_agents.rs:1464`).
- Its `expected_kind` maps variants to `ConversationCommandKind` (`lifecycle_agents.rs:1619`).
- Its `command_precondition` returns the submitted contract precondition (`lifecycle_agents.rs:1635`).

Migration direction:

- Replace with application policy input, e.g. `AgentRunWorkspaceCommandPolicyInput { command_kind, submitted_precondition, run_id, agent_id, runtime_session_id, frame_ref }`.
- API can keep route-specific mapping from endpoint to intended command kind, but not the policy enum that owns business semantics.

#### Stale guard validation

- `ensure_agent_run_command_allowed` reads current execution state, resolves current frame, computes terminal agent and state code, then calls stale guard validation (`lifecycle_agents.rs:1479`).
- `ensure_command_submission_matches_snapshot` validates expected command kind/id, run/agent identity, runtime session id, frame id, active turn id and snapshot id (`lifecycle_agents.rs:1645`).
- It recomputes current snapshot id using application `conversation_snapshot_id` (`lifecycle_agents.rs:1656`).
- `stale_command_conflict` constructs stale conflict response with replacement command (`lifecycle_agents.rs:1740`).
- `ensure_composer_command_precondition_matches_agent_run` does a special, looser composer validation: run/agent identity and submit-message kind only (`lifecycle_agents.rs:1753`).

Migration direction:

- Phase 4 application command policy should own stale guard validation and the exact difference between composer submit and non-text control commands.
- API should pass the submitted contract precondition into policy and map `AgentRunWorkspaceCommandConflict` to `ApiError::ConflictWithCode`.
- Composer policy should stay aligned with frontend spec: text input expresses "submit this user input" and scheduler decides current mailbox behavior, while non-text control commands bind to precise snapshot refs.

#### Command availability checks

- Terminal-agent command block allows cancel/delete but blocks promote/resume (`lifecycle_agents.rs:1517`).
- Promote requires `Running { turn_id: Some(_) }`, rejects starting-claimed, rejects non-running, and requires connector steering support (`lifecycle_agents.rs:1532`).
- Resume requires mailbox state paused and visible message count > 0 (`lifecycle_agents.rs:1569`).
- Cancel requires running or cancelling execution state (`lifecycle_agents.rs:1605`).
- Delete currently allows after stale guard passes (`lifecycle_agents.rs:1533`).
- Composer precondition only rejects non-submit command intent and run/agent mismatch (`lifecycle_agents.rs:1771`, `lifecycle_agents.rs:1790`).

Migration direction:

- Move to `AgentRunWorkspaceCommandPolicy`.
- Policy should return structured conflict with stable `error_code`, optional `replacement_command`, and `detail`.
- API maps that conflict to `ApiErrorWithCode`; it should not inspect execution state/mailbox state/steering support itself.

#### Replacement command and error mapping

- `replacement_command_for_state` currently returns `Some("submit_message")` for every non-terminal execution state (`lifecycle_agents.rs:1802`).
- `command_id_for_kind` duplicates application `conversation_snapshot.rs` command ids (`lifecycle_agents.rs:1819`, `conversation_snapshot.rs:582`).
- `command_conflict` builds `ApiError::ConflictWithCode` (`lifecycle_agents.rs:1830`).
- `ApiErrorWithCode` is the HTTP response shape (`rpc.rs:24`).

Migration direction:

- Replacement command selection and error code/detail should be application policy output.
- `command_conflict` or equivalent should remain API mapper only: `AgentRunWorkspaceCommandConflict -> ApiError::ConflictWithCode`.
- `command_id_for_kind` should not remain duplicated in API after policy migration. Reuse an application command id helper or compare the submitted command id against application-generated command model.

### Phase Grouping Summary

Phase 3 query service / projection should replace:

- `build_agent_run_workspace_view` business assembly (`lifecycle_agents.rs:624`).
- `delivery_runtime_session_for_agent_run` when used as workspace fact resolution (`lifecycle_agents.rs:606`).
- `control_plane` branch and reason strings (`lifecycle_agents.rs:723`).
- `actions` availability branch and reason strings (`lifecycle_agents.rs:758`).
- `workspace_delivery_status` (`lifecycle_agents.rs:1879`).
- `execution_state_turn_id` for shell last visible turn (`lifecycle_agents.rs:1908`).
- `conversation_state_code` if used by workspace read model / conflict model (`lifecycle_agents.rs:1844`).
- `runtime_command_state_dto`'s state derivation, while keeping final DTO mapping in API (`lifecycle_agents.rs:1411`).
- `workspace_resource_diagnostics` / `lifecycle_resource_surface_diagnostics` (`lifecycle_agents.rs:891`).
- mailbox visibility and can_* derivation if query service returns workspace mailbox projection (`lifecycle_agents.rs:1001`, `lifecycle_agents.rs:1044`, `lifecycle_agents.rs:1055`).
- workspace title fallback/selection semantics (`lifecycle_agents.rs:924`, `lifecycle_agents.rs:941`).
- list entry projection semantics (`lifecycle_agents.rs:972`).

Phase 4 command policy should replace:

- `AgentRunCommandPrecondition` route-local enum (`lifecycle_agents.rs:1464`).
- `ensure_agent_run_command_allowed` (`lifecycle_agents.rs:1479`).
- `ensure_command_submission_matches_snapshot` (`lifecycle_agents.rs:1645`).
- `stale_command_conflict` policy payload generation (`lifecycle_agents.rs:1740`).
- `ensure_composer_command_precondition_matches_agent_run` (`lifecycle_agents.rs:1753`).
- `replacement_command_for_state` (`lifecycle_agents.rs:1802`).
- `command_id_for_kind` duplicate command id mapping (`lifecycle_agents.rs:1819`).
- `execution_state_active_turn_id` as stale-guard current fact helper (`lifecycle_agents.rs:1856`).

API mapper should still own:

- `router` endpoint table (`lifecycle_agents.rs:70`).
- route path/body extraction and `parse_uuid` (`lifecycle_agents.rs:1460`).
- current user and Project permission loading before calling application service (`lifecycle_agents.rs:114`, `lifecycle_agents.rs:167`, `lifecycle_agents.rs:568`), unless a broader authorization service is introduced separately.
- contract DTO conversion helpers such as `agent_run_message_command_response`, mailbox enum/status/source/delivery/barrier/drain conversions, accepted refs mapping, receipt mapping and runtime trace meta mapping (`lifecycle_agents.rs:1128`, `lifecycle_agents.rs:1140`, `lifecycle_agents.rs:1154`, `lifecycle_agents.rs:1179`, `lifecycle_agents.rs:1229`, `lifecycle_agents.rs:1280`, `lifecycle_agents.rs:1303`, `lifecycle_agents.rs:1369`, `lifecycle_agents.rs:956`).
- `command_conflict` only as HTTP mapper from application conflict to `ApiError::ConflictWithCode`; it should no longer decide message/error_code/replacement/detail (`lifecycle_agents.rs:1830`).

### Grep Checks For Final Acceptance

High-signal old-path checks in `lifecycle_agents.rs`:

```powershell
rg -n "build_agent_run_workspace_view|ensure_agent_run_command_allowed|ensure_command_submission_matches_snapshot|ensure_composer_command_precondition_matches_agent_run|AgentRunCommandPrecondition|stale_command_conflict|replacement_command_for_state|conversation_state_code|workspace_delivery_status|execution_state_turn_id|execution_state_active_turn_id|runtime_command_state_dto" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Projection leakage checks:

```powershell
rg -n "SessionExecutionState::(Idle|Running|Cancelling|Completed|Failed|Interrupted)|inspect_session_execution_state|supports_session_steering|conversation_snapshot_id|AgentConversationSnapshotResolver|ConversationModelConfigResolver" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Command policy leakage checks:

```powershell
rg -n "stale_command|command_unavailable|starting_claimed|connector_steer_unsupported|active_turn_mismatch|snapshot_id_mismatch|runtime_session_mismatch|frame_mismatch|agent_run_identity_mismatch|submitted_guard|replacement_command" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

DTO-only mapper caveat checks:

```powershell
rg -n "ApiErrorWithCode|ConflictWithCode|RuntimeSessionCommandStateDto|AgentRunWorkspace(ControlPlane|Action|Shell|View|ListEntry)|MailboxMessageView|MailboxStateView" crates/agentdash-api/src/routes/lifecycle_agents.rs
```

Expected after migration:

- The first grep should be empty or contain only renamed mapper functions whose inputs are application read models, not `SessionExecutionState`.
- The second grep should be empty for lifecycle workspace route business logic; if present, verify it is only endpoint glue and not projection/policy.
- The third grep should be empty from API policy branches; stable error codes/details should originate in application policy.
- The fourth grep can still match API DTO mappers. Review each match to ensure no state/policy branch remains.

Broader duplicate checks:

```powershell
rg -n "command_id_for_kind|command_id_for\\(|submit_message|promote_mailbox_message|delete_mailbox_message|resume_mailbox|cancel" crates/agentdash-api/src/routes crates/agentdash-application/src/workflow
rg -n "is_terminal_agent_status\\(|matches!\\(status, \"completed\" \\| \"failed\" \\| \"cancelled\"\\)" crates/agentdash-api/src crates/agentdash-application/src crates/agentdash-domain/src
rg -n "mailbox_message_visible|mailbox_state_view|mailbox_message_view" crates/agentdash-api/src crates/agentdash-application/src
```

Use the broader checks to identify duplicate helper definitions, but do not blindly remove `sessions.rs` runtime-control mappers because that route has a different identity origin.

## Code Patterns

- API currently enters workspace list by looping runs/agents, resolving latest anchor and building full workspace view for each entry (`lifecycle_agents.rs:128`, `lifecycle_agents.rs:143`, `lifecycle_agents.rs:155`). This is a query service candidate; repeated per-agent assembly may remain expensive if left in API.
- API workspace detail route simply calls `resolve_agent_run_context` then `build_agent_run_workspace_view` (`lifecycle_agents.rs:167`). This is the cleanest Phase 3 integration point.
- API command routes all resolve context, extract delivery runtime, call route-local precondition policy, then call mailbox/cancel service (`lifecycle_agents.rs:271`, `lifecycle_agents.rs:314`, `lifecycle_agents.rs:359`, `lifecycle_agents.rs:463`). This is the cleanest Phase 4 integration point.
- Application conversation snapshot already computes command views and stale guards from a single input (`conversation_snapshot.rs:269`, `conversation_snapshot.rs:388`, `conversation_snapshot.rs:510`), so Phase 4 policy should validate against the same application facts, not reconstruct a parallel snapshot in API.
- Application mailbox service already owns user-message policy and scheduler outcome (`agent_run_mailbox.rs:147`, `agent_run_mailbox.rs:203`, `agent_run_mailbox.rs:1652`). API should not add route-local launch/queue/steer decisions.
- Contract DTOs are wire types in `agentdash-contracts::workflow` (`workflow.rs:769`, `workflow.rs:904`, `workflow.rs:1221`, `workflow.rs:1271`). Application read models should not become `ApiError` or `axum::Json` aware.

## External References

- No external references used. This research is based on repository code and Trellis specs only.

## Related Specs

- `.trellis/spec/backend/architecture.md`: API/application/domain layering.
- `.trellis/spec/backend/error-handling.md`: structured application errors and API HTTP mapping.
- `.trellis/spec/backend/session/runtime-execution-state.md`: AgentRun workspace shell/control/action facts vs RuntimeSession trace metadata.
- `.trellis/spec/backend/session/agentrun-mailbox.md`: mailbox command/scheduler authority and route-local anti-pattern.
- `.trellis/spec/backend/workflow/architecture.md`: Lifecycle/AgentFrame/RuntimeSession ownership and conversation snapshot facts.
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: contract DTO/generation source of truth.
- `.trellis/spec/frontend/state-management.md`: frontend consumes generated snapshot command/stale guard and does not infer command authority.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)` in this subagent session. The user prompt provided the task path explicitly, so this note was written under that path.
- No `crates/agentdash-application/src/workflow/agent_run_workspace/` module exists at research time; Phase 3/4 integration must add it or an equivalent application module and export it from `workflow/mod.rs`.
- `sessions.rs` currently reuses some mailbox mapper helpers from `lifecycle_agents.rs`; final cleanup needs to preserve RuntimeSession detail behavior or move shared mapping to a safer boundary.
- Current API stale guard active-turn helper and application conversation snapshot active-turn helper differ for completed/failed states. Treat this as a semantic alignment risk during Phase 4, not just a mechanical move.
- This research did not run tests or inspect uncommitted diffs because the instruction was read-only code research and no git operations.
