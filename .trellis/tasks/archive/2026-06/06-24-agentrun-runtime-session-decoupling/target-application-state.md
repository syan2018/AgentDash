# Target Application Module State

## Final Shape

After this decoupling task, `agentdash-application` should still be one crate, but its internal module graph should already match the future crate graph.

## Module Responsibilities

| Module | Final responsibility |
| --- | --- |
| `agent_run` | AgentRun command surface, current runtime surface query, resource surface query, effective capability/admission, runtime surface update command boundary, project-agent context/workspace resolution, frame construction/update internals. |
| `lifecycle` | LifecycleRun control plane, dispatch/materialization, orchestration reducer, AgentRun materialization, lifecycle/resource projection. |
| `session` | RuntimeSession substrate: delivery session metadata/events, turn processing, connector live runtime, stream/trace projection, runtime command delivery, live adoption adapter implementation. |
| `runtime_gateway` | Runtime action registry, actor/context admission, Session/Setup/Extension providers, and provider implementations consuming gateway-facing ports. |
| `vfs` | Generic VFS service/provider/mutation/surface summary. It does not own AgentRun resource surface semantics. |
| `permission` | PermissionGrant lifecycle, policy and requested effect facts. AgentRun owns final admission and model-visible surface update effect. |
| `canvas` / `workspace_module` | Domain mutations and typed update requests. They do not own AgentFrame writes or active runtime adoption. |
| `api` | Auth, DTO, request/response mapping, route composition. It does not assemble current frame/resource surface from repositories. |

## Allowed Direct AgentFrame Access

- Domain entity/repository and infrastructure adapters.
- AgentRun frame construction/update/query internals.
- Lifecycle dispatch/materialization internals.
- RuntimeSession live adoption adapter validating already-persisted revisions.
- Explicit presentation/debug read-model facade.

## Forbidden Production Dependencies

- API current-surface routes importing SessionHub, current frame resolver or session construction planner internals.
- RuntimeGateway MCP access importing SessionHub or AgentFrame types.
- RuntimeGateway importing AgentRun implementation internals instead of consuming `agentdash-application-ports` contracts for gateway-facing AgentRun surface/MCP access.
- Canvas/WorkspaceModule/Permission building AgentFrame revisions directly.
- RuntimeSession public facade exporting AgentRun/Lifecycle ownership types.
- Generic VFS provider code owning AgentRun resource surface semantics.

## Import Direction Target

```text
business/API/runtime_gateway consumers
  -> AgentRun/Lifecycle facades
  -> domain repositories and ports
  -> RuntimeSession ports where delivery is needed

RuntimeSession implementation
  -> domain/spi/ports
  -> no AgentRun current-surface query ownership
```

Final verification: API, MCP and local crates consume AgentRun/VFS facades or ports rather than public implementation submodules. `session::construction_planner` has been removed; project-agent context/workspace resolution is owned by AgentRun.

## Crate Split Readiness

This task is complete when physical crate extraction becomes mechanical:

- Remaining imports express ports/facades instead of implementation internals.
- Broad public exports are reduced.
- RuntimeSession can be moved without dragging AgentRun/Lifecycle business logic.
- RuntimeGateway can be moved without dragging AgentRun implementation.
- AgentRun and Lifecycle can be split after their interaction is port-mediated.
