# Technical Design

## Scope

This child task introduces the common waitable activity module and migrates exec, companion/subagent, human response and mailbox wake into one waiting behavior.

## Confirmed Execution Design

Sub-agent review confirmed that the smallest correct implementation is a source-root-first wait module, not a new parallel runtime or a flat tool family.

`wait` is an AgentRun control-plane runtime tool:

```text
agentdash-application::wait_activity
  WaitRuntimeToolProvider
  WaitActivityService
  WaitTool
  source adapters:
    exec terminal source
    LifecycleGate source
    mailbox wake source
```

The implementation module should stay discoverable rather than hiding the tool entrypoint in the module root:

```text
crates/agentdash-application/src/wait_activity/
  mod.rs              # declarations and public re-exports only
  provider.rs         # WaitRuntimeToolProvider and SessionRuntimeToolProvider binding
  tool.rs             # WaitTool, schema handling and RuntimeTool implementation
  service.rs          # WaitActivityService orchestration, polling and scope resolution
  types.rs            # request/result/item/context/error types
  sources/
    exec.rs           # SessionTerminalCache adapter
    lifecycle_gate.rs # LifecycleGate adapter
    mailbox.rs        # AgentRunMailbox adapter
```

This shape keeps four review paths separate: catalog registration, Agent-facing schema, wait orchestration, and
source authority mapping. `mod.rs` can re-export the public provider/service/types, but it should not own the
tool implementation or source adapter logic.

The provider is registered as one independent runtime tool provider in `build_session_runtime_tool_composer`, alongside VFS / workflow / collaboration / task / workspace module providers. It is not part of `VfsToolFactory`, not a companion-private helper, and not split into `wait_exec`, `wait_human`, or other top-level tools.

Runtime tool schema/admission metadata should place `wait` in the existing `collaboration` capability:

```text
capability_key = collaboration
tool_path      = collaboration::wait
source         = platform:collaboration
```

This capability describes structured collaboration requests, responses and activity returns. `wait` does not
grant source capabilities such as shell execution, file access or workflow mutation; it only observes current
AgentRun-scoped waitable sources and returns bounded refs.

First delivery does not introduce a `wait_activities` durable ledger table. Existing source roots are already the durable or canonical facts:

```text
exec activity      -> terminal_id
human activity     -> LifecycleGate(gate_id)
subagent activity  -> LifecycleGate(gate_id)
companion activity -> LifecycleGate(gate_id)
mailbox activity   -> AgentRunMailboxMessage(message_id)
workflow activity  -> runtime node coordinate / LifecycleGate
```

The wait module may mint an activity id later only for a source that has no stable root. This keeps the public continuation surface small and avoids redundant internal indexes.

RuntimeSession remains a delivery/trace ref. The wait tool may resolve `RuntimeSessionExecutionAnchor` from the current `ExecutionContext` to find `run_id + agent_id + frame_id`, but the activity owner is always AgentRun control-plane identity. No `/sessions/*` wait/control endpoint is introduced.

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

For the first implementation:

- `activity_ref == terminal_id` for exec.
- `activity_ref == gate_id` for LifecycleGate-backed human/subagent/companion/workflow waiting.
- `activity_ref == mailbox_message_id` for mailbox wake/result observation.
- `source_ref` uses the same natural source id unless the source has a more specific readable ref.
- `correlation_ref` remains request/dispatch/turn correlation, not a second continuation id.

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

First delivery models `register/update` as adapter observation over existing facts:

- exec registration is already performed by `shell_exec` through `ShellTerminalRegistry` into `SessionTerminalCache`;
- gate registration is `LifecycleGateRepository::create`;
- mailbox wake registration is `AgentRunMailboxRepository::create_message_idempotent` with `MailboxSourceIdentity`;
- terminal state updates come from relay terminal events into `SessionTerminalCache`;
- gate resolution remains `LifecycleGate::resolve` plus existing companion mailbox delivery adapters.

`WaitActivityService` should return the current bounded projection and optionally wait by polling these sources. A future notification layer can replace polling without changing the Agent-facing `wait` contract.

During one wait call, the service keeps the natural activity refs it has already observed and continues resolving those refs explicitly on later polls. This preserves completed gate visibility after a source leaves an “open only” scope projection.

Explicit `activity_refs` are still scoped to the current AgentRun delivery context:

- exec terminal refs must belong to the current delivery runtime session;
- LifecycleGate refs must belong to the current run, allowing same-run child gates to be observed by the parent Agent;
- mailbox message refs must belong to the current run and agent.

## Source Adapters

### Exec

Exec adapter reads status from `TerminalActivity(terminal_id)` and the session control layer built by the first child task. Terminal state changes update activity status and exit code. Output remains in shell retained buffer / output refs; wait returns only preview and next read instruction.

This adapter must depend on AgentDashboard terminal activity/session-control ports, not Codex process handles or `codex-exec-server-protocol` DTOs. `codex-utils-pty` is an implementation detail of the local backend only.

Source details:

```text
SessionTerminalCache.terminal_id -> activity_ref/source_ref
state starting|running           -> running
state exited + exit_code 0       -> completed
state exited + non-zero exit     -> failed
state killed                     -> cancelled
state lost                       -> lost
```

`next` points to `shell_exec`:

```json
{
  "tool": "shell_exec",
  "operation": "read",
  "terminal_id": "term_..."
}
```

If a cursor is available from shell retained output, expose it as `after_seq` / `cursor`. If the wait source only has terminal cache state, return refs and let `shell_exec read` provide the output cursor.

### LifecycleGate

Companion/subagent/human wait uses LifecycleGate as source fact. Gate open is the activity root; gate resolution updates projection and creates result refs. Existing private polling functions become wrappers over WaitService or are removed.

Use the existing kind mapping from AgentRun conversation snapshot:

```text
companion_human_request | orchestration_human_gate -> human
companion_wait + payload.request_type              -> human
companion_wait | companion_wait_blocking
  | companion_wait_follow_up                       -> subagent
companion_parent_request                           -> companion
exec_*                                             -> exec
other                                              -> workflow
```

`wait=true` companion/human flows should call `WaitActivityService.wait(activity_refs=[gate_id])` instead of using private polling as the long-term path. Timeout is a wait-call result only; it does not resolve or close the gate.

### Mailbox

Mailbox remains delivery authority. Wait module observes pending/completed wake envelopes and `MailboxStateChanged`; when an activity result should resume AgentRun, it writes a deduped mailbox envelope and lets scheduler claim/deliver it.

Mailbox wake identity uses existing `MailboxSourceIdentity` and source dedup:

```text
namespace = companion | workflow | wait | exec
kind = result | response | wake | completion
source_ref = terminal_id | gate_id | mailbox_message_id | runtime_node_ref
correlation_ref = request_id | dispatch_id | turn ref
source_dedup_key = mailbox_source_identity_dedup_key(source)
```

`wait` must not drain mailbox messages or directly launch/steer/resume a turn. It only observes mailbox readiness or asks the mailbox service to create an idempotent wake envelope; scheduler remains the launch/steer/resume authority.

### Workflow

Workflow/runtime node activity can be added with the same model after MVP. Runtime node coordinate should act as source root.

## Agent Tool

`wait` input:

```json
{
  "activity_refs": ["term_..."],
  "kinds": ["exec", "human"],
  "timeout_ms": 10000,
  "max_items": 10,
  "after_cursor": "1783000000000"
}
```

Input rules:

- `activity_refs` may contain `terminal_id`, `gate_id`, or `mailbox_message_id`.
- `kinds` filters current AgentRun scope when refs are omitted.
- `timeout_ms` is a bounded wait window and is capped server-side.
- `max_items` limits returned summaries.
- `after_cursor` is the opaque cursor returned by a previous wait call; first delivery treats it as a millisecond activity update cursor.
- Empty `activity_refs` and `kinds` means “observe current AgentRun waitable sources”.

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

Output rules:

- `items[].preview` is bounded.
- `items[].result_refs` contains ids/paths needed to fetch the real result elsewhere.
- `items[].next` names the next command/tool when there is a canonical continuation.
- Large stdout/stderr, human/subagent response bodies, and mailbox payload bodies stay in terminal retained output, gate payload, mailbox payload, lifecycle VFS/cache, or artifacts.

## Projection

Workspace snapshot should expose wait activity rows through or alongside `ConversationWaitingItemView`. The frontend can display kind/status/preview/source and link exec activities to terminal refs.

The projection must not require the frontend to understand private source payloads to decide Agent behavior.

First delivery keeps the existing `ConversationWaitingItemView` contract unless a concrete UI requirement needs generated DTO expansion. For exec rows without a gate, use `wait_id=terminal_id`, `gate_id=terminal_id`, `kind="exec"`, and `source_ref=terminal_id`; the current frontend can already open `terminal://{terminal_id}` from that shape. The API workspace snapshot appends running/starting terminal cache entries for the current delivery runtime session into the conversation waiting projection, while terminal output remains owned by terminal projection.

Generated contract expansion is reserved for adding durable activity-specific fields such as `activity_ref`, `result_refs`, or `next` to workspace snapshots. Agent-facing `wait` result already carries those fields and does not require frontend DTO churn.

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
