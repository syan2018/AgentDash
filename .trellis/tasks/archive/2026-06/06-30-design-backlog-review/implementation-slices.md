# Implementation Slices

This file turns the D1-D12 design review into ordered implementation slices. It is not a task tree, does not create Trellis child tasks, and should be used as dependency order when future implementation work is selected.

## Slice 1: Relay Typed Prompt Payload

Items: D12.

- Replace relay `prompt_blocks` raw JSON with `input: Vec<UserInputBlock>`.
- Remove cloud `UserInputBlock -> ACP JSON` relay conversion and local ACP JSON parser.
- Keep ACP conversion only at true ACP/model adapter edge.
- Validation: relay protocol serde tests and cloud/local prompt handler unit tests.

## Slice 2: RuntimeGateway Dynamic Action Catalog

Items: D7.

- Add dynamic action discovery contract.
- Make extension dynamic provider emit concrete descriptors from enabled Project installations.
- Reuse the same resolver for catalog and invoke.
- Remove actor-visible marker `extension.runtime_action`.
- Validation: RuntimeGateway surface includes concrete enabled extension actions; disabled/missing context has no descriptors.

## Slice 3: Runtime Action Availability Split

Items: D8.

- Source WorkspaceModule runtime operations from RuntimeGateway catalog.
- Preserve AgentRun effective capability as visibility owner only.
- Add typed readiness diagnostics for missing gateway/channel/backend/artifact.
- Update frontend bridge to consume runtime action surface or backend denial.
- Validation: WorkspaceModule describe/list and frontend bridge tests.

## Slice 4: AgentRun Admission Production Boundary

Items: D1.

- Implement production `AgentRunEffectiveCapabilityPort`.
- Replace capability-state-only runtime session admission assumptions.
- Wire `admit_tool` at tool invocation entry.
- Ensure denied decisions prevent `tool.execute`.
- Validation: visible state unchanged, frame-scoped grants, production `admit_tool` call, agent loop denied-execution test.

## Slice 5: Command Availability Cleanup

Items: D5.

- Remove or rename `runtime_command_state` as display-only status.
- Make all command availability resolve through `ConversationCommandAvailabilityResolver`.
- Update frontend local gates to UX-only semantics.
- Validation: command policy stale/disabled tests and frontend command handler tests.

## Slice 6: Agent Runtime Delegate Set

Items: D6, related to D1.

- Introduce delegate facets.
- Convert hook runtime, mailbox adapter and admission adapter.
- Remove broad forwarding wrapper shape.
- Make launch/prepared-turn delegate composition explicit.
- Validation: facet invocation tests and mailbox turn-boundary tests.

## Slice 7: Runtime VFS Access Policy

Items: D9.

- Narrow/rename current Project VFS mount grants.
- Add `RuntimeVfsAccessPolicy`.
- Compile Project grants and PermissionGrant path grants into policy.
- Enforce policy in VFS tool resolution after path normalization.
- Validation: mount/path allow/deny tests across read/write/search/shell.

## Slice 8: WorkspacePlacementService

Items: D10.

- Move workspace detect invocation and directory fact transaction to application service.
- Add explicit placement intents.
- Convert backend inventory register, bind-discovered, create/update and sync paths.
- Remove route-local detect helpers.
- Validation: service tests for each intent and API adapter tests.

## Slice 9: Desktop Local Ownership

Items: D11.

- Move desktop profile/settings DTOs and IO to `agentdash-local`.
- Move desktop access-token ensure client and response validation to `agentdash-local`.
- Reduce Tauri to shell adapter.
- Align TS port with moved Rust contract.
- Validation: local unit tests and Tauri command compile check.

## Slice 10: Canonical Launch Command

Items: D4. Requires user decision.

- Add canonical `LaunchCommand` / `LaunchPlanningInput` in chosen owner.
- Remove AgentRun/RuntimeSession/FrameLaunch duplicate models.
- Delete mapping loop functions.
- Make backend selection planner-only.
- Validation: grep/static one-model check and focused launch pipeline tests.

## Slice 11: Shared Lifecycle Gate Resolver

Items: D3. Requires user decision.

- Add `GateTransitionOutcome` and delivery intents.
- Move gate transitions into shared resolver.
- Move mailbox/session delivery to adapters.
- Stop writing delivery status into gate payload.
- Validation: resolver tests, adapter tests and companion gate route test.

## Slice 12: Lifecycle Dispatch Internal Owners

Items: D2. Best after D3/D4 decisions.

- Extract `RunOrchestrationStarter`.
- Extract `AgentRuntimeMaterializer`.
- Extract `SubjectAssociationWriter`.
- Extract `LifecycleRelationWriter`.
- Extract `OrchestrationReducerBridge`.
- Keep public dispatch facade stable.
- Validation: lifecycle dispatch tests and graph-backed reducer/anchor consistency regression.

## Dependency Notes

- D1 can start before D6, but final clean shape is better when D6 introduces `RuntimeToolPolicyDelegate`.
- D3 should settle before D2 relation/gate writer extraction so the relation writer targets the final gate resolver shape.
- D4 should settle before broad dispatch materialization refactors, because lifecycle dispatch creates launch/runtime refs.
- D7/D8 should precede any WorkspaceModule owner split so action catalog semantics are not reworked twice.
