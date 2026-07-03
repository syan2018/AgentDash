# Codex Crate Reuse Review

## Local Dependency Facts

The current worktree already depends on selected Codex crates:

- `Cargo.toml` declares `codex-app-server-protocol`, `codex-utils-pty`, and `codex-utils-output-truncation` from the Codex git tag.
- `crates/agentdash-local/Cargo.toml` depends on `codex-utils-pty` and `codex-utils-output-truncation`.
- `crates/agentdash-local/src/shell_session_manager.rs` already uses:
  - `codex_utils_pty::{spawn_pty_process, spawn_pipe_process, ProcessHandle, SpawnedProcess, TerminalSize}`
  - `codex_utils_output_truncation::{TruncationPolicy, approx_tokens_from_byte_count, truncate_text}`
- `agentdash-local` also declares a direct `portable-pty = "0.9"` dependency, but there is no direct `portable_pty::` call in `crates/agentdash-local`; the direct dependency looks redundant while `codex-utils-pty` remains the chosen process wrapper.
- `codex-app-server-protocol` is already a broader project dependency through `agentdash-agent-protocol` and Backbone-oriented DTOs. This does not mean its terminal/process protocol DTOs should be imported for AgentDashboard exec control.

## Directly Useful

### `codex-utils-pty`

Decision: keep and use as the local process backend wrapper.

Why:

- It is a thin crate compared with `codex-core`.
- It wraps both pipe and PTY process modes behind one `ProcessHandle`.
- It already exposes the primitives this task needs:
  - `spawn_pipe_process`
  - `spawn_pty_process`
  - `ProcessHandle::writer_sender`
  - `ProcessHandle::close_stdin`
  - `ProcessHandle::resize`
  - `ProcessHandle::request_terminate` / `terminate`
  - exit state and exit code accessors
- It includes Windows PTY handling and input normalization that should not be reimplemented in this task.
- Using it returns shell execution to byte-stream semantics. This is the right layer for terminal output; PowerShell object formatting issues should not be solved by creating an object-stream executor.

Boundary:

- Keep `ProcessHandle` inside `agentdash-local` / terminal activity internals.
- Do not expose Codex process ids or handles to Agent-facing tools.
- The public continuation ref remains AgentDashboard `terminal_id`.

### `codex-utils-output-truncation`

Decision: acceptable to keep for the current local shell manager, but use narrowly.

Why:

- It provides the exact helpers already used by retained output buffers and omitted-token estimates.
- It keeps truncation behavior close to Codex's exec output budget model.
- The extra dependency cost is currently acceptable because this project already depends on `codex-protocol` through `codex-app-server-protocol`.

Boundary:

- Do not let Codex `TruncationPolicy` leak into AgentDashboard public relay/runtime contracts.
- AgentDashboard relay DTOs should continue to own `ToolShellTruncationInfo`.
- If the project later reduces Codex protocol coupling, this crate is easy to replace with a small local helper or `agentdash-relay` truncation functions.

## Existing But Not For This Exec/Wait Protocol

### `codex-app-server-protocol`

Decision: keep existing Backbone usage, but do not use it as the exec/wait control protocol.

Why:

- The crate is broad and pulls Codex protocol, macros, schema generation and TS export concerns.
- Its process and command-exec DTOs are connection-scoped and Codex-owned.
- AgentDashboard needs AgentRun-owned terminal activity, mailbox wake and frontend projection boundaries.

Use as reference:

- `command/exec`
- `command/exec/write`
- `command/exec/terminate`
- `command/exec/resize`
- `command/exec/outputDelta`
- raw `process/spawn`
- `process/writeStdin`
- `process/kill`
- `process/resizePty`
- `process/outputDelta`
- `process/exited`

Do not copy:

- Codex session/thread/process handle identity.
- Connection-scoped process ownership.
- Any public `/sessions/*` style control surface.

## Reference Only

### `codex-exec-server-protocol`

Decision: reference only.

Why:

- It has a useful compact operation set: `process/start`, `process/read`, `process/write`, `process/signal`, `process/terminate`, `process/output`, `process/exited`, `process/closed`.
- It also pulls Codex file-system sandbox, network proxy, path URI and shell-command crates.
- It models a `process_id` scoped to the exec-server session. AgentDashboard should not add another public process index when `terminal_id` is already the canonical ref.
- It does not cover the full app-server resize semantics that AgentDashboard needs for terminal projection.

Borrow:

- `read` after-seq cursor shape.
- byte chunk stream model.
- exited/closed distinction.
- write status vocabulary.

Adapt into:

- `terminal_id`
- AgentRun owner validation
- relay/local shell session control DTOs
- wait activity projections

### `codex-core`

Decision: do not depend on directly.

Why:

- Unified exec is embedded inside Codex core with sandbox, approvals, hooks, config and model-tool lifecycle.
- The process manager behavior is valuable as an architecture reference, not as a crate dependency.

Borrow:

- running result returns continuation handle and no exit code;
- completed result returns exit code and no live handle;
- yield timeout and execution timeout are different;
- list/status/terminate are manager operations over one process store.

### `codex-agent-graph-store`

Decision: no direct use.

Why:

- It is tied to Codex protocol/state.
- AgentDashboard already has AgentRun, LifecycleGate, mailbox and companion ownership concepts.

### `codex-code-mode-protocol`

Decision: no direct use.

Why:

- It is shaped around Codex code-mode cell/process protocol.
- The required module here is a generic AgentRun wait service over existing source roots.

### `codex-terminal-detection`

Decision: not needed for this task.

Why:

- AgentDashboard should render shell facts through its Environment ContextFrame and runtime surface.
- Detecting the default shell is useful as reference, but not enough to justify another direct dependency for wait/exec closure.

## PowerShell Output Implication

Process-level `powershell.exe -Command` renders `pwd` / `Get-ChildItem` object output into stdout text. If an Agent-facing shell tool observes empty output for those commands, the likely fault is an upper-layer object-stream or serialization adapter, not PowerShell itself.

The exec repair should therefore:

- keep byte-stream process execution through `codex-utils-pty`;
- avoid PowerShell SDK object stream execution for `shell_exec`;
- add Windows Environment ContextFrame guidance so agents write stable PowerShell commands when exact text output matters;
- keep dedicated VFS tools preferred for file inspection and search.

## Implementation Consequences

- Reuse `codex-utils-pty` for PTY, pipe, stdin, resize and termination.
- Add `close_stdin` to AgentDashboard shell input/control semantics by calling `ProcessHandle::close_stdin`.
- Keep `terminal_id` as the only public continuation ref. Local shell session ids and Codex process handles remain private.
- Consider removing direct `portable-pty` from `agentdash-local` if no implementation code starts using it directly.
- Do not add `codex-exec-server-protocol` for the wait/exec tasks.
- Do not expand use of `codex-app-server-protocol` into AgentDashboard terminal activity contracts.
