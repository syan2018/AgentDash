# Technical Design

## Scope

This child task restores the current exec and terminal chain. It does not implement the full common wait module, but it must shape exec results so the wait module can observe the same terminal root later. Exec remains one instruction-style tool surface.

## Current Gap

The Agent can call `shell_exec`, and `shell_exec` can start a real OS shell process through VFS -> mount provider -> relay -> local backend. If the process is still running, the result currently contains `session_id`, `terminal_id` and `next_seq`.

The missing path is above relay/local:

- `MountProvider` currently exposes `exec` but no shell session read/input/terminate/status continuation.
- `VfsToolFactory` registers only `shell_exec`.
- `shell_exec` has no operation envelope for continuation.
- frontend terminal projection exists, but running exec refs are not a complete jump/read contract.

## Backend Design

### Crate Reuse

The local shell process backend should keep using `codex-utils-pty` rather than reimplementing process/PTY control. It already provides pipe and PTY spawn, stdin write, stdin close, resize, terminate and exit observation. These handles stay private to local shell session state.

`codex-utils-output-truncation` may remain a local retained-output helper. Public relay/runtime DTOs must continue to expose AgentDashboard truncation structs, not Codex protocol types.

Do not add `codex-exec-server-protocol` for this child. Its operation names are useful reference material, but its `process_id`, path URI and sandbox semantics are not the AgentDashboard terminal activity boundary.

If no direct `portable_pty::` usage is introduced, remove the redundant `portable-pty` dependency from `agentdash-local`.

### Terminal Activity Root

Use one internal root record for exec/terminal continuation:

```text
TerminalActivity
  terminal_id
  owner_ref
  backend_ref
  local_shell_ref
  cwd
  command_preview
  state
  exit_code
  next_seq
  truncation
  pty_size
  created_at / updated_at / completed_at
```

`terminal_id` is the only Agent-facing continuation ref. `local_shell_ref`, backend placement, mount root and runtime trace refs stay inside this record. In the local backend implementation, `terminal_id` may equal the local shell session id, but the upper layers should not teach the Agent a separate `session_id`.

### Shell Session Control Port

Add an application/SPI bridge for existing relay/local continuation primitives. This bridge is used behind `shell_exec` operations; it is not exposed as multiple top-level Agent tools.

Recommended shape:

```text
ShellSessionControlRequest
  terminal_id
  owner_ref
  after_seq
  wait_ms
  max_bytes

ShellSessionControlResult
  state
  exit_code
  chunks
  next_seq
  truncation
```

Implementation can be either:

- extend the exec-capable mount/provider boundary with shell session control methods; or
- introduce a dedicated `ShellSessionControlPort` resolved by exec-capable mount/provider.

The important boundary is that runtime tools must not call local backend globals directly. They should resolve the same terminal root and backend authority that started the exec session.

### Runtime Tool Operation Model

Expose one tool through the existing runtime tool provider path:

```json
{
  "operation": "start | read | write | terminate | resize | status"
}
```

- `operation=start`: use current command/cwd/timeout shape and return initial bounded output.
- `operation=read`: resolve `terminal_id`, call shell read, return chunks/status/cursor.
- `operation=write`: resolve `terminal_id`, write stdin or close stdin, return follow-up read.
- `operation=terminate`: resolve `terminal_id`, terminate and return terminal state.
- `operation=resize`: resolve `terminal_id`, resize PTY terminal.
- `operation=status`: resolve `terminal_id`, return state, exit code and next cursor.

`operation=read` with `wait_ms > 0` can provide a bounded wait for output/terminal state. This is not the final common `wait` tool.

### PowerShell Context

Update Environment ContextFrame Windows note to include:

- real OS shell is PowerShell;
- PowerShell command composition should use PowerShell syntax;
- bash-only `&&` / `|| true` should not be generated;
- object-returning commands need explicit text output for stable non-interactive capture;
- examples using `Write-Output (Get-Location).Path` and `Get-ChildItem | ForEach-Object { Write-Output $_.FullName }`;
- dedicated VFS file tools remain preferred for inspect/read/search.

## Frontend Design

Frontend should keep projection-only responsibility:

- terminal output/state still comes from Backbone `terminal_output` / `terminal_state_changed`;
- terminal store remains keyed by terminal id under the runtime trace projection it belongs to;
- command execution or waiting rows link to `terminal_id`;
- opening a terminal should use projected refs, not a new `/sessions/*` command/control API.

If a new backend contract is needed for terminal refs, put it on AgentRun workspace or runtime tool result DTOs.

## Tests

- Runtime tool catalog keeps one `shell_exec` tool and does not add flat continuation tool names.
- Local shell manager read/input/terminate semantics are preserved.
- Local shell manager exposes close-stdin by calling the existing `codex-utils-pty` process handle, not by adding another public tool.
- VFS/relay bridge tests cover `operation=read|write|terminate|resize|status`.
- `shell_exec operation=start` running result includes public `terminal_id` and does not instruct Agent to use `session_id`.
- Windows Environment ContextFrame contains PowerShell shell kind and examples only on Windows.
- Frontend terminal projection/open tests cover running terminal output/state.
- Search confirms no new `/sessions/*` control endpoint.
