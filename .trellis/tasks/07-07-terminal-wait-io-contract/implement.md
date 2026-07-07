# Implementation Plan

## Scope

This task implements the first terminal / exec wait IO contract. The main code path should stay in `WaitActivityService` and its exec source; terminal output ownership remains with the terminal registry/output store and `shell_exec read`.

Avoid expanding mailbox. Terminal wait may align with companion `system_message` projection shape when a system projection is already needed, but this task does not introduce a terminal mailbox queue or channel implementation.

## Slices

### A. Runtime Shape Research

- Inspect `crates/agentdash-application/src/wait_activity/types.rs`.
- Inspect `crates/agentdash-application/src/wait_activity/sources/exec.rs`.
- Inspect `AgentRunTerminalRegistry` and terminal output buffer APIs before adding fields.
- Inspect `shell_exec read/status` details only to reuse existing continuation refs.

Output of this slice: exact available fields for terminal state, exit code, output preview and output cursor.

### B. Wait Exec Item Contract

- Map terminal state into `running | completed | failed | cancelled | lost | unknown`.
- Treat exit code `0` as `completed` and non-zero exit code as `failed`.
- Add structured exec detail for terminal id, terminal state, exit code and bounded stdout/stderr preview when available.
- Add structured `result_refs`:
  - `terminal_id`
  - `source.namespace = "terminal"`
  - `source.kind = "exec"`
  - `source.source_ref = terminal_id`
  - `source.correlation_ref` when an existing runtime/command ref is available
  - `output_ref.kind = "terminal_output"`
  - cursor/seq fields from the existing output owner
  - `diagnostic.kind = "exec_exit" | "terminal_lost" | "terminal_killed" | "terminal_state_unknown"`

### C. Read Continuation

- Return a `next` object for `shell_exec` read continuation when the terminal is readable.
- Prefer the existing cursor/sequence owner; do not invent a parallel cursor.
- If current APIs only expose a byte/base-offset cursor, name it honestly and keep the shape structured.

### D. Optional System Projection

- If implementation touches session/system projection for terminal wait, use a `system_message` payload aligned with companion mailbox delivery:
  - `kind`
  - `origin`
  - `source`
  - `status`
  - `summary`
  - `result_refs` / `output_ref`
- Do not emit `UserInputSubmitted`.
- Do not add terminal wait result body to mailbox rows.

### E. Tests

- Add targeted Rust tests around wait activity exec source:
  - running terminal stays `running`;
  - exit code `0` is `completed`;
  - non-zero exit is `failed` and includes exit diagnostic;
  - cancelled/killed/lost map to non-success statuses;
  - later wait can read an already terminal state;
  - timeout does not consume terminal state/output;
  - bounded preview and read continuation refs are present when the terminal output owner exposes them.

## Non-Goals

- Do not implement `shell_exec wait_read` / `read_on_complete` in this slice.
- Do not move stdout/stderr body authority into `wait`.
- Do not build the future channel system.
- Do not make mailbox store terminal output payloads.

## Coordination Notes

- Current workspace has unrelated dirty files in `crates/agentdash-application-vfs/src/mount_file_discovery.rs` and `crates/agentdash-application-vfs/src/service.rs`; do not edit, stage or revert them.
- If `shell_exec` must change, report why before broadening scope; the preferred implementation is to reuse existing terminal registry/output metadata from `agentdash-application`.
