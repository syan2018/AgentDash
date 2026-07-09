# Implementation Plan

## Ordered Steps

1. Read parent task artifacts and required specs.
2. Inspect current VFS exec and relay/local shell code immediately before editing.
3. Add shell session control bridge from runtime tool layer to relay/local continuation primitives.
4. Add canonical `terminal_id` / terminal record owner validation around running `shell_exec` results.
5. Add `shell_exec` operation handling: read, write, terminate, resize, status.
6. Add close-stdin support through the existing local process handle.
7. Remove redundant direct `portable-pty` dependency if implementation continues to use only `codex-utils-pty`.
8. Add or update result formatting and structured details.
9. Strengthen Windows Environment ContextFrame note and tests.
10. Repair frontend terminal projection/open path using existing terminal events and refs.
11. Run focused backend and frontend tests.
12. Grep for `/sessions` in changed code and confirm no new control path.

## Validation Commands

Use focused commands first, then broaden if touched code crosses boundaries:

```powershell
cargo test -p agentdash-application-runtime-session environment_context_frame
cargo test -p agentdash-local shell_session_manager
cargo test -p agentdash-application-vfs shell
pnpm --filter app-web test -- useTerminalStore
pnpm --filter app-web test -- MailboxMessageRow
rg -n "/sessions" crates packages
```

Exact test names may need adjustment after inspecting local package test targets.

## Risk Points

- `MountProvider` currently ends at `exec`; adding continuation must be done at the same authority boundary that starts the process.
- `terminal_id` should be the public and application-level root. Backend/local shell refs stay inside terminal record metadata.
- `wait_ms` for read is a yield/read window, not execution timeout.
- `close_stdin` and PTY resize are terminal protocol semantics, not separate tools.
- `codex-utils-pty` stays behind local shell manager state; Codex process handles do not cross the application boundary.
- frontend terminal store is not a source of truth for Agent reads.

## Done

This child task is done when an Agent can use one `shell_exec` tool to start a long-running shell command, read new output, write stdin, terminate it, inspect final status, and see the terminal projection in the frontend without relying on `/sessions/*` control endpoints.
