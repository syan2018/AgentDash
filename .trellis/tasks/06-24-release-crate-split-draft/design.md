# Release Crate Split Draft

## Draft Position

Physical crate extraction is a later step. The first release-facing task is boundary decoupling inside `agentdash-application`. Crate split starts only after imports already express the intended graph.

## Candidate Crates

| Candidate | Responsibility | Must Not Own |
| --- | --- | --- |
| `agentdash-application-ports` expansion | Pure ports/DTOs for AgentRun current surface, RuntimeSession delivery/adoption, VFS runtime projection, RuntimeGateway transport-facing contracts | Application services, repository sets, API DTOs |
| `agentdash-runtime-session` or `agentdash-application-runtime-session` | RuntimeSession delivery/trace substrate: session meta/events, turn processing, connector live runtime, runtime command delivery, live adoption adapter implementation | AgentRun current surface query, AgentFrame write ownership, Permission, Canvas, WorkspaceModule, RuntimeGateway providers |
| `agentdash-application-runtime-gateway` | RuntimeGateway registry, actor/context admission, setup/session/extension providers | AgentRun implementation, API routes, infrastructure probe implementation |
| `agentdash-application-agentrun` | AgentRun current surface query/update, effective capability/admission, workspace/resource surface facade, mailbox/workspace command surface | RuntimeSession live internals, Lifecycle orchestration reducer implementation |
| `agentdash-application-lifecycle` | LifecycleRun dispatch/orchestration/reducer/surface projection and AgentRun materialization use cases | RuntimeSession storage implementation, API route DTOs |
| Future VFS crate | Generic VFS provider/service/surface/mutation/runtime tools after resource surface separation | AgentRun resource surface ownership, Lifecycle node state ownership |

## Dependency Direction

```text
agentdash-api
  -> application facade crates
  -> agentdash-application-ports
  -> agentdash-domain / agentdash-spi / protocol crates

AgentRun / Lifecycle
  -> RuntimeSession ports
RuntimeSession implementation
  -> ports + domain/spi
RuntimeGateway
  -> gateway-facing ports + domain/spi
```

RuntimeSession must not depend on AgentRun implementation. AgentRun/Lifecycle may depend on RuntimeSession ports and receive RuntimeSession implementation through API/local composition root.

## Extraction Waves

### Wave 0: Preconditions

- Complete `06-24-agentrun-runtime-session-decoupling` facade and visibility cleanup phases.
- No production API route imports `session::construction_planner`, `session::plan`, `session::AgentFrameRuntimeTarget`, or current frame resolver for current surface behavior.
- `agent_run <-> session` direct imports reduced to explicit RuntimeSession port/facade imports.

### Wave 1: Ports

- Expand `agentdash-application-ports`.
- Move pure DTO/traits only.
- Keep all implementations in current crates.

### Wave 2: Low-Risk Extraction

- Extract RuntimeGateway after it consumes only ports.
- Extract RuntimeSession substrate after it no longer owns AgentRun/Lifecycle facts.

### Wave 3: Control Plane Extraction

- Extract AgentRun and Lifecycle after their interaction is port-mediated.
- Defer VFS extraction until AgentRun resource surface and generic VFS provider boundaries are separated.

## Gates

- `cargo metadata --no-deps --format-version 1`
- `cargo check --workspace`
- Targeted tests per extracted crate:
  - RuntimeGateway provider/admission/MCP tests
  - RuntimeSession delivery/event/turn tests
  - AgentRun runtime surface/effective capability/workspace tests
  - Lifecycle dispatch/orchestration tests

## Blocking Conditions

- Broad `pub use` surfaces still expose internals as public API.
- AgentRun imports session internals directly for business surface behavior.
- Session imports AgentRun/Lifecycle implementation details for current surface query.
- API route layer still chooses anchors/current frames for resource/current surface.
- RuntimeGateway consumes AgentRun implementation instead of a port.
