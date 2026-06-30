# Decision Points

Only D3 and D4 need user-level architectural confirmation. All other items are marked `self-decided` because code and specs already imply a single convergence direction.

## D4. Canonical Launch Command Owner

### Decision

Where should the single launch command/source model live?

### Why It Matters

AgentRun, RuntimeSession and FrameLaunch currently each define source/command/modifier models and map between them. This creates a loop rather than a boundary:

```text
AgentRun command -> RuntimeSession command -> FrameLaunch command -> application command
```

The owner choice affects launch source language, backend placement ownership and future source adapters.

### Options

| Option | Description | Trade-off |
| --- | --- | --- |
| A | Put canonical model in `agentdash-application-ports` | Recommended. Neutral boundary for AgentRun, RuntimeSession and FrameConstruction; makes backend selection a planner input. |
| B | RuntimeSession owns launch command | Smaller initial move, but makes delivery/trace substrate own source identity. |
| C | AgentRun owns launch command | Aligns AgentRun workspace commands, but non-AgentRun sources would depend on AgentRun language. |

### Recommendation

Choose A: application ports own canonical `LaunchCommand`, and backend selection moves into `LaunchPlanningInput`.

### Implementation After Decision

- Add canonical model.
- Remove AgentRun/RuntimeSession/FrameLaunch duplicate enums.
- Delete mapping functions.
- Update frame construction and launch planner to consume command/planning input separately.

## D3. Shared Lifecycle Gate Resolver

### Decision

Should gate resolution be companion-only first, or shared across CompanionGate, workflow HumanGate and future Routine gates?

### Why It Matters

`CompanionGateControlService` currently mixes durable gate facts with mailbox/session delivery. A resolver split is necessary either way, but making it companion-only may preserve a second human-gate language in workflow orchestration.

### Options

| Option | Description | Trade-off |
| --- | --- | --- |
| A | Companion-only resolver now | Fastest, lower blast radius, but leaves workflow HumanGate as a parallel gate model. |
| B | Shared `LifecycleGateResolver` for companion/workflow/routine | Recommended. Single durable gate transition language; higher upfront design cost. |
| C | Keep facade and only move methods to submodules | Lowest churn, but does not fix owner boundary. |

### Recommendation

Choose B: shared `LifecycleGateResolver`, with Companion-specific context resolver and delivery adapters.

### Implementation After Decision

- Add `GateTransitionOutcome` / `GateDeliveryIntent`.
- Move pure gate transitions into resolver.
- Move mailbox/session event delivery into adapters.
- Stop storing mailbox delivery status blobs in gate payload.

## Self-Decided Items

- D1: AgentRun owns visible capability/admission; wire production `admit_tool`.
- D2: keep Lifecycle dispatch facade, split internal owners.
- D5: `ConversationCommandAvailabilityResolver` is the command availability owner.
- D6: split `AgentRuntimeDelegate` into delegate facets.
- D7: RuntimeGateway owns dynamic runtime action discovery.
- D8: split visibility/catalog/readiness across AgentRun/RuntimeGateway/WorkspaceModule.
- D9: add runtime VFS access policy; narrow Project VFS grant.
- D10: create application-level `WorkspacePlacementService`.
- D11: move desktop profile/claim/settings to `agentdash-local`.
- D12: relay prompt payload uses typed `UserInputBlock`.
