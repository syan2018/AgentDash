# Research: runtime-diagnostic-propagation

- Query: runtime diagnostic propagation for SubAgent/provider fatal failures
- Scope: internal
- Date: 2026-07-07

## Findings

### 1. Summary recommendation

Provider fatal diagnostics already exist close to the connector and agent loop, but the chain loses structure at the Backbone `ErrorNotification`, RuntimeSession terminal evidence, AgentRun terminal convergence/effect replay, and LifecycleGate fallback boundaries. The minimal fix should introduce one bounded, typed terminal diagnostic payload and propagate it as data, not by parsing provider error text or `additional_details`.

Recommended canonical fields:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeTerminalDiagnostic {
    pub kind: String,
    pub code: Option<String>,
    pub http_status: Option<u16>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub message: String,
    pub retryable: bool,
}
```

The bounded diagnostic should be present on:

- the connector/agent-loop error source, preferably before conversion to Backbone DTOs;
- RuntimeSession terminal evidence and `AgentRunTerminalControlInput`;
- AgentRun convergence effects, wait-producer terminal effects, and terminal delivery binding for replay/boot reconciliation;
- `GateProducerTerminalEvent` and the `LifecycleGate.result` fallback payload;
- wait output details and mailbox/system projection source payloads.

The highest-value invariant is: a Codex API 400 `invalid_request` fatal failure should reach `LifecycleGate.result`, `wait` details, and mailbox/system projection as structured `{ kind, code, http_status, provider, model, message, retryable }`, while the human preview can still show a concise summary.

### 2. Existing chain with file:line anchors

1. Provider failure classification is created at the bridge boundary.

- `crates/agentdash-agent/src/bridge.rs:61` defines `BridgeError::Provider { message, classification }`.
- `crates/agentdash-agent/src/bridge.rs:84` defines `ProviderErrorClassification` with `kind`, `http_status`, `provider_code`, `retry_after_ms`, and `safe_to_retry_before_visible_delta`.
- `crates/agentdash-agent/src/bridge.rs:92` provides retryable/fatal/aborted classification constructors, and `crates/agentdash-agent/src/bridge.rs:116` / `crates/agentdash-agent/src/bridge.rs:124` attach HTTP status and provider code.
- `crates/agentdash-agent/src/bridge.rs:185` constructs provider errors and `crates/agentdash-agent/src/bridge.rs:197` exposes their classification.

2. Codex API non-2xx responses are classified as fatal provider errors.

- `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_codex_responses_bridge.rs:94` reads the non-2xx body, derives a display body, classifies the status/body, and returns `BridgeError::provider(format!("Codex API 返回 {status}: {display_body}"), classification)`.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_codex_responses_bridge.rs:112` applies Codex-specific classification; usage-limit/rate-limit cases are fatal with HTTP status and provider code.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs:230` maps HTTP 400 to `invalid_request`.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs:242` extracts provider code/type from JSON bodies.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs:265` treats context and invalid-request provider codes as fatal.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs:393` tests that context and invalid-request bodies are fatal.

3. The agent loop emits structured run errors, but provider/model are not carried on the run error.

- `crates/agentdash-agent/src/types.rs:75` defines `AgentEvent::ProviderAttemptStatus`.
- `crates/agentdash-agent/src/types.rs:79` defines `AgentEvent::RunError`.
- `crates/agentdash-agent/src/types.rs:118` defines `ProviderAttemptStatus` with `reason_code`, `message`, `provider`, and `model`.
- `crates/agentdash-agent/src/types.rs:138` defines `AgentRunErrorKind::Provider`.
- `crates/agentdash-agent/src/types.rs:148` defines `AgentRunError` with `kind`, `message`, `code`, `retryable`, `aborted`, `http_status`, and `details`.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:916` emits `AgentEvent::RunError` for provider failures with `kind=Provider`, `code`, `retryable`, `aborted`, and `http_status`.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:975` emits failed `ProviderAttemptStatus` with a `reason_code` and message, but `provider` and `model` are currently `None`.
- `crates/agentdash-agent/src/agent_loop/streaming.rs:999` derives provider reason code from `provider_code` or `http_status`.

4. Backbone currently carries provider status and error notification on separate lossy surfaces.

- `crates/agentdash-agent-protocol/src/backbone/event.rs:63` models runtime errors as `BackboneEvent::Error(codex::ErrorNotification)`, reusing the Codex app-server DTO.
- `crates/agentdash-agent-protocol/src/backbone/platform.rs:31` models `PlatformEvent::ProviderAttemptStatus`.
- `crates/agentdash-agent-protocol/src/backbone/platform.rs:116` gives provider attempt status `provider` and `model` fields.
- `packages/app-web/src/generated/backbone-protocol.ts:162` shows generated `ErrorNotification = { error, willRetry, threadId, turnId }`.
- `packages/app-web/src/generated/backbone-protocol.ts:464` shows generated `TurnError = { message, codexErrorInfo, additionalDetails }`; it has no structured `kind`, `code`, `http_status`, `provider`, `model`, or `retryable`.
- `packages/app-web/src/generated/backbone-protocol.ts:283` shows generated `ProviderAttemptStatus` includes `provider` and `model`, but not `http_status` or `retryable`.

5. The Pi stream mapper flattens run-error structure into Codex-shaped notification details.

- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:206` preserves `ProviderAttemptStatus.provider` and `.model` if they are present.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:224` maps `AgentRunError` to `codex::ErrorNotification`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:238` builds `TurnError { message, codex_error_info, additional_details }`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:257` maps HTTP status/code to a coarse `CodexErrorInfo`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:285` stores `kind`, `code`, `http_status`, and `retryable=true` as newline-separated text in `additional_details`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:873` maps `AgentEvent::ProviderAttemptStatus` to `PlatformEvent::ProviderAttemptStatus`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:883` maps `AgentEvent::RunError` to `BackboneEvent::Error`.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:1054` also maps assistant message-end errors into `BackboneEvent::Error`, using a less specific unknown run-error kind.
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:700` verifies provider/model mapping for provider attempt status.
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:750` verifies provider run error mapping only as `additional_details` text such as `kind=Provider`, `code=auth_error`, and `http_status=401`.

6. RuntimeSession terminal processing only preserves terminal message.

- `crates/agentdash-application-runtime-session/src/session/hub_support.rs:70` builds a `turn_terminal` `SessionMetaUpdate` with `terminal_type`, `message`, and timing fields.
- `crates/agentdash-application-runtime-session/src/session/hub_support.rs:131` parses terminal events from envelopes and returns only `(turn_id, TurnTerminalKind, Option<String>)`.
- `crates/agentdash-application-runtime-session/src/session/hub_support.rs:471` exposes failed/interrupted/lost runtime execution state using only `meta.last_terminal_message`.
- `crates/agentdash-application-runtime-session/src/session/launch/ingestion.rs:75` converts a parsed terminal envelope into `TurnEvent::Terminal { kind, message }` and stops ingestion.
- `crates/agentdash-application-runtime-session/src/session/turn_processor.rs:318` builds `RuntimeTerminalBoundaryEvidence` with terminal state and terminal message only.
- `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs:13` defines `RuntimeTerminalBoundaryEvidence` without a diagnostic field.
- `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs:39` derives terminal state and sends `AgentRunTerminalControlInput`.
- `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs:75` passes only `terminal_message` into AgentRun terminal control.
- `crates/agentdash-application-runtime-session/src/session/turn_processor.rs:684` tests the current terminal control input shape with terminal message only.

7. Session persistence stores only last terminal message/status.

- `crates/agentdash-infrastructure/src/persistence/session_core.rs:49` maps session metadata fields including `last_delivery_status` and `last_terminal_message`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:666` defines `SessionProjection` with `last_terminal_message`, but no terminal diagnostic.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:692` projects `TurnStarted`, `TurnCompleted`, `BackboneEvent::Error`, and `turn_terminal` into status/message updates.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:708` sets `last_terminal_message` from `TurnCompleted.turn.error.message`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:720` sets `last_delivery_status="failed"` and `last_terminal_message=e.error.message` for `BackboneEvent::Error`.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs:725` handles `turn_terminal` meta events and stores only `value["message"]`.

8. AgentRun control effects and terminal convergence have no diagnostic slot.

- `crates/agentdash-application-ports/src/agent_run_control_effect.rs:75` defines `AgentRunTerminalControlInput` with terminal state and message only.
- `crates/agentdash-application-ports/src/agent_run_control_effect.rs:103` defines `AgentRunWaitProducerTerminalEvent` with terminal state, terminal message, source turn ID, and delivery trace ref only.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:221` persists delivery convergence effect payload with `terminal_state` and `terminal_message` only.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:376` constructs `AgentRunWaitProducerTerminalEvent` without a diagnostic.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:480` deserializes and executes delivery convergence using only terminal state/message.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:499` deserializes wait terminal convergence events, again with no diagnostic field.
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs:23` defines `AgentRunRuntimeTerminalCommand` without a diagnostic.
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs:31` defines `AgentRunDeliveryTerminalEvent` without a diagnostic.
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs:83` converges runtime terminal state into delivery state using only terminal state/message.
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs:137` replays terminal binding with only `binding.terminal_state` and `binding.terminal_message`.

9. Boot reconciliation can only reconstruct lossy terminal events.

- `crates/agentdash-application/src/reconcile/boot.rs:414` reconstructs `GateProducerTerminalEvent` from terminal binding with `terminal_state`, `terminal_message`, `source_turn_id`, and `trace_ref`.
- `crates/agentdash-application/src/reconcile/boot.rs:667` tests boot reconciliation with terminal message `provider failed`, not a structured diagnostic.

10. Gate fallback writes a generic terminal result.

- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:17` defines `GateProducerTerminalEvent` with producer, terminal state, terminal message, source turn ID, and trace ref only.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:211` builds fallback result payload.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:215` hard-codes summary as `Producer reached terminal before the expected result was written.`
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:217` stores `terminal_message`.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:219` stores `delivery_trace_ref`.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:222` stores `failure_kind`.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:223` stores `source="producer_terminal"`.

11. Wait output consumes gate projection/result refs but has no diagnostic field.

- `crates/agentdash-application/src/wait_activity/types.rs:112` defines `WaitActivityItem` with preview and result refs, but no terminal diagnostic field.
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:10` builds wait items from `LifecycleGate::waiting_projection()`.
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs:18` sets `result_refs` to gate/run/frame identity fields only.
- `crates/agentdash-application/src/wait_activity/tool.rs:52` renders wait output text from item kind/status/activity ref/preview.
- `crates/agentdash-application/src/wait_activity/tool.rs:88` includes item details JSON, so any future structured field on items or result refs can survive the wait tool response.
- `crates/agentdash-application/src/wait_activity/tests.rs:160` verifies resolved gate payload status/summary drive wait status and preview.
- `crates/agentdash-application/src/wait_activity/tests.rs:207` verifies wait and workspace gate projection share kind/preview/status.

12. Mailbox/system projection currently depends on gate payload/free-text paths.

- `crates/agentdash-application/src/gate_wait_policy.rs:211` executes mailbox wake delivery for gate terminal/resolution intents in the application adapter layer.
- `crates/agentdash-application/src/companion/gate_control.rs:1136` is the companion gate-control path identified by the task context for gate terminal result handling.
- `crates/agentdash-application/src/companion/gate_control.rs:2459` tests terminal message propagation through existing companion gate control behavior.
- `crates/agentdash-application/src/companion/tools.rs:519` / `crates/agentdash-application/src/companion/tools.rs:523` / `crates/agentdash-application/src/companion/tools.rs:527` are the task-identified mailbox/system text construction anchors. The diagnostic should be read from the gate payload/source refs there rather than reconstructed from human text.

### 3. Missing diagnostic fields / lossy boundaries

The lossy boundaries are:

1. `AgentRunError` has most provider-failure fields but lacks `provider` and `model`.

`ProviderAttemptStatus` has `provider` and `model`, but the current fatal-error `RunError` does not. The current Codex bridge has enough local context to attach a provider label and model identifier, but that identity is not included in `AgentRunError` or the later terminal path.

2. `ProviderAttemptStatus` has provider/model fields, but the current emitted values are mostly `None`.

The mapper can preserve provider/model, and tests prove it, but the streaming loop emits `None` in provider status events. This makes provider/model unavailable to downstream observers unless added to the run error diagnostic or populated at source.

3. `BackboneEvent::Error` is a Codex DTO boundary.

`codex::ErrorNotification` and generated `TurnError` expose `message`, `codexErrorInfo`, and `additionalDetails`. The mapper flattens structured details into newline text. This is not a reliable source for `LifecycleGate.result`, wait output, or mailbox/system projection.

4. RuntimeSession terminal event parsing drops all fields except kind/message.

`parse_turn_terminal_event_from_envelope` returns only `TurnTerminalKind` and optional message. `RuntimeTerminalBoundaryEvidence` and `AgentRunTerminalControlInput` therefore cannot carry diagnostic data even if Backbone receives it.

5. Session meta/projection persists only `last_terminal_message`.

`SessionMeta` and `SessionProjection` do not have a terminal diagnostic field. This affects runtime state APIs and any restart/replay path that uses persisted session projection instead of live envelopes.

6. AgentRun control effect payloads are a durable replay boundary and currently omit diagnostics.

`agent_run_delivery_convergence` and `wait_producer_terminal_convergence` serialize terminal state/message only. If a diagnostic is added only to live memory and not these payloads, recovery or effect replay will still produce lossy gate output.

7. AgentRun delivery binding is the boot-reconcile source and currently stores terminal message only.

Boot reconciliation constructs `GateProducerTerminalEvent` from the terminal binding. Without a persisted diagnostic on the binding, restart will recreate only the generic fallback.

8. `GateProducerTerminalEvent` and `producer_terminal_result_payload` have no diagnostic field.

The gate fallback currently creates a valid failed result, but its summary is generic and the only failure-specific data is `terminal_message`.

9. Wait items expose `result_refs` but not a diagnostic.

Wait details can preserve structured JSON if it is added to `WaitActivityItem` or `result_refs`, but the current lifecycle-gate source does not read a diagnostic from the gate payload.

10. Mailbox/system projection must not parse prose.

Mailbox wake and parent-continuation text can include a concise human message, but structured diagnostics should come from the gate result payload/source refs. Otherwise the terminal result remains hard to classify programmatically.

### 4. Proposed minimal contract shape

Use one bounded diagnostic object across the terminal path:

```json
{
  "kind": "provider",
  "code": "invalid_request",
  "http_status": 400,
  "provider": "Codex API",
  "model": "gpt-5.3-codex",
  "message": "Codex API returned 400: ...",
  "retryable": false
}
```

Field notes:

- `kind`: bounded source category such as `provider`, `runtime`, `hook`, `producer_terminal`, or `unknown`. For the target failure, use `provider`.
- `code`: provider or runtime stable code, e.g. `invalid_request`.
- `http_status`: numeric provider HTTP status when applicable.
- `provider`: bounded provider label from connector/runtime context, not scraped from prose.
- `model`: model identifier from bridge/request context.
- `message`: sanitized bounded diagnostic message suitable for internal UI/projection.
- `retryable`: false for fatal provider failures and true only when the producer says retry is valid.

Recommended propagation points:

1. Source construction:

- Add provider/model to the source diagnostic in the bridge or agent loop while building `AgentEvent::RunError`.
- Prefer a typed field over `details` or formatted message parsing.

2. Backbone/session boundary:

- Either extend the Backbone protocol with a typed AgentDash error/terminal diagnostic event or include `diagnostic` on the `turn_terminal` `SessionMetaUpdate` payload.
- Do not rely on `TurnError.additionalDetails`; it is currently text and generated TypeScript confirms the shape is not diagnostic-safe.

3. RuntimeSession terminal boundary:

- Add `diagnostic: Option<RuntimeTerminalDiagnostic>` to `RuntimeTerminalBoundaryEvidence`.
- Add matching optional diagnostic to `AgentRunTerminalControlInput`.
- Persist or project it in session metadata if runtime APIs or restart paths need to expose the same information.

4. AgentRun convergence and replay:

- Add optional diagnostic to the delivery convergence effect payload.
- Add optional diagnostic to `AgentRunRuntimeTerminalCommand`.
- Add optional diagnostic to `AgentRunDeliveryTerminalEvent`.
- Add optional diagnostic to `AgentRunWaitProducerTerminalEvent`.
- Persist optional diagnostic on the terminal delivery binding used by `converge_terminal_binding` and boot reconciliation.

5. Gate fallback:

- Add `diagnostic: Option<...>` to `GateProducerTerminalEvent`.
- Include `diagnostic` in `producer_terminal_result_payload`.
- Keep `source="producer_terminal"` and `failure_kind`, but allow `summary` to prefer the diagnostic message or a concise diagnostic-aware summary over the generic fallback.
- Add `result_refs.runtime_trace` / `result_refs.gate_id` if the implementation wants stable trace references; at minimum preserve existing `delivery_trace_ref`, `resolved_turn_id`, and the structured diagnostic.

6. Wait and mailbox/system projection:

- In `wait_activity`, read `diagnostic` from resolved gate payload and expose it either as `WaitActivityItem.diagnostic` or under `result_refs.diagnostic`.
- Keep the text preview short, but preserve the full bounded diagnostic in details.
- In mailbox/system projection, read from gate payload/source refs and include the diagnostic in structured source metadata. Human-facing text should summarize it, not become the only source of truth.

Minimal resulting gate payload:

```json
{
  "source": "producer_terminal",
  "status": "failed",
  "summary": "Codex API returned 400 invalid_request.",
  "terminal_state": "failed",
  "terminal_message": "Codex API returned 400: ...",
  "failure_kind": "runtime_terminal_failed",
  "diagnostic": {
    "kind": "provider",
    "code": "invalid_request",
    "http_status": 400,
    "provider": "Codex API",
    "model": "gpt-5.3-codex",
    "message": "Codex API returned 400: ...",
    "retryable": false
  },
  "result_refs": {
    "runtime_trace": "...",
    "gate_id": "..."
  }
}
```

### 5. Test targets

1. Connector/agent-loop diagnostic source:

- Unit/integration test that Codex API 400 `invalid_request` becomes a fatal provider diagnostic with `kind=provider`, `code=invalid_request`, `http_status=400`, `retryable=false`, plus provider/model.
- Existing useful anchors: `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs:393`, `crates/agentdash-agent/src/agent_loop/streaming.rs:916`.

2. Backbone mapping:

- Test that the typed diagnostic is emitted through the selected Backbone/session surface and is not only present in `TurnError.additional_details`.
- Existing anchors: `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs:750`, `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs:224`.

3. RuntimeSession terminal evidence:

- Test that an error/terminal envelope produces `RuntimeTerminalBoundaryEvidence` and `AgentRunTerminalControlInput` with the diagnostic intact.
- Existing anchors: `crates/agentdash-application-runtime-session/src/session/turn_processor.rs:318`, `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs:39`, `crates/agentdash-application-runtime-session/src/session/turn_processor.rs:684`.

4. Session projection/persistence:

- Test that session metadata/projection can retain the latest terminal diagnostic if runtime APIs or restart paths read session state.
- Existing anchors: `crates/agentdash-infrastructure/src/persistence/session_core.rs:49`, `crates/agentdash-infrastructure/src/persistence/session_core.rs:692`.

5. AgentRun effect replay:

- Test that delivery convergence and wait-producer terminal convergence serialize/deserialize the diagnostic in effect payloads.
- Existing anchors: `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:221`, `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:376`, `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs:499`.

6. AgentRun terminal binding and boot reconciliation:

- Test that terminal diagnostic persists on delivery binding and boot reconcile reconstructs `GateProducerTerminalEvent` with the same diagnostic.
- Existing anchors: `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs:137`, `crates/agentdash-application/src/reconcile/boot.rs:414`.

7. Gate fallback result:

- Test that `producer_terminal_result_payload` includes `diagnostic`, preserves `failure_kind`, and does not overwrite an already-written normal producer result.
- Existing anchors: `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs:211`.

8. Wait output:

- Test that a resolved failed lifecycle gate exposes the diagnostic in wait details and uses a diagnostic-aware preview.
- Existing anchors: `crates/agentdash-application/src/wait_activity/tests.rs:160`, `crates/agentdash-application/src/wait_activity/tests.rs:207`.

9. Mailbox/system projection:

- Test that mailbox wake / parent-continuation source metadata carries the diagnostic from the gate payload instead of only free text.
- Existing anchors: `crates/agentdash-application/src/gate_wait_policy.rs:211`, `crates/agentdash-application/src/companion/gate_control.rs:2459`, `crates/agentdash-application/src/companion/tools.rs:519`.

### Files found

- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/prd.md` - task requirement for preserving provider fatal diagnostics through gate/wait/mailbox projections.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/design.md` - target terminal diagnostic payload and convergence design.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/implement.md` - implementation slice requesting this runtime diagnostic propagation research.
- `.trellis/tasks/07-07-subagent-companion-fact-source-convergence/implement.jsonl` - implementation context entries for backend/session/workflow/backbone.
- `crates/agentdash-agent/src/bridge.rs` - provider error and classification source types.
- `crates/agentdash-agent/src/types.rs` - agent event, provider status, and run-error types.
- `crates/agentdash-agent/src/agent_loop/streaming.rs` - provider failure event emission.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_codex_responses_bridge.rs` - Codex API response failure classification.
- `crates/agentdash-executor/src/connectors/pi_agent/bridges/mod.rs` - generic HTTP provider failure classification helpers.
- `crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs` - AgentEvent to Backbone envelope mapping.
- `crates/agentdash-executor/src/connectors/pi_agent/connector_tests.rs` - current Backbone mapping tests.
- `crates/agentdash-agent-protocol/src/backbone/event.rs` - Backbone event enum.
- `crates/agentdash-agent-protocol/src/backbone/platform.rs` - platform provider attempt status.
- `packages/app-web/src/generated/backbone-protocol.ts` - generated web-facing Backbone DTO shape.
- `crates/agentdash-application-runtime-session/src/session/hub_support.rs` - terminal envelope construction/parsing and runtime state projection.
- `crates/agentdash-application-runtime-session/src/session/launch/ingestion.rs` - stream ingestion terminal event handling.
- `crates/agentdash-application-runtime-session/src/session/turn_processor.rs` - terminal evidence creation.
- `crates/agentdash-application-runtime-session/src/session/terminal_boundary.rs` - RuntimeSession to AgentRun terminal boundary.
- `crates/agentdash-infrastructure/src/persistence/session_core.rs` - session projection and metadata persistence.
- `crates/agentdash-application-ports/src/agent_run_control_effect.rs` - AgentRun control effect DTOs.
- `crates/agentdash-application-agentrun/src/agent_run/control_effects.rs` - effect insertion and replay.
- `crates/agentdash-application-agentrun/src/agent_run/terminal_convergence.rs` - terminal convergence and binding replay.
- `crates/agentdash-application/src/reconcile/boot.rs` - boot-time gate terminal reconstruction.
- `crates/agentdash-application-workflow/src/gate/gate_wait_policy.rs` - gate producer terminal fallback result.
- `crates/agentdash-application/src/wait_activity/types.rs` - wait activity item DTO.
- `crates/agentdash-application/src/wait_activity/sources/lifecycle_gate.rs` - lifecycle gate wait item projection.
- `crates/agentdash-application/src/wait_activity/tool.rs` - wait tool result rendering.
- `crates/agentdash-application/src/wait_activity/tests.rs` - wait projection tests.
- `crates/agentdash-application/src/gate_wait_policy.rs` - application gate wait policy adapter and mailbox wake delivery path.
- `crates/agentdash-application/src/companion/gate_control.rs` - companion gate control terminal handling tests and flow.
- `crates/agentdash-application/src/companion/tools.rs` - parent continuation/mailbox text construction anchors.

### Code patterns

- Provider failure classification is already structured at source: `BridgeError::Provider` plus `ProviderErrorClassification`.
- Backbone currently splits provider diagnostics between structured provider-attempt status and Codex-shaped error notification; only the latter drives terminal failure projection.
- `additional_details` is used as diagnostic text, but no downstream code should rely on parsing it.
- Terminal propagation repeatedly uses `{ terminal_state, terminal_message }` pairs, making it straightforward to add sibling `diagnostic: Option<...>` fields at each boundary.
- AgentRun control effects and delivery binding are the critical durable replay boundaries; any field missing there will be missing after recovery.
- Wait output can already carry structured JSON in details; it needs a populated diagnostic source from the gate result or item refs.

### External references

- None. This research is based on local task documents, local Trellis specs, generated protocol files, and repository code.

### Related specs

- `.trellis/spec/backend/index.md` - backend spec index and package conventions.
- `.trellis/spec/backend/error-handling.md` - structured error and diagnostic expectations.
- `.trellis/spec/backend/diagnostics-guidelines.md` - bounded diagnostic/logging guidance.
- `.trellis/spec/cross-layer/backbone-protocol.md` - Backbone protocol contract expectations.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - AgentRun mailbox/session interaction guidance.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - workflow lifecycle and terminal activity semantics.
- `.trellis/spec/backend/session/runtime-execution-state.md` - runtime execution-state projection expectations.
- `.trellis/spec/backend/session/architecture.md` - runtime session architecture.
- `.trellis/tasks/07-07-agent-lifecycle-fact-source-convergence/design.md` - adjacent lifecycle fact-source convergence design context.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` did not report an active task in this environment, so this file uses the task path explicitly provided by the user.
- `.trellis/spec/backend/session/index.md` and `.trellis/spec/backend/workflow/index.md` were not present; the relevant concrete session/workflow specs listed above were read instead.
- This research does not implement the contract or modify production code. It identifies propagation points and tests for a later implement agent.
- Mailbox/system projection was traced from the task-identified anchors and adjacent gate/companion paths, but not exhaustively line-by-line through every renderer. The recommended invariant remains that mailbox/system output should consume the structured gate diagnostic, not parse prose.
