# Command Availability Cleanup Design

## Boundary

`ConversationCommandAvailabilityResolver` owns command availability. It produces command ids, kinds, placements, keyboard mappings, enabled flags, disabled reasons and stale guards for both UI projection and server-side command policy validation.

Workspace shell status remains display metadata. It may explain the workspace chrome or list state, but it must not decide whether a mutating command is semantically allowed.

## Current Split To Converge

- `ConversationCommandAvailabilityResolver` already builds UI-visible commands and preconditions.
- `AgentRunWorkspaceCommandPolicy` already re-resolves the same availability model and validates submitted preconditions.
- `AgentRunWorkspaceProjection::runtime_command_state` derives another status/message model from `SessionExecutionState` but does not appear in generated public contracts.
- Frontend `useAgentRunWorkspaceCommands` submits command preconditions, but still blocks several commands when `workspaceStatus !== "ready"`.

## Target Shape

1. Backend projection cleanup
   - Remove `AgentRunWorkspaceRuntimeCommandStateModel`, `AgentRunWorkspaceRuntimeCommandStatus`, and `AgentRunWorkspaceProjection::runtime_command_state` if no public contract requires them.
   - Keep `AgentRunWorkspaceProjectionModel.state_code`, `delivery_status`, `active_turn_id`, `last_turn_id` as display/status projection.
   - Keep policy tests centered on `ConversationCommandAvailabilityResolver`.

2. Frontend command cleanup
   - Remove `workspaceStatus !== "ready"` semantic blocks from composer submit, cancel, promote, and resume handlers.
   - Retain null `currentRunId/currentAgentId` guards, draft readiness checks, local input/model completeness checks, backend `command.enabled` checks, command preconditions and stale refresh.
   - If UI needs loading prevention, it should happen outside command authority and not shadow backend-enabled commands.

3. Spec sync
   - Record that command availability is resolver-owned and shell status is display-only.

## Non-Goals

- Do not introduce a new command availability service.
- Do not redesign command DTOs or stale guard format.
- Do not alter mailbox command semantics beyond removing local workspace-status authority.
