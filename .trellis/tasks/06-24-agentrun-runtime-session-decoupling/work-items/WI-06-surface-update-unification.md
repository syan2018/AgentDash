# WI-06 Surface Update Unification

Status: pending

Assigned Worker: unassigned

## Tracking

- Files changed: TBD.
- Tests run: TBD.
- Blockers: None recorded.
- Handoff summary: TBD.

## Purpose

Make AgentRun typed update facade the single public entry for surface-changing business paths.

## Dependencies

- `WI-01`
- `WI-02`
- `WI-03`

## Scope

- Route Canvas expose/bind through generic AgentRun update command.
- Move Permission frame-writing adapter under AgentRun or hide it behind AgentRun-owned port.
- WorkspaceModule surface-changing paths submit typed AgentRun update requests only.
- Decide and document handling for currently contract-only update variants: MCP preset, Project VFS mount, Skill inventory, AgentProcedure contract.

## Out Of Scope

- Do not change PermissionGrant domain state machine.
- Do not change Canvas domain storage semantics.

## Deliverables

- Surface-changing business modules do not own `AgentFrameBuilder` or active adoption primitive.
- Regression tests for Canvas bind, Permission apply/revoke, WorkspaceModule update.

## Acceptance

- `rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission` has no public business-path usage except AgentRun-owned adapter exceptions.
- Relevant tests pass.
