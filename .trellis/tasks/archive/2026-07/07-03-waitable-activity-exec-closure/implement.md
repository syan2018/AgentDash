# Implementation Plan

## Work Organization

This parent task remains the planning and integration owner. Implementation proceeds through two child tasks:

1. `07-03-exec-terminal-blocker-repair`
2. `07-03-waitable-activity-module`

The implementation order is intentional. The exec/terminal repair creates the minimal operational closure needed to verify real shell sessions before the common wait module wraps them as activities.

## Phase A: Exec / Terminal Blocker Repair

Implementation target: `07-03-exec-terminal-blocker-repair`.

Checklist:

- Keep one Agent-facing `shell_exec` tool and add operation-mode handling to it.
- Reuse existing relay/local `ToolShellReadPayload`, `ToolShellInputPayload`, `ToolShellTerminatePayload`.
- Introduce a canonical `terminal_id` / terminal record for running `shell_exec`.
- Ensure `terminal_id` resolution validates current AgentRun/runtime owner and backend/mount refs.
- Add `shell_exec operation=read|write|terminate|status` schemas and results.
- Preserve bounded output, `next_seq`, truncation and final exit code semantics.
- Strengthen Windows Environment ContextFrame and tests.
- Repair frontend terminal projection/open path for running shell output and state.
- Verify no new `/sessions/*` control endpoint is introduced.

Validation:

- Rust tests for tool catalog and VFS shell tools.
- Rust tests or integration tests for local shell read/input/terminate/status.
- ContextFrame unit tests for Windows PowerShell wording.
- Frontend tests for terminal projection/opening behavior.
- `rg -n "/sessions" crates packages` review to ensure no new dependency in changed code.

## Phase B: Waitable Activity Module

Implementation target: `07-03-waitable-activity-module`.

Checklist:

- Add waitable activity domain/application model and repository/storage decision.
- Add wait service with register/update/wait/notify APIs.
- Add runtime tool provider exposing generic `wait`.
- Register exec running `terminal_id` refs as `kind=exec` activities.
- Replace companion/subagent/human private polling with wait service path.
- Wire LifecycleGate resolution into activity update and mailbox wake.
- Wire mailbox pending/completed wake observation into wait readiness.
- Define source identity/dedup for exec completion/failure/cancel and companion/human/subagent results.
- Extend workspace snapshot / waiting item projection as needed for exec activities.
- Update generated frontend contracts and waiting item UI tests.
- Verify wait timeout does not terminate work; terminate/cancel is explicit.

Validation:

- Unit tests for wait service state transitions and timeout.
- Runtime tool catalog tests showing `wait` is present.
- Exec wait tests: running -> output -> completed with exit code.
- Companion/human tests: gate opened -> wait timeout -> later resolution -> mailbox wake.
- Mailbox dedup tests for repeated wake/result.
- Frontend tests for exec/human/subagent waiting rows.
- No `/sessions/*` endpoint/control dependency in new code.

## Phase C: Parent Integration And PR

After both child tasks pass their own checks:

- run focused backend/frontend tests from both child tasks;
- run generated contract checks if DTOs changed;
- inspect git diff for unrelated workspace changes;
- update relevant `.trellis/spec/` only with durable module contracts learned from implementation;
- create commits per phase using `type(scope): 中文信息` with bullet notes;
- create one PR from the final branch to `main`.

## Review Gate

Do not start Phase A until the user reviews this planning set and confirms implementation.

Planning is ready when:

- parent `prd.md`, `design.md`, `implement.md` exist;
- child PRDs exist;
- `implement.jsonl` and `check.jsonl` contain real context entries;
- subagent research and main evidence are available under `research/`.

## Rollback Points

- Exec operation-mode changes can be reverted independently of wait module if tool schema or permission policy conflicts appear.
- ContextFrame wording is isolated and covered by unit tests.
- Frontend terminal projection changes should stay separate from wait activity DTO changes.
- Wait module migrations, if required, must be reviewed before implementation because this pre-release project prefers correct schema shape over compatibility shims.
