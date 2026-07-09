# Main Evidence: waitable activity and exec closure

## Verdict

AgentDashboard already has most low-level primitives required for exec continuation and asynchronous wake. The missing layer is an AgentDashboard-native application/runtime contract that exposes those primitives to the Agent and unifies waiting across exec, LifecycleGate, companion/human/subagent and mailbox.

Planning update after review: earlier research notes and subagent reports mention flat follow-up tools such as `shell_read` or an extra `exec_handle`. Final design supersedes that shape. Exec remains one `shell_exec` tool with `operation=start|read|write|terminate|resize|status`, and the canonical exec/terminal ref is `terminal_id`. Local shell session ids, backend refs and runtime trace refs stay inside the terminal activity record.

## Runtime Tool Surface

- Runtime tools are built by `SessionRuntimeToolComposer::build_tools`, which loops providers and rejects duplicate names: `crates/agentdash-application/src/runtime_tools/provider.rs:60`.
- Launch preparation calls `assemble_tool_surface_for_execution_context`, stores tools and schemas, and merges MCP tools: `crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:22`.
- Session bootstrap currently registers VFS, workflow, collaboration, task and workspace module providers: `crates/agentdash-api/src/bootstrap/session.rs:468`.
- Therefore `wait` and the `shell_exec` operation-mode terminal controls must be added through a runtime tool provider path; adding relay/local RPC alone will not make them Agent-visible.

## Current Exec Chain

- VFS execute cluster currently pushes only `ShellExecTool`: `crates/agentdash-application-vfs/src/tools/factory.rs:116`.
- `shell_exec` describes long-running commands as returning a background session after initial yield: `crates/agentdash-application-vfs/src/tools/fs/shell.rs:102`.
- Tool execution calls `VfsService::exec_with_policy` and then formats the result: `crates/agentdash-application-vfs/src/tools/fs/shell.rs:270`.
- Running result text includes `session_id` and `next_seq`: `crates/agentdash-application-vfs/src/tools/fs/shell.rs:393`.
- Structured details include `state`, `exit_code`, `session_id`, `terminal_id`, `next_seq` and truncation: `crates/agentdash-application-vfs/src/tools/fs/shell.rs:473`.

## Existing Local / Relay Primitives

- Relay protocol already defines `ToolShellReadPayload`, `ToolShellInputPayload` and `ToolShellTerminatePayload`: `crates/agentdash-relay/src/protocol/tool.rs:83`.
- Relay responses already include read chunks, `next_seq`, truncation, input acceptance and terminate status: `crates/agentdash-relay/src/protocol/tool.rs:319`.
- Local `ShellSessionManager::read_session` waits until output, terminal state or deadline: `crates/agentdash-local/src/shell_session_manager.rs:327`.
- Local `input_shell` writes stdin and returns a follow-up read: `crates/agentdash-local/src/shell_session_manager.rs:363`.
- Local `terminate_shell` is idempotent over terminal/unknown/running sessions and emits terminal state change when it kills a process: `crates/agentdash-local/src/shell_session_manager.rs:407`.
- Windows non-interactive OS shell uses PowerShell and UTF-8 output encoding: `crates/agentdash-local/src/shell_session_manager.rs:798`.

## Companion / Human Wait

- `wait_for_lifecycle_gate_resolution` polls `LifecycleGateRepository` until resolved or timeout: `crates/agentdash-application/src/companion/tools.rs:243`.
- companion `wait=true` uses private polling after dispatch: `crates/agentdash-application/src/companion/tools.rs:1066`.
- human request `wait=true` also opens a gate and privately polls it: `crates/agentdash-application/src/companion/tools.rs:1312`.
- `companion_respond` resolves parent request gates, pending actions and child-to-parent completion as independent side effects: `crates/agentdash-application/src/companion/tools.rs:1694`.
- This should become a LifecycleGate adapter under the wait module, preserving existing gate and mailbox semantics.

## Mailbox / Scheduler

- Scheduler resolves AgentRun control-plane target from runtime session: `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:154`.
- Scheduler claims messages and then decides launch/steer/resume delivery: `crates/agentdash-application-agentrun/src/agent_run/mailbox/scheduler.rs:333`.
- Runtime adapter emits `MailboxStateChanged` notification after boundary scheduling changes state: `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:229`.
- Terminal callback schedules AgentRun turn boundary on completed runtime sessions: `crates/agentdash-api/src/agent_run_mailbox.rs:81`.
- Therefore wait module should create/observe wake envelopes and let mailbox scheduler deliver continuation.

## Frontend Projection

- Terminal platform events are dispatched to terminal store: `packages/app-web/src/features/session/model/sessionPlatformEventDispatcher.ts:19`.
- Terminal store is keyed by `session_id -> terminal_id` and keeps bounded output buffers: `packages/app-web/src/features/session/model/useTerminalStore.ts:26`.
- Output and state events are projected idempotently by session event seq: `packages/app-web/src/features/session/model/useTerminalStore.ts:163`.
- Mailbox row reads `mailbox.waiting_items` and renders waiting rows: `packages/app-web/src/features/agent-run-workspace/ui/MailboxMessageRow.tsx:46`.
- Waiting item contract contains `wait_id`, `gate_id`, `kind`, source/correlation refs, status, preview and timestamps: `crates/agentdash-contracts/src/runtime/workflow.rs:1151`.
- Frontend has the observation foundation but not the Agent tool closure.

## PowerShell Context

- Environment ContextFrame already includes a Windows PowerShell object-output note: `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:55`.
- The note is rendered only on Windows and tested: `crates/agentdash-application-runtime-session/src/session/environment_context_frame.rs:193`.
- It should be strengthened with shell kind and syntax guidance because Agent-generated commands currently used Unix `&&` / `|| true` against PowerShell.

## Codex Reference Lessons

See `subagent-codex-reference.md` for detailed anchors.

Useful ideas:

- `exec_command` is start plus first bounded read/yield.
- running command returns a live handle and no exit code.
- completed command returns exit code and no live handle.
- `write_stdin` doubles as write or empty-input poll/read.
- wait tools return small status/summary/ref objects.
- mailbox/steer activity wakes waiters without transferring large payloads.

Not portable:

- Codex app-server process handles are connection-scoped.
- Codex Thread/AgentPath/session identity do not map to AgentDashboard product ownership.
- `/sessions/*` is not allowed as a new control surface here.

## Existing Endpoint Constraint

Search confirms historical `/sessions/*` diagnostic/trace routes still exist in code, including `crates/agentdash-api/src/routes/sessions.rs` and frontend services. This task must not add to or depend on those endpoints. New wait/exec control must use AgentRun/runtime tool/mailbox surfaces.
