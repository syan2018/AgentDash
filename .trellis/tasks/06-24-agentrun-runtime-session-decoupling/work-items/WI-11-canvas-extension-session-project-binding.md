# WI-11 Canvas And Extension Session Project Binding

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-api/src/agent_run_runtime_surface.rs`
  - `crates/agentdash-api/src/routes/canvases.rs`
  - `crates/agentdash-api/src/routes/extension_runtime.rs`
- Tests run:
  - `cargo fmt -p agentdash-api`
  - `cargo test -p agentdash-api current_surface_project_guard -- --nocapture` passed in final integration.
  - `cargo check -p agentdash-api` passed in final integration.
- Blockers:
  - 无。
- Handoff summary:
  - API current-surface adapter 新增 Project 预校验 helper，先通过 AgentRun current surface facade 解析 runtime session current surface，再比对期望 Project。
  - Canvas runtime invoke 在解析 action key 和调用 RuntimeGateway 前校验 Canvas Project 与 current surface Project。
  - Canvas runtime snapshot/bridge manifest 在 `surface_for_actor` 前校验 Canvas Project 与 current surface Project，并复用同一 current surface VFS。
  - Extension runtime action/channel 在 backend access、Gateway/provider invocation 前校验 path Project 与 current surface Project。

## Purpose

Add explicit Canvas and Extension runtime route validation that the path Project or Canvas Project matches the current runtime session surface Project before RuntimeGateway or provider invocation.

## Dependencies

- `WI-02`

## Scope

- Canvas runtime invoke validates the Canvas Project against the current runtime session surface Project.
- Canvas runtime bridge manifest validates the Canvas Project against the current runtime session surface Project.
- Extension runtime action validates the path Project against the current runtime session surface Project before installation lookup and Gateway invocation.
- Extension runtime channel validates the path Project against the current runtime session surface Project before channel provider invocation.
- Reuse the AgentRun current surface facade; do not introduce SessionHub/current-frame fallback checks.

## Out Of Scope

- Do not move generic API current-surface adapters; `WI-05` owns API/VFS/Terminal migration.
- Do not change RuntimeGateway actor/context admission semantics except through prevalidated route/application input.

## Deliverables

- Canvas mismatch rejection tests.
- Extension mismatch rejection tests.
- Route or application facade path that returns a prevalidated runtime invocation context.

## Acceptance

- Mismatched Canvas Project / runtime session Project is rejected before Gateway invocation.
- Mismatched path Project / runtime session Project is rejected before Extension provider invocation.
- Valid Canvas and Extension paths still use AgentRun current surface DTOs for backend/runtime placement.
