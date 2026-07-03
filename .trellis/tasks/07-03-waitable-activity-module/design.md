# Technical Design

## Scope

This child task introduces the common waitable activity module and migrates exec, companion/subagent, human response and mailbox wake into one waiting behavior.

## Owner Boundary

Wait activity belongs to the AgentRun application/control plane, but it should not create redundant durable IDs when a source already has a stable root.

Use a compact owner ref:

```text
owner_ref = run_id + agent_id + frame_id
```

Delivery/runtime trace refs may be attached as metadata for routing or diagnostics, but they are not the owner and should not become parallel public indexes.

Source roots:

- exec: `TerminalActivity(terminal_id)`
- companion/subagent/human: `LifecycleGate(gate_id)`
- mailbox: `AgentRunMailboxMessage(message_id)`
- workflow: runtime node coordinate

For exec, `terminal_id` is the canonical source and activity ref. Local shell session ids and runtime trace refs stay internal to the terminal record.

## Activity Projection Model

Recommended projection fields:

```text
activity_ref
owner_ref
kind: exec | companion | subagent | human | mailbox | workflow
source_ref
correlation_ref
status: pending | running | completed | failed | cancelled | timed_out | lost
preview
result_refs
cursor
source_dedup_key
created_at
updated_at
resolved_at
```

`activity_ref` should use the source natural id where possible:

- exec activity ref = `terminal_id`
- LifecycleGate-backed activity ref = `gate_id`
- mailbox activity ref = `mailbox_message_id`

The wait module should only mint a separate activity id for sources that have no stable root.

## Wait Service

Responsibilities:

- observe/register source roots;
- update source state/result;
- wait on activity refs or kind filters with timeout;
- return bounded summary/ref;
- notify in-process waiters;
- project activity state for workspace snapshot;
- delegate continuation to mailbox scheduler via wake envelopes.

`wait` timeout means “no relevant activity became ready in this call”. It must not cancel or terminate the activity.

## Source Adapters

### Exec

Exec adapter reads status from `TerminalActivity(terminal_id)` and the session control layer built by the first child task. Terminal state changes update activity status and exit code. Output remains in shell retained buffer / output refs; wait returns only preview and next read instruction.

This adapter must depend on AgentDashboard terminal activity/session-control ports, not Codex process handles or `codex-exec-server-protocol` DTOs. `codex-utils-pty` is an implementation detail of the local backend only.

### LifecycleGate

Companion/subagent/human wait uses LifecycleGate as source fact. Gate open is the activity root; gate resolution updates projection and creates result refs. Existing private polling functions become wrappers over WaitService or are removed.

### Mailbox

Mailbox remains delivery authority. Wait module observes pending/completed wake envelopes and `MailboxStateChanged`; when an activity result should resume AgentRun, it writes a deduped mailbox envelope and lets scheduler claim/deliver it.

### Workflow

Workflow/runtime node activity can be added with the same model after MVP. Runtime node coordinate should act as source root.

## Agent Tool

`wait` input:

```json
{
  "activity_refs": ["term_..."],
  "kinds": ["exec", "human"],
  "timeout_ms": 10000,
  "max_items": 10
}
```

`wait` output:

```json
{
  "status": "ready",
  "timed_out": false,
  "items": [
    {
      "activity_ref": "term_...",
      "kind": "exec",
      "status": "completed",
      "source_ref": "term_...",
      "correlation_ref": "...",
      "preview": "...",
      "result_refs": {},
      "next": {
        "tool": "shell_exec",
        "operation": "read",
        "terminal_id": "term_...",
        "cursor": 12
      }
    }
  ]
}
```

## Projection

Workspace snapshot should expose wait activity rows through or alongside `ConversationWaitingItemView`. The frontend can display kind/status/preview/source and link exec activities to terminal refs.

The projection must not require the frontend to understand private source payloads to decide Agent behavior.

## Mailbox Wake

Use stable source identity:

```text
namespace = exec | companion | wait
kind = completion | response | wake
source_ref = terminal_id | gate_id | mailbox_message_id | runtime_node_ref
correlation_ref = request_id / dispatch_id / turn ref
source_dedup_key = stable hash of source identity
```

Wake payload should include only bounded preview and refs. Large result body stays in gate payload, exec output buffer, artifact or mailbox retained payload according to source policy.

## Tests

- wait service state transitions.
- wait timeout without cancellation.
- exec activity registration and terminal completion update.
- LifecycleGate open/resolved adapter for companion/human/subagent.
- mailbox wake dedup and scheduler boundary.
- runtime tool catalog includes `wait`.
- frontend projection consumes generated waiting/activity contract.
