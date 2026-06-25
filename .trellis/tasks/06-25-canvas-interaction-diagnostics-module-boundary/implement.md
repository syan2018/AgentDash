# Implement Plan: Canvas 交互诊断与模块边界预研

## Phase 0: Planning Review

- [ ] Review this PRD/design/implement with the user.
- [ ] Decide whether crate extraction is a prerequisite child task or a follow-up child task.
- [ ] Confirm MVP excludes screenshot artifact unless explicitly prioritized.
- [ ] Create child tasks after scope approval.

## Phase 1: Canvas Observation And Interaction State

- [ ] Add bridge envelope types in `CanvasRuntimePreview.tsx`.
- [ ] Extend `CanvasRuntimePreview.runtime.ts` with `agentdash.interaction` SDK and render observation capture.
- [ ] Add frontend service methods for observation/snapshot upload and retrieval.
- [ ] Add Rust DTOs for observation and interaction snapshot.
- [ ] Add storage/repository for AgentRun↔Canvas latest observation and interaction snapshot.
- [ ] Add API routes under Canvas runtime route group.
- [ ] Add Agent/workspace module operations for inspect render state and get interaction state.

Validation:

```powershell
pnpm --filter @agentdash/app-web test -- CanvasRuntimePreview
cargo test -p agentdash-api canvas
cargo test -p agentdash-application canvas
pnpm run contracts:check
```

## Phase 2: Canvas Submit-To-Agent

- [ ] Add `window.agentdash.agent.submit(...)` SDK in the Canvas iframe runtime.
- [ ] Add parent-page handler that validates frame generation and calls Canvas submit API.
- [ ] Add Canvas submit request/response DTO.
- [ ] Add `MailboxMessageSource::CanvasAction` or equivalent source variant.
- [ ] Implement backend route resolving `run_id + agent_id + canvas_mount_id` to the current AgentRun Canvas reference and delivery target.
- [ ] Reuse `AgentRunMailboxService.accept_user_message` with canonical `UserInputBlock`.
- [ ] Return `AgentRunMessageCommandResponse` to the iframe.
- [ ] Refresh AgentRun workspace projection after successful submit.

Validation:

```powershell
cargo test -p agentdash-application agent_run::mailbox
cargo test -p agentdash-api agent_run
cargo test -p agentdash-contracts
pnpm run contracts:check
pnpm --filter @agentdash/app-web test -- agentRunMailbox
```

## Phase 3: Crate Boundary Evaluation / Extraction

- [ ] Inspect current Canvas and Workspace Module dependencies with `cargo metadata` or targeted `rg`.
- [ ] Identify pure identity/value/helper files that can move without runtime behavior changes.
- [ ] Decide crate shape:
  - `agentdash-canvas` only.
  - `agentdash-canvas` plus `agentdash-workspace-module`.
  - defer extraction until bridge MVP stabilizes.
- [ ] If extraction is approved, create the crate manifest and move pure helpers first.
- [ ] Update application/api imports and contract generation paths.
- [ ] Keep runtime gateway, repository implementations, VFS service and AgentRun surface update logic in application/API crates.

Validation:

```powershell
cargo check --workspace
cargo test -p agentdash-canvas
cargo test -p agentdash-application canvas
cargo test -p agentdash-api canvas
pnpm run contracts:check
```

## Phase 4: Integration Review

- [ ] Verify Canvas tab loaded from `canvas://{canvas_mount_id}` supports observation, interaction and submit.
- [ ] Verify extension `canvas_panel` either hydrates live session context or reports bridge unavailability clearly.
- [ ] Verify runtime action bridge still uses RuntimeGateway and is distinct from submit-to-Agent.
- [ ] Verify Canvas interaction state does not enter mailbox unless a submit action includes it.
- [ ] Verify Agent can inspect latest render state after Canvas ready/error.

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

## Review Gates

- Gate 1: User approves MVP and crate split ordering.
- Gate 2: DTO and storage shape reviewed before migration.
- Gate 3: Mailbox source and submit route reviewed before implementation.
- Gate 4: Crate extraction starts only after a dependency map confirms pure helper candidates.

## Rollback Points

- Bridge SDK can be introduced behind generated runtime snapshot capability flags before route wiring.
- Observation/interaction storage can be reverted independently from submit-to-Agent if no mailbox enum change has shipped.
- Crate extraction should be committed separately from behavior changes so import moves can be reverted without losing bridge implementation.
