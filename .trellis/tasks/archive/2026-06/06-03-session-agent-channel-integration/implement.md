# Session 与 Agent 会话信道整合实现计划

## Step 0: Pre-Development Context

- Load `trellis-before-dev` before editing source files.
- Read relevant frontend/backend/cross-layer specs:
  - `.trellis/spec/frontend/architecture.md`
  - `.trellis/spec/frontend/workflow-activity-lifecycle.md`
  - `.trellis/spec/backend/workflow/architecture.md`
  - `.trellis/spec/backend/workflow/lifecycle-run-link.md`
  - `.trellis/spec/backend/session/session-startup-pipeline.md`
  - `.trellis/spec/cross-layer/frontend-backend-contracts.md`

## Step 1: Delete Or Seal Session-First Send Paths

- Remove the `SessionChatView` UX path where a normal prompt submit reaches a Runtime trace forbidden runtime error. Without parent Agent dispatcher, the composer should be unavailable with a clear reason.
- Search and prevent session-first prompt service/API names:
  - `sendSessionPrompt`
  - `/sessions/{id}/prompt`
  - `POST /sessions/{id}/prompt`
  - frontend direct calls to `session_runtime.start_prompt` equivalents
- Update stale spec references that still name `/sessions/{id}/prompt` as a user prompt path.
- Keep `SessionRuntimeService::start_prompt` only as internal runtime delivery plumbing.

## Step 2: Backend Agent Message Command

- Locate the existing LifecycleAgent / AgentFrame resolution helpers and runtime delivery launch services.
- Add a narrow backend command endpoint for sending user messages through LifecycleAgent by delivery runtime session.
- Add request/response contract types and regenerate frontend TS contracts if the project expects generated DTO updates.
- Ensure command handler resolves:
  - delivery runtime session
  - AgentFrame
  - LifecycleAgent
  - LifecycleRun/project permission
- Delegate runtime prompt delivery only after Agent/Frame resolution.
- Add backend tests for successful dispatch and unresolved runtime session.

## Step 3: Frontend Service And Session Page Wiring

- Add a frontend service function with Agent/Lifecycle naming, mapping unknown API response to typed refs.
- Update `SessionPage` to derive send readiness from resolved AgentFrame/Lifecycle runtime state.
- Pass `customSend` into `SessionChatView`.
- After send, refresh session execution state and lifecycle/frame projections needed by the page.
- Keep Session as the visible user concept; reserve Lifecycle labels for detail/debug surfaces.

## Step 4: Frontend Tests

- Add or update tests proving `SessionPage` provides Agent dispatcher when a runtime session has AgentFrame context.
- Add regression coverage that unresolved runtime traces are non-sendable before submit.
- Keep session feed rendering tests intact.

## Step 5: Automated Validation

- `pnpm -C packages/app-web exec tsc --noEmit`
- Targeted frontend vitest files for Session page/chat/service changes.
- Targeted Rust tests for the new backend command/use-case.
- Contract generation/check command if DTO contracts change.
- `rg -n "POST /sessions/\\{id\\}/prompt|sendSessionPrompt|/sessions/\\$\\{.*\\}/prompt" packages/app-web/src crates .trellis/spec`
- `git diff --check`

## Step 6: Real Frontend Interaction Validation

- Start `pnpm dev`.
- Use browser automation or in-app browser to open the local frontend.
- Create/open a Project Agent session from the Agent entry.
- Send first message and wait for visible Agent response.
- Send second message and wait for visible Agent response.
- Record the local URL, the tested route, and observed result in final report.

## Rollback Points

- Backend route and contract can be reverted independently if route tests fail.
- Frontend SessionPage custom send wiring can be reverted independently if browser verification reveals unrelated launch instability.
