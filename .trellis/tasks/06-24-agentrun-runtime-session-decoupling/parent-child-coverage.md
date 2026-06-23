# Parent Child Task Coverage

## Purpose

This document maps the parent task's original child task map back to this decoupling task's work items. It exists to prevent the decoupling plan from drifting into a narrower "session cleanup" task.

## Coverage Matrix

| Parent child task | Required scope | Covered by | Coverage status |
| --- | --- | --- | --- |
| `agentrun-current-surface-facade` | Settle `AgentRunRuntimeSurfaceQueryPort`, runtime surface DTOs, resource surface facade and effective capability/admission facade as public application boundary. | `WI-02`, `WI-06`, `WI-04` | Covered, split by query/resource/update/gateway consumer ownership. `WI-02` owns query/resource DTOs; `WI-06` owns update/admission write paths; `WI-04` consumes the query from RuntimeGateway. |
| `runtime-session-substrate-facade` | Tighten `session/mod.rs`, move `AgentFrameRuntimeTarget` ownership to AgentRun, limit RuntimeSession public surface to delivery/trace/turn/event/resume/debug/persistence. | `WI-01`, `WI-03` | Covered, intentionally split because target/adopter ownership blocks facade tightening. |
| `launch-commit-agentrun-boundary` | Move AgentFrame write, LifecycleAgent delivery binding and bootstrap decisions from session launch commit/orchestrator into AgentRun/Lifecycle adapters. | `WI-07` | Covered. |
| `runtime-gateway-port-boundary` | Move gateway-facing AgentRun surface/MCP access contracts to `agentdash-application-ports`; keep providers behind RuntimeGateway facade. | `WI-04` | Covered. `WI-04` owns the ports crate contract move as a required boundary step. |
| `vfs-resource-surface-facade` | Move AgentRun resource surface query out of API `session_construction.rs`, preserve launch frame/current surface frame ids, clean VFS AgentRun latest-anchor selection from route layer. | `WI-02`, `WI-05` | Covered, split between application facade/DTO ownership and API route migration. |
| `canvas-extension-session-project-binding` | Add explicit Canvas/Extension runtime route validation that path Project/Canvas Project matches current runtime session surface Project before Gateway/provider invocation. | `WI-11` | Covered as an independent work item so binding guards can be implemented without coupling them to VFS/Terminal route cleanup. |
| `application-public-visibility-cleanup` | Reduce `pub mod` / `pub use` exposure in application root, `session/mod.rs`, `agent_run/frame/mod.rs`, and `vfs/mod.rs`. | `WI-09` | Covered. |

## Parent Crate Split Tasks

The parent task also defines physical crate extraction tasks:

- `physical-crate-extraction-wave-1`
- `physical-crate-extraction-wave-2`

Those are intentionally not part of this implementation task. They remain in `.trellis/tasks/06-24-release-crate-split-draft/` and must consume this task's final import graph after `WI-10`.

## Non-Negotiable Parent Requirements

- `agentdash-application-ports` is the first crate-level expansion point for pure gateway-facing AgentRun surface/MCP access contracts.
- `ApiCurrentRuntimeSurface` or its replacement must distinguish launch evidence frame id from current surface frame id.
- API routes may perform auth, DTO mapping and error mapping, but must not own current frame/resource surface assembly.
- RuntimeGateway MCP access must not import SessionHub, raw `AgentFrame`, `AgentFrameSurfaceExt`, or current frame resolver.
- Canvas and Extension runtime paths must reject mismatched Project/session bindings before Gateway/provider invocation.
- Physical crate extraction waits until facade and visibility cleanup make the import graph mechanical.
