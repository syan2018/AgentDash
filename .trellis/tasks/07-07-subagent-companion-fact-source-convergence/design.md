# Design

## Problem Statement

当前问题不是单个 400 错误没有展示出来，而是同一个 SubAgent terminal failure 被多层包装后变成多条近似事实：gate terminal fallback、wait result、parent mailbox wake、conversation input_text、workspace/list refresh signal。修复目标是把这些链路恢复为事实、投递、观察和展示四个边界。

## Authority Model

| Concern | Owner | Reason |
| --- | --- | --- |
| waiting / result state | `LifecycleGate` | gate 已经持有 wait policy、expected result、terminal policy 和 resolved payload，是 companion/subagent 等待的 durable owner。 |
| runtime terminal evidence | RuntimeSession terminal -> AgentRun control effects | terminal 是 delivery trace 事实，只能触发 gate convergence / diagnostics / delivery effects。 |
| parent continuation delivery | AgentRun mailbox envelope | mailbox 提供 dedup、claim、schedule 和 replay，不拥有等待结果本身。 |
| activity waiting | `WaitActivityService` | wait 是 watcher，返回状态、refs 和 bounded preview。 |
| workspace/list UI | Project projection notification + AgentRun workspace/list projection | 前端消费后端 projection 和 project-scoped projection notification，不本地推导 lineage / delivery / waiting。 |
| system/subagent notifications | System/subagent event projection | subagent completion、companion wake、system delivery 是系统来源事件，不是 human input。 |

## Root Cause Map

1. AgentRun list stale
   - Backend protocol has `ControlPlaneProjection::AgentRunList`, but production code does not emit it.
   - Frontend list store refreshes only on project `StateChanged`.
   - Workspace-local stream plans list refresh only when the workspace page is open.

2. Main Agent lacks useful SubAgent failure diagnostics
   - Gate terminal fallback resolves missing companion result with a generic summary.
   - The terminal event currently carries terminal state/message/trace ref, but the provider diagnostic body from the failed child run is not projected into the gate result or result refs in a way `wait` can surface.
   - `wait` correctly observes the gate but can only summarize what the gate payload contains.

3. Duplicate and confusing result sources
   - Gate result is authoritative.
   - Mailbox wake renders that result into `input_text`.
   - `accept_intake_message` injects the wake as `text_user_input_blocks`, so the parent conversation/model sees a textual result notice.
   - `wait=true` and generic `wait` can return the gate status at the same time, so the Agent sees both wait output and mailbox wake text.

## Proposed Shape

### Complete Target State

The desired end state is not "hide duplicate messages". It is a smaller fact model:

```text
LifecycleGate.result
  = the only companion/subagent wait result truth

MailboxEnvelope
  = durable delivery ledger for whether that result should wake or continue another AgentRun

WaitActivity
  = observer that can always read a resolved gate, even after the original waiter ended

Conversation / model context
  = projection of delivery envelopes, preserving companion/system origin

AgentRunList
  = backend read model refreshed by explicit projection invalidation
```

For a blocking request:

```text
companion_request(wait=true)
  -> opens gate
  -> waits for gate result
  -> gate resolves
  -> delivery convergence claims delivered_to_waiter
  -> tool call returns result
  -> no additional parent model continuation is queued for the same result
```

For an async request or vanished waiter:

```text
companion_request(wait=false) or waiter not claimable
  -> opens gate
  -> gate resolves
  -> delivery convergence queues mailbox continuation
  -> parent model receives one companion/system-origin continuation
```

For later recovery:

```text
wait(activity_refs=[gate_id])
  -> reads resolved LifecycleGate.result directly
  -> returns status / diagnostic / result refs
```

The common invariant is that the result is written before delivery convergence. If delivery state is missing after a crash, replay reads the resolved gate and converges delivery. This preserves both "no lost result" and "no duplicate continuation".

The model context should never have to infer source from text such as `Companion child result is available`. It should receive a projection whose structured metadata says it came from companion/system delivery, while the text remains a bounded, human-readable summary derived from the gate result.

Child evidence locators are part of the result refs, but they are not literal child-local lifecycle URIs projected into the parent view. They must identify the child AgentRun/session evidence and a relative evidence kind so the correct resolver can produce the parent-visible view.

### AgentRun List Invalidation

Introduce a generic project-scoped projection notification path, then publish `ControlPlaneProjection::AgentRunList` through it whenever the AgentRun list read model can change.

The notification abstraction should not be named or shaped as an AgentRun-list-only facility. A cleaner shape is:

```text
ProjectProjectionNotificationPort
  -> publish(ProjectProjectionInvalidation {
       project_id,
       projection,
       reason,
       run_id?,
       agent_id?,
       frame_id?,
       gate_id?,
       mailbox_message_id?,
       delivery_runtime_session_id?
     })
  -> API adapter maps to ProjectEventStreamEnvelope::ControlPlaneProjectionChanged
```

This remains a broadcast invalidation hint, not a durable business fact and not a mailbox/channel message. Its payload identifies what should be refetched; it does not carry the projection body. Frontend reconnect / project stream Connected handling should be allowed to refetch visible project projections because a pure broadcast can be missed while disconnected.

Publish `projection=AgentRunList` when these list read-model facts change:

- child lineage created or removed
- cross-run fork/new-root materialized
- child AgentRun delivery binding terminal changes
- AgentRun shell title/activity/status changes
- mailbox/waiting changes that affect list entry activity or badges

Frontend list store should consume a project-scoped invalidation source. The preferred route is to project `ControlPlaneProjectionChanged(projection=AgentRunList)` into the project event stream and bridge it into `agent-run-list-state-store` without requiring a workspace page instance. The existing workspace-local control-plane handling can remain a local refresh optimization, but correctness must come from the project-scoped projection notification path.

The current implementation slice already covers same-run lineage, delivery running/terminal, title, and a check-agent patch for cross-run fork/new-root. It still needs cleanup toward the generic notification abstraction and a producer for run-level `last_activity_at` changes.

### Gate Result Diagnostics

Extend terminal-derived gate result payload with bounded diagnostic fields:

```json
{
  "source": "producer_terminal",
  "status": "failed",
  "summary": "...",
  "terminal_state": "failed",
  "failure_kind": "runtime_terminal_failed | missing_companion_respond",
  "diagnostic": {
    "kind": "provider",
    "code": "invalid_request",
    "http_status": 400,
    "provider": "Codex API",
    "model": "gpt-5.3-codex",
    "message": "The model is not supported when using Codex with a ChatGPT account.",
    "retryable": false
  },
  "result_refs": {
    "runtime_trace": "...",
    "gate_id": "..."
  }
}
```

The exact source of diagnostic extraction should be resolved during implementation by tracing RuntimeSession terminal/error persistence. If the terminal event does not currently carry enough bounded detail, add the smallest typed diagnostic projection at the runtime terminal boundary instead of scraping log strings.

### Wait Tool Result

`WaitActivityService` continues to observe `LifecycleGate`. For resolved gates, `WaitActivityItem.preview` should be derived from gate result status/summary/diagnostic. `result_refs` should point to gate/runtime trace/mailbox wake refs, while `details.items[]` carries structured status. Large result body stays behind refs.

### Mailbox Wake Contract

Mailbox wake remains a delivery effect for parent continuation, but its envelope should carry structured payload and source identity as the primary fact. The text form should be treated as a bounded model instruction/notification projection, not the only result body.

For UI/feed, companion/system wake rows should render as system/companion events, not human user messages. For model continuation, the runtime can still provide a concise textual block, but it should be clearly system-origin and derived from the gate payload.

### Model Context Semantics

Agent-facing continuation is a projection of the mailbox envelope, not the mailbox envelope itself and not the gate result itself. The continuation carries:

- `origin = companion | system | human | hook | routine`
- `source.namespace/kind/source_ref/correlation_ref`
- `delivery_kind = wake | result | request | response`
- `result_ref = gate_id | mailbox_message_id | runtime_trace_ref`
- bounded text generated from the authoritative payload

Human composer input remains the only ordinary user input. Companion/system wake can still continue the parent model turn, but its role/source must be visible to Backbone projection and model context assembly so UI and future agents do not confuse it with a human-authored message.

Codex-native subagent notifications follow the same rule. A payload such as:

```xml
<subagent_notification>
...
</subagent_notification>
```

is not user input, even when it appears in the outer Codex conversation surface. AgentDash should classify it as system/subagent-origin delivery metadata or wait result material before it reaches model-visible conversation assembly. If it needs to be shown, the UI should render it as a system/subagent event; if it needs to be consumed by the Agent, the model context should carry a bounded source discriminant, correlation refs and result refs.

### Duplicate Source Reduction

The single source of truth for result status is `LifecycleGate.payload_json`.

- `wait` reads it.
- workspace waiting projection reads it.
- mailbox wake source metadata points to it.
- parent continuation text is derived from it.
- companion request wait=true returns it only as the blocking call result for that invocation.

No consumer should choose between wait output and mailbox text as competing facts; mailbox text is only delivery notification.

### Waiter / Wake Race Semantics

Gate resolution and result delivery are separate durable transitions. Resolving a gate writes the authoritative result first. Delivery then converges through one of these states:

```text
resolved_gate_result
  -> delivered_to_waiter
  -> queued_for_parent_continuation
  -> dispatched_to_parent
```

`delivered_to_waiter` and `queued_for_parent_continuation` are mutually exclusive for the same result attempt, but the transition must be recoverable:

- If a blocking waiter is still registered/claimable when the gate resolves, mark the result delivered to that waiter and return the result through the tool call.
- If no waiter can be claimed, queue a mailbox continuation.
- If the process crashes after gate resolve but before delivery state is recorded, replay inspects the resolved gate and delivery ledger, then claims either waiter delivery or mailbox continuation.
- A later `wait(activity_refs=[gate_id])` always reads the resolved gate result directly. It does not require the original waiter to still exist.
- Timeouts do not consume the result. A timed-out wait call leaves the gate available for later wait or mailbox continuation policy.

Use a thin durable delivery convergence marker independent of mailbox. It is keyed by `gate_id + result_attempt` and owns only the result-delivery convergence state:

```text
gate_result_delivery_marker
  gate_id
  result_attempt
  status = pending | delivered_to_waiter | queued_for_parent_continuation | dispatched_to_parent
  claim_token / claim_expires_at
  target_ref = waiter_ref | mailbox_message_id | command_receipt_id
  updated_at
```

This marker must not become a second mailbox or result store:

- it does not store the result payload; the payload stays on `LifecycleGate`;
- it does not schedule turns; mailbox/scheduler owns continuation delivery;
- it does not render conversation text; feed/model projection derives text from envelope + gate result;
- it does not model all future message transport; it is only a convergence guard until the future channel system exists.

Mailbox stays clean: it receives a continuation envelope only after convergence decides `queued_for_parent_continuation`. Mailbox status then describes mailbox delivery of that envelope, not whether the original gate result was delivered to a blocking waiter.

### Child Evidence Locators

SubAgent results should expose traceable child evidence locators in addition to status/summary. These locators must not assume that the parent Agent's `lifecycle` mount contains the child journal at the same path as the child Agent's own view. The parent-visible address model needs implementation-phase research.

The payload should carry stable coordinates and relative evidence intent, not guessed absolute VFS URIs:

```json
{
  "result_refs": {
    "gate_id": "...",
    "child_run_id": "...",
    "child_agent_id": "...",
    "child_frame_id": "...",
    "child_delivery_runtime_session_id": "...",
    "evidence": [
      { "kind": "journal", "scope": "child_agent_run", "relative": "journal" },
      { "kind": "events", "scope": "child_lifecycle_surface", "relative": "session/events.json" },
      { "kind": "messages", "scope": "child_lifecycle_surface", "relative": "session/messages" },
      { "kind": "turns", "scope": "child_lifecycle_surface", "relative": "session/turns" },
      { "kind": "tools", "scope": "child_lifecycle_surface", "relative": "session/tools" }
    ]
  }
}
```

These refs do not make lifecycle mount the result authority. They give the parent Agent and UI a stable way to inspect the child run evidence when the bounded summary is insufficient. Before implementation starts, a sub-agent should research the existing AgentRun journal / lifecycle surface resolver and replace the placeholder relative locator shape with the correct product contract.

### Future Channel Compatibility

This task should leave a clean migration path for a future channel system that models message transport across humans, agents, system services, routines, hooks and platform brokers.

The current task does not implement that channel system. It should, however, avoid choices that would make channel migration harder:

- delivery envelopes carry source, target, route, correlation, payload refs and delivery state as structured fields;
- model continuation text is a projection of an envelope, not the envelope's authority;
- mailbox wake, wait result, conversation feed and AgentRun list use typed discriminants instead of parsing message text;
- new field names should describe facts generically enough to become channel message metadata later, such as source identity, delivery kind, result ref and delivery state;
- companion/subagent-specific logic stays at adapter edges where possible, so a future channel core can replace the transport without rewriting gate result authority.
- the thin gate result delivery marker should be easy to retire or map into channel delivery receipts later because it has no payload or scheduling behavior of its own.

The immediate design still uses existing mailbox/gate/wait infrastructure because this task is a convergence repair, not the long-term channel architecture task.

### Error Path Cleanup

The implementation must remove or converge known wrong paths instead of layering the new model beside them:

| Wrong path | Cleanup expectation |
| --- | --- |
| `Producer reached terminal before the expected result was written` as the only user-visible failure fact | Keep it only as protocol/failure-kind summary; preserve provider/runtime diagnostic on the gate result and projections. |
| `Companion child result is available...` as free-form user-like `input_text` authority | Convert to companion/system-origin projection derived from gate result; source and refs remain structured. |
| Blocking `wait=true` result plus async mailbox continuation for the same result | Thin delivery marker makes `delivered_to_waiter` and `queued_for_parent_continuation` mutually exclusive. |
| mailbox row/status carrying gate-result delivery state | Move `delivered_to_waiter` style state to the thin marker; mailbox only tracks continuation envelope delivery. |
| child-local `lifecycle://session/...` exposed as parent-visible result ref | Replace with child evidence locator contract after research. |
| frontend list refresh depending on a currently open workspace stream | Add AgentRun list projection invalidation consumption for the list itself. |
| Codex-native `<subagent_notification>` printed as human/user message | Route it into system/subagent-origin event projection or wait result; never append it as ordinary human composer input. |
| AgentRun list notification implemented as one-off list-specific plumbing | Collapse toward a generic project projection notification port; `agent_run_list` is a projection discriminant, not a bespoke transport. |

No implementation should keep the old path as a fallback unless it is only an internal migration step removed before this task completes. The project is pre-release, so schema/DTO cleanup should prefer the correct model over compatibility.

## Data Flow

```text
Child runtime terminal/error
  -> RuntimeTerminalBoundary
  -> AgentRun control effect
  -> GateProducerTerminalConvergence
  -> LifecycleGate resolved payload + diagnostic refs + child evidence locators
  -> MailboxWake delivery intent
  -> AgentRun mailbox envelope (source=companion/result, payload refs)
  -> parent scheduler/model continuation when needed

LifecycleGate resolved payload
  -> WaitActivityService result
  -> workspace waiting projection
  -> mailbox row / conversation system projection
```

## Validation Strategy

- Rust tests around gate terminal convergence payload diagnostic preservation.
- Rust tests around companion mailbox delivery dedup and structured source/payload retention.
- Wait activity tests for failed gate diagnostic preview/details.
- Race tests for waiter ended / gate resolved / replay cleanup.
- Static or targeted tests that old user-like companion result text is no longer the authority path.
- Frontend model tests for `ControlPlaneProjectionChanged(agent_run_list)` invalidating list store.
- Frontend rendering tests ensuring companion/system wake is not treated as human user input.
