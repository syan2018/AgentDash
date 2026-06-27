# Implement Plan: Canvas 交互诊断与模块边界实现

## Phase 0: Main Task Alignment

- [x] 将任务从 planning parent 改为主任务直接实现。
- [x] 保持 MVP 不包含截图 artifact。
- [x] 将 observation / interaction / submit API 归属改为 AgentRun→Canvas 引用。
- [x] 明确 Canvas 前端 bridge 不持有、不传入 `sessionId`。
- [x] 清理 Trellis metadata 中的历史 child 引用。

## Phase 1: Canvas Observation And Interaction State

- [x] Add bridge envelope types in `CanvasRuntimePreview.tsx`.
- [x] Extend `CanvasRuntimePreview.runtime.ts` with `agentdash.interaction` SDK and render observation capture.
- [x] Add frontend service methods for AgentRun-scoped observation/snapshot upload.
- [x] Add Rust DTOs for observation and interaction snapshot.
- [x] Add storage/repository for AgentRun↔Canvas latest observation and interaction snapshot.
- [x] Replace Canvas runtime snapshot/invoke usage with AgentRun-scoped routes that do not accept `sessionId` from the Canvas frontend.
- [x] Add API routes under `/agent-runs/{run_id}/agents/{agent_id}/canvases/{canvas_mount_id}/...`.
- [x] Add Agent/workspace module operations for inspect render state and get interaction state.

Validation:

```powershell
pnpm --filter @agentdash/app-web test -- CanvasRuntimePreview
cargo test -p agentdash-api canvas
cargo test -p agentdash-application canvas
pnpm run contracts:check
```

## Phase 2: Canvas Submit-To-Agent

- [x] Add `window.agentdash.agent.submit(...)` SDK in the Canvas iframe runtime.
- [x] Add parent-page handler that validates frame generation and calls AgentRun-scoped Canvas submit API without passing `sessionId`.
- [x] Add Canvas submit request/response DTO.
- [x] Add `MailboxMessageSource::CanvasAction` or equivalent source variant.
- [x] Implement backend route resolving `run_id + agent_id + canvas_mount_id` to the current AgentRun Canvas reference and backend current delivery target.
- [x] Reuse `AgentRunMailboxService.accept_user_message` with canonical `UserInputBlock`.
- [x] Return `AgentRunMessageCommandResponse` to the iframe.
- [x] Refresh AgentRun workspace projection after successful submit.

Validation:

```powershell
cargo test -p agentdash-application agent_run::mailbox
cargo test -p agentdash-api agent_run
cargo test -p agentdash-contracts
pnpm run contracts:check
pnpm --filter @agentdash/app-web test -- agentRunMailbox
```

## Phase 3: Crate Boundary Follow-Through

- [x] Establish `agentdash-workspace-module` crate as the Workspace Module business boundary.
- [x] Move Canvas business objects into `agentdash-workspace-module::canvas` and remove the independent `agentdash-canvas` crate.
- [x] Keep Canvas entity, value objects, repository contracts, runtime state contracts and embedded skill bundle in `agentdash-domain::canvas`.
- [x] Move Canvas identity helpers, management/runtime/VFS/visibility business services, operation keys and runtime tool support under `agentdash-workspace-module::canvas` / `agentdash-workspace-module::workspace_module`.
- [x] Replace workspace-module runtime session bridge naming with AgentRun bridge naming; runtime session ids remain adapter-internal delivery trace coordinates.
- [x] Keep HTTP authorization, route mapping, Postgres adapters, concrete RuntimeGateway/service wiring, AgentRun delivery selection and extension package artifact storage in application/API/infrastructure crates.

Validation:

```powershell
cargo test -p agentdash-workspace-module
cargo check -p agentdash-workspace-module --tests
cargo check -p agentdash-application -p agentdash-api --tests
cargo check -p agentdash-api
rg "agentdash-workspace-module|agentdash_workspace_module" crates/agentdash-domain crates/agentdash-infrastructure -n
rg "WorkspaceModuleSessionBridge|SharedWorkspaceModuleSessionBridgeHandle|session_bridge|with_runtime_visibility" crates/agentdash-workspace-module/src crates/agentdash-api/src/bootstrap/session.rs -n
```

## Phase 4: Integration Review

- [ ] Verify Canvas tab loaded from `canvas://{canvas_mount_id}` supports observation, interaction and submit.
- [ ] Verify extension `canvas_panel` either hydrates live AgentRun bridge context or reports bridge unavailability clearly.
- [x] Verify runtime action bridge still uses RuntimeGateway and is distinct from submit-to-Agent.
- [x] Verify Canvas interaction state does not enter mailbox unless a submit action includes it.
- [x] Verify Agent can inspect latest render state after Canvas ready/error.

Validation:

```powershell
pnpm dev
pnpm --filter @agentdash/app-web test
cargo test --workspace
```

## Files Likely Touched

- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.tsx`
- `packages/app-web/src/features/canvas-panel/CanvasRuntimePreview.runtime.ts`
- `packages/app-web/src/services/canvas.ts`
- `packages/app-web/src/types/canvas.ts`
- `crates/agentdash-contracts/src/surface/canvas.rs`
- `crates/agentdash-contracts/src/agent/run_mailbox.rs`
- `crates/agentdash-api/src/routes/canvases.rs`
- `crates/agentdash-application/src/canvas/*`
- `crates/agentdash-application/src/agent_run/mailbox.rs`
- `crates/agentdash-domain/src/agent_run_mailbox/mod.rs`
- `crates/agentdash-infrastructure/migrations/*`
- `crates/agentdash-infrastructure/src/persistence/postgres/*`

## Rollback Points

- Bridge SDK can be introduced behind generated runtime snapshot capability flags before route wiring.
- Observation/interaction storage can be reverted independently from submit-to-Agent if no mailbox enum change has shipped.
- Crate extraction should keep application adapters outside `agentdash-workspace-module` so the business crate remains independent from HTTP, Postgres, RuntimeGateway and VFS runtime implementations.
