# Design

## Target Model

Terminal / exec wait 的目标不是让 `wait` 变成第二个输出读取工具，而是让 Agent 在等待完成时拿到足够的决策信息：

```text
Terminal / exec state
  = execution authority

wait
  = bounded observer and salvage surface

shell_exec read
  = stdout/stderr body reader

workspace waiting projection
  = UI projection over the same authority

future channel system
  = can map source/cursor/payload refs without parsing text
```

## Authority Map

| Concern | Owner | Contract |
| --- | --- | --- |
| terminal resource lifecycle | terminal registry / terminal cache | running / exited / killed / lost, output sequence, terminal id |
| exec command completion | exec terminal state projection | exit code, completed/failed/cancelled/lost status |
| output body | `shell_exec read` / terminal output store | paged or ranged stdout/stderr read |
| wait result | `WaitActivityService` exec source | bounded status, preview, refs, next read instruction |
| model-visible text | wait tool result renderer | bounded projection only |
| future channel receipt | future channel layer | can consume source refs, cursor, payload refs, diagnostic refs |

RuntimeSession delivery terminal state remains separate from terminal resource state. A RuntimeSession can fail without a PTY terminal resource, and a PTY terminal can be lost without meaning the AgentRun delivery terminal failed.

## Proposed `wait` Exec Item Shape

`wait` should keep the existing generic item frame, but exec items should fill a richer bounded payload:

```json
{
  "activity_ref": "term_abc",
  "kind": "exec",
  "status": "failed",
  "source_ref": "term_abc",
  "correlation_ref": "runtime-session-or-command-ref",
  "preview": "exit 2; stderr: no such file or directory",
  "result_refs": {
    "terminal_id": "term_abc",
    "output_ref": {
      "kind": "terminal_output",
      "terminal_id": "term_abc",
      "after_seq": 42,
      "next_seq": 87
    },
    "diagnostic": {
      "kind": "exec_exit",
      "exit_code": 2
    }
  },
  "exec": {
    "terminal_id": "term_abc",
    "terminal_state": "exited",
    "exit_code": 2,
    "stdout_preview": {
      "text": "last bounded stdout text",
      "bytes": 1024,
      "truncated": true,
      "from": "tail"
    },
    "stderr_preview": {
      "text": "last bounded stderr text",
      "bytes": 2048,
      "truncated": false,
      "from": "tail"
    }
  },
  "next": {
    "tool": "shell_exec",
    "operation": "read",
    "terminal_id": "term_abc",
    "after_seq": 42
  },
  "cursor": "1783000001000"
}
```

The exact field placement can be adjusted to match existing `WaitActivityItem`, but these facts should remain structured. In particular, `exit_code`, stdout preview, stderr preview and read cursor must not be encoded only in `preview`.

## Status Mapping

| Terminal / exec condition | wait item status | Diagnostic |
| --- | --- | --- |
| running / starting | `running` | none or `terminal_running` |
| exited with code 0 | `completed` | `exit_code=0` |
| exited with non-zero code | `failed` | `kind=exec_exit`, `exit_code` |
| killed by user/system | `cancelled` | `kind=terminal_killed` |
| output/resource lost | `lost` | `kind=terminal_lost` |
| state unknown but ref valid | `unknown` | `kind=terminal_state_unknown` |

`completed` should mean successful exit when an exit code is available. A non-zero exit code should be `failed`, because Agent decisions depend on this distinction.

## Preview Contract

Preview exists to reduce unnecessary follow-up reads for small or obvious commands. It is not the output body authority.

Initial proposal:

```text
stdout_preview: tail up to 4 KiB or 200 lines, whichever is smaller
stderr_preview: tail up to 4 KiB or 200 lines, whichever is smaller
preview: one-line bounded summary derived from status + exit_code + stderr tail
```

Each preview carries:

- `text`
- `truncated`
- `bytes`
- `from = tail`

If output is too large, `shell_exec read` remains the only way to retrieve the full body or a selected range.

## Read Cursor Contract

Exec wait item should include a read continuation that is immediately usable by the Agent:

```json
{
  "next": {
    "tool": "shell_exec",
    "operation": "read",
    "terminal_id": "term_abc",
    "after_seq": 42
  }
}
```

Open point for implementation research: current terminal output APIs may use `after_seq`, byte offset, event seq or retained output cursor. The design requirement is stable shape, not a specific cursor name. The final implementation should prefer the existing terminal output sequence owner instead of inventing a parallel cursor.

## Later Wait And Race Semantics

`wait(activity_refs=[terminal_id])` must be able to observe a terminal that completed before the wait call started:

```text
terminal exits
  -> terminal state/output cursor persists
  -> later wait reads terminal state
  -> wait returns completed/failed item + bounded preview + next read refs
```

Timeout does not consume output and does not change terminal state. It only describes the wait call outcome.

If terminal exit and wait timeout happen concurrently, terminal state wins as the execution authority. A later wait should still return the completed/failed state.

## `shell_exec wait_read` / `read_on_complete`

Do not include this in the first implementation unless the existing `shell_exec` tool already has a natural operation slot for it.

Reason:

- It combines launch, wait and read into one convenience operation.
- It can be useful, but it must not make `shell_exec` own a second wait protocol.
- The first step should make `wait` items good enough that the Agent can decide whether to read.

Future shape could be:

```json
{
  "operation": "run",
  "command": "...",
  "wait": {
    "until": "terminal",
    "timeout_ms": 30000,
    "read_on_complete": {
      "stdout_tail_bytes": 4096,
      "stderr_tail_bytes": 4096
    }
  }
}
```

This should internally reuse `WaitActivityService` and terminal output read APIs rather than creating an alternate waiting implementation.

## Channel Compatibility

Future channel system should be able to map terminal wait/read into channel-like facts:

| Current field | Future channel meaning |
| --- | --- |
| `activity_ref` / `terminal_id` | source stream id |
| `source_ref` | producer identity |
| `correlation_ref` | command / runtime / turn correlation |
| `output_ref.kind=terminal_output` | payload stream ref |
| `after_seq` / `next_seq` | channel cursor |
| `stdout_preview` / `stderr_preview` | bounded projection |
| `diagnostic` | delivery/execution diagnostic |
| `next` | suggested follow-up operation, not transport authority |

The important constraint: no consumer should parse the natural-language `preview` to recover exit code, cursor or output refs.

## Validation Plan

- Backend wait exec source tests:
  - running terminal returns `running` and read refs when available;
  - exit code 0 returns `completed`;
  - non-zero exit returns `failed`, `exit_code`, stderr preview;
  - completed-before-wait is salvageable;
  - timeout does not consume state/output.
- Shell read tests:
  - continuation cursor returned by wait can be passed to read;
  - read remains output body owner.
- Frontend/tool-result snapshot tests:
  - wait exec item renders status/exit code/preview without dumping full output;
  - `next` read instruction remains visible/copyable enough for Agent-facing debug.
