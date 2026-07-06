# Implement Plan

## Current State

Task status: planning. Activation happens after PRD/design/implement review.

## Dispatch Plan

This task is cross-layer and should use parallel implement agents after start:

- Toolchain/package agent: `packages/extension/src/toolchain/**` packaging and validation for backend service entries.
- Local runtime agent: `crates/agentdash-local/**` materialization, process lifecycle, health/logs.
- Relay/API agent: relay payload, API/service bridge invocation, generated contracts.
- Workspace Module agent: backendService dispatch from operation catalog, readiness behavior, Agent visibility tests.

Main session owns integration, validation, spec update and commits.

## Milestones

### M1: Package Contract

- Validate `backend_services[].entry`, `routes`, `health_path`, `runtime`.
- Ensure backend service files are included in `.agentdash-extension.tgz`.
- Add smoke fixture with `agentdash.app.ts` + backend service entry.
- Preserve explicit route binding semantics through manifest/service declarations.

### M2: Local Materialization And Lifecycle

- Add service instance identity and cache layout.
- Materialize package artifact into local runtime cache.
- Start Node service process with bounded env/cwd.
- Track readiness, endpoint, process state and logs.
- Implement stop/restart cleanup.

### M3: Bridge Invoke

- Extend relay/local command handling for backend service invoke.
- Route request to local service endpoint and return status/headers/body.
- Include metadata: project id, backend id, extension key/id, service key, route, trace id.
- Map unavailable states to structured diagnostics.

### M4: Workspace Module And Panel Fetch

- Replace fail-closed backendService dispatch with readiness-aware bridge invoke.
- Keep `panel_only` blocked for Agent invoke.
- Connect panel `fetch_routes` backendService target to the same bridge path.
- Cover no-body status and route mismatch behavior.

### M5: Verification And Spec

- Generate/check contracts.
- Run focused tests for extension toolchain, local runtime, relay/API, workspace module.
- Update specs only for stable contracts discovered during implementation.

## Validation Commands

```bash
pnpm --filter @agentdash/extension run test
pnpm run contracts:generate
pnpm run contracts:check
cargo test -p agentdash-workspace-module backend_service --lib
cargo test -p agentdash-relay extension_backend_service --lib
cargo test -p agentdash-api extension_runtime --lib
cargo test -p agentdash-local extension_backend_service --lib
cargo test -p agentdash-application-runtime-gateway extension_action --lib
rg -n "@agentdash/extension-(sdk|ui|dev)|process\\.execute" packages/extension examples/extensions crates/agentdash-workspace-module crates/agentdash-domain crates/agentdash-local
git diff --check
```

## Review Gates

- Backend service can be declared, packaged and materialized.
- Cloud/API carries invoke intent while local runtime performs private network access.
- Workspace Module operation behavior is driven by `operation_catalog`.
- Service unavailable behavior is deterministic and diagnosable.
- Public authoring remains centered on `backendService()`.
