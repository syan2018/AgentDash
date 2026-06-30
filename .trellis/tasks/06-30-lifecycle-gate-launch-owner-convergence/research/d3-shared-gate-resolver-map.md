# Research: D3 Shared LifecycleGateResolver implementation map

- Query: Map current Companion and Workflow HumanGate LifecycleGate call sites, owner responsibilities, and the D3 transition plan for a shared LifecycleGateResolver.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/prd.md` - D3 acceptance requires Companion gate and Workflow HumanGate to use a shared resolver and remove mailbox delivery status blobs from gate payload.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/design.md` - Defines `LifecycleGateResolver -> GateTransitionOutcome -> delivery / notification adapters`.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/implement.md` - Stage 2 checklist and focused static/cargo checks for D3.
- `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/implement.jsonl` - Curated spec/research manifest used before code inspection.
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession is delivery/trace substrate; business gate facts should remain Lifecycle-owned.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - Source adapter and RuntimeSessionExecutionAnchor lookup boundaries, relevant to companion runtime context resolution.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - Mailbox is durable AgentRun delivery fact; delivery receipts should remain mailbox/receipt projection, not LifecycleGate state.
- `.trellis/spec/backend/workflow/architecture.md` - Workflow HumanGate and orchestration reducer contracts; runtime node state advances through reducer events.
- `.trellis/spec/backend/workflow/lifecycle-run-link.md` - RuntimeSession -> anchor -> Lifecycle lookup contract used by companion context resolution.
- `.trellis/tasks/archive/2026-06/06-30-design-backlog-review/research/04-orchestration-gate-launch.md` - Prior D3 evidence and owner recommendation.
- `.trellis/tasks/archive/2026-06/06-30-design-backlog-review/design-review.md` - D3 decision matrix and validation shape.
- `.trellis/tasks/archive/2026-06/06-30-design-backlog-review/implementation-slices.md` - Slice 11 sequence for shared gate resolver.
- `crates/agentdash-domain/src/workflow/lifecycle_gate.rs` - Durable `LifecycleGate` aggregate and current `resolve` method.
- `crates/agentdash-domain/src/workflow/repository.rs` - `LifecycleGateRepository` trait.
- `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs` - PostgreSQL LifecycleGate persistence implementation.
- `crates/agentdash-application/src/companion/gate_control.rs` - Current thick Companion gate service, target for facade thinning.
- `crates/agentdash-application/src/companion/tools.rs` - Companion tool call sites and current AgentRun mailbox delivery adapter.
- `crates/agentdash-api/src/routes/companion_gates.rs` - Human response HTTP route.
- `crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs` - Older generic gate create/wait/resolve service still directly mutates gate payload.
- `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs` - Workflow HumanGate open/resolve path currently bypasses any shared resolver.
- `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs` - Workflow executor call sites for opening HumanGate and submitting decisions.
- `crates/agentdash-api/src/routes/workflows.rs` - API route for submitting Workflow HumanGate decisions.

### Current Call Sites And Owner Responsibilities

#### Durable domain/repository

- `LifecycleGate` is a mutable durable fact with `run_id`, optional `agent_id/frame_id`, `gate_kind`, `correlation_id`, `status`, `payload_json`, `resolved_by`, and timestamps (`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:10`).
- The aggregate exposes only `open`, `resolve`, and `is_open`; `resolve` mutates status and resolution metadata but does not validate gate kind, owner, payload shape, or transition-specific invariants (`crates/agentdash-domain/src/workflow/lifecycle_gate.rs:30`, `crates/agentdash-domain/src/workflow/lifecycle_gate.rs:53`).
- `LifecycleGateRepository` only supports create/get/list-open-for-agent/update (`crates/agentdash-domain/src/workflow/repository.rs:117`). This is enough for D3 if resolver remains application-level and owns load/create/update sequencing.
- PostgreSQL persistence stores `payload_json` as serialized JSON text and `update` only writes status/payload/resolution fields (`crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:743`, `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:799`). D3 should not need a migration unless the implementation adds new persisted columns.

#### CompanionGateControlService

- `CompanionGateControlService` currently owns too many concerns: gate repo, run/frame/agent/anchor/lineage lookup, session notification delivery, parent mailbox delivery, and human response mailbox delivery (`crates/agentdash-application/src/companion/gate_control.rs:346`).
- It defines delivery traits and result DTOs beside transition logic (`crates/agentdash-application/src/companion/gate_control.rs:188`, `crates/agentdash-application/src/companion/gate_control.rs:201`, `crates/agentdash-application/src/companion/gate_control.rs:219`).
- `respond` validates payload, loads an open gate, resolves delivery runtime, delivers a mailbox message, writes `human_mailbox_delivery` into `gate.payload_json`, calls `gate.resolve`, and persists (`crates/agentdash-application/src/companion/gate_control.rs:417`, `crates/agentdash-application/src/companion/gate_control.rs:453`, `crates/agentdash-application/src/companion/gate_control.rs:485`, `crates/agentdash-application/src/companion/gate_control.rs:521`, `crates/agentdash-application/src/companion/gate_control.rs:526`).
- `complete_child_result_to_parent` resolves child frame/lineage, finds an open child-owned gate by `correlation_id`, delivers to parent mailbox, writes `parent_mailbox_delivery`, resolves the gate, and emits best-effort notifications (`crates/agentdash-application/src/companion/gate_control.rs:537`, `crates/agentdash-application/src/companion/gate_control.rs:549`, `crates/agentdash-application/src/companion/gate_control.rs:569`, `crates/agentdash-application/src/companion/gate_control.rs:652`, `crates/agentdash-application/src/companion/gate_control.rs:687`, `crates/agentdash-application/src/companion/gate_control.rs:698`).
- `open_parent_request` resolves child/parent context, creates a parent-owned `LifecycleGate`, stores a request payload containing `parent_mailbox_delivery: { status: "pending" }`, delivers to parent mailbox, then rewrites gate payload with delivery result (`crates/agentdash-application/src/companion/gate_control.rs:727`, `crates/agentdash-application/src/companion/gate_control.rs:742`, `crates/agentdash-application/src/companion/gate_control.rs:779`, `crates/agentdash-application/src/companion/gate_control.rs:791`, `crates/agentdash-application/src/companion/gate_control.rs:801`, `crates/agentdash-application/src/companion/gate_control.rs:834`, `crates/agentdash-application/src/companion/gate_control.rs:874`).
- `resolve_parent_request` validates parent ownership/current delivery runtime, reads child refs from gate payload, delivers response to child mailbox, writes `child_mailbox_delivery`, resolves the parent-owned gate, and emits a notification (`crates/agentdash-application/src/companion/gate_control.rs:905`, `crates/agentdash-application/src/companion/gate_control.rs:928`, `crates/agentdash-application/src/companion/gate_control.rs:941`, `crates/agentdash-application/src/companion/gate_control.rs:966`, `crates/agentdash-application/src/companion/gate_control.rs:1052`, `crates/agentdash-application/src/companion/gate_control.rs:1091`, `crates/agentdash-application/src/companion/gate_control.rs:1092`).
- Current helper functions explicitly embed mailbox delivery status blobs into gate payload (`crates/agentdash-application/src/companion/gate_control.rs:1366`, `crates/agentdash-application/src/companion/gate_control.rs:1408`, `crates/agentdash-application/src/companion/gate_control.rs:1443`, `crates/agentdash-application/src/companion/gate_control.rs:1453`, `crates/agentdash-application/src/companion/gate_control.rs:1463`). These are D3 deletion targets or must become response/result diagnostics outside durable gate facts.
- `resolve_delivery_runtime_session_id`, `validate_current_delivery_runtime_session_id`, and `select_current_delivery` are companion runtime context lookup logic inside the service (`crates/agentdash-application/src/companion/gate_control.rs:1120`, `crates/agentdash-application/src/companion/gate_control.rs:1166`, `crates/agentdash-application/src/companion/gate_control.rs:1184`). This belongs in a context resolver.

#### Companion routes/tools/adapters

- `POST /companion-gates/{gate_id}/respond` builds the full `CompanionGateControlService`, all owner repos, session eventing, and `AgentRunCompanionMailboxDelivery` just to respond to a human gate (`crates/agentdash-api/src/routes/companion_gates.rs:51`, `crates/agentdash-api/src/routes/companion_gates.rs:60`). D3 should narrow this route to a human-response use case/facade method.
- `CompanionGateControlFactory` in tools wires `CompanionGateControlService` with session eventing plus parent and human mailbox delivery (`crates/agentdash-application/src/companion/tools.rs:133`, `crates/agentdash-application/src/companion/tools.rs:142`, `crates/agentdash-application/src/companion/tools.rs:155`).
- Tool call sites currently invoke `open_parent_request`, `resolve_parent_request`, and `complete_child_result_to_parent` through that factory (`crates/agentdash-application/src/companion/tools.rs:1081`, `crates/agentdash-application/src/companion/tools.rs:1669`, `crates/agentdash-application/src/companion/tools.rs:1821`).
- `AgentRunCompanionMailboxDelivery` is already the concrete mailbox delivery adapter in shape: it maps companion intents to `MailboxSourceIdentity` and calls `deliver_companion_mailbox_message` (`crates/agentdash-application/src/companion/tools.rs:166`, `crates/agentdash-application/src/companion/tools.rs:224`, `crates/agentdash-application/src/companion/tools.rs:261`, `crates/agentdash-application/src/companion/tools.rs:297`, `crates/agentdash-application/src/companion/tools.rs:336`). D3 can move/rename this rather than inventing mailbox mechanics.

#### Workflow HumanGate

- `HumanGateLauncher::open` directly constructs `LifecycleGate::open` with `gate_kind = "orchestration_human_gate"` and payload contract `orchestration_human_gate.v1`, persists it, and returns a `NodeStarted` event with `ExecutorRunRef::HumanDecision { decision_id: gate_id }` (`crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:28`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:58`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:68`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:80`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:90`).
- `HumanGateLauncher::resolve_decision` loads the gate, checks open, replaces `gate.payload_json` with the decision payload, calls `gate.resolve`, and persists (`crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:101`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:120`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:125`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:130`, `crates/agentdash-application-workflow/src/orchestration/human_gate_launcher.rs:131`).
- `OrchestrationExecutorLauncher::submit_human_gate_decision` calls the launcher, then applies `NodeCompleted` and drains ready nodes (`crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:185`, `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:196`, `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:201`, `crates/agentdash-application-workflow/src/orchestration/executor_launcher.rs:213`).
- API route `submit_orchestration_human_decision` constructs the workflow launcher and passes `SubmitHumanGateDecisionInput` (`crates/agentdash-api/src/routes/workflows.rs:526`, `crates/agentdash-api/src/routes/workflows.rs:543`, `crates/agentdash-api/src/routes/workflows.rs:547`).

#### Existing generic LifecycleGateService

- `LifecycleGateService::create_gate` and `resolve_gate` are older generic helpers and directly create/resolve gates with arbitrary payload (`crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs:23`, `crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs:94`).
- `resolve_gate` still sets `gate.payload_json = Some(payload)` and calls `gate.resolve` (`crates/agentdash-application-lifecycle/src/lifecycle/gate_service.rs:112`). If still used in product paths, it should either delegate to the shared resolver or be explicitly kept out of D3 scope as legacy polling helper. Search found direct references mostly from companion tool text, not a new route.

### Proposed File / Module Layout

Dependency direction matters:

- `agentdash-application-workflow` depends only on ports/domain/spi and can be consumed by top-level `agentdash-application`.
- `agentdash-application-lifecycle` already depends on `agentdash-application-workflow`.
- Top-level `agentdash-application` depends on both workflow and lifecycle.

Therefore a shared resolver used by Workflow HumanGate and Companion must not live under top-level `agentdash-application::companion`, and placing it under `agentdash-application-lifecycle` would require `agentdash-application-workflow` to depend back on lifecycle. Recommended layout:

```text
crates/agentdash-application-workflow/src/
  gate/
    mod.rs
    resolver.rs
    commands.rs
    outcome.rs
```

- `gate/commands.rs`: `LifecycleGateCommand`, command payload structs for `RespondHuman`, `OpenParentRequest`, `ResolveParentRequest`, `CompleteChildResult`, `OpenWorkflowHumanGate`, `ResolveWorkflowHumanGate`.
- `gate/outcome.rs`: `GateTransitionOutcome`, `GateTransitionKind`, `GateDeliveryIntent`, `GateNotificationIntent`, stable delivery ref structs. Keep this application-level, not domain, because it includes side-effect intents.
- `gate/resolver.rs`: `LifecycleGateResolver` with repository-backed methods. It owns load/create/update and pure durable transition semantics only.
- `gate/mod.rs`: re-export public command/outcome/resolver types from `agentdash_application_workflow::gate::{...}`.

Companion-specific modules should remain in top-level application because they depend on AgentRun mailbox/session services:

```text
crates/agentdash-application/src/companion/
  gate_control.rs            # temporary facade only; delegates
  gate_context.rs            # CompanionGateContextResolver
  gate_delivery.rs           # CompanionGateDeliveryAdapter / AgentRunCompanionMailboxDelivery moved or re-exported
  gate_notification.rs       # CompanionGateNotificationAdapter / SessionEventing adapter
```

- `CompanionGateContextResolver`: owns `resolve_current_frame_from_delivery_trace_ref`, lineage lookup, current delivery selection, and gate owner validation currently embedded in `CompanionGateControlService`.
- `CompanionGateDeliveryAdapter`: consumes `GateDeliveryIntent` plus resolved companion context and creates AgentRun mailbox envelopes. Existing `AgentRunCompanionMailboxDelivery` should move here or become this adapter.
- `CompanionGateNotificationAdapter`: consumes `GateNotificationIntent` and calls `SessionEventingService`; failures remain diagnostics, not gate transition facts.
- `CompanionGateControlService`: stays as public facade for this stage, but its methods become orchestration: context -> resolver -> delivery -> notification -> response DTO.

Workflow HumanGate adapter can stay small:

```text
crates/agentdash-application-workflow/src/orchestration/
  human_gate_launcher.rs     # delegates gate open/resolve to workflow::gate::LifecycleGateResolver
```

It should keep orchestration-specific tasks: deriving node coordinate, producing `NodeStarted` / `NodeCompleted`, and computing `NodePortValue` outputs. It should not directly write `gate.payload_json` or call `gate.resolve`.

### Exact Transition Slices

#### Slice 1: Shared resolver skeleton and Workflow HumanGate open/resolve

1. Add `agentdash-application-workflow/src/gate/{mod.rs,commands.rs,outcome.rs,resolver.rs}` and export from `lib.rs`.
2. Implement:
   - `LifecycleGateCommand::OpenWorkflowHumanGate { run_id, orchestration_id, node_path, attempt, plan_node_id, label, executor }`
   - `LifecycleGateCommand::ResolveWorkflowHumanGate { gate_id, decision, resolved_by }`
   - `GateTransitionKind::{Opened, Resolved}`
   - `GateTransitionOutcome { gate, transition, delivery_intents: vec![], notification_intents: vec![] }`
3. Move `LifecycleGate::open(... "orchestration_human_gate" ...)` payload construction from `HumanGateLauncher::open` into resolver.
4. Move `gate.payload_json = Some(input.decision.clone()); gate.resolve(...)` from `HumanGateLauncher::resolve_decision` into resolver.
5. Keep `HumanGateLauncher` responsible for executor validation, `ExecutorRunRef::HumanDecision`, `human_decision_outputs`, and reducer event wiring.
6. Update existing test `launcher_opens_human_gate_with_orchestration_node_contract` to assert the same opened payload through resolver. Add a new resolve test proving the launcher no longer mutates gate directly.

#### Slice 2: Respond human gate

1. Add `LifecycleGateCommand::RespondHuman { gate_id, payload, resolved_by }`.
2. Resolver responsibilities:
   - load gate by id;
   - reject missing/closed gate;
   - validate response payload object shape and request type using the same payload registry semantics currently in `respond`;
   - set durable decision payload without `human_mailbox_delivery`;
   - resolve with `resolved_by` such as `"companion_respond"` or a human actor id;
   - emit a `GateDeliveryIntent::CompanionHumanResponse` carrying stable refs: gate id, request id, run id, agent id, turn id, request type, payload, and delivery runtime target placeholder.
3. Context resolver responsibilities:
   - derive requesting `agent_id` from gate owner;
   - resolve current delivery runtime session id;
   - reject missing/mismatched frame/agent ownership.
4. Delivery adapter responsibilities:
   - create the mailbox message from `CompanionHumanResponse` intent;
   - return mailbox/receipt refs to the facade response only.
5. Notification adapter responsibilities:
   - if a notification is still product-required, inject it after resolver has persisted the durable transition; do not write notification status into gate payload.
6. Remove or rewrite assertions in tests such as `respond_resolves_gate_and_delivers_by_anchor_runtime_ref` and `respond_records_human_mailbox_delivery_failure` so gate payload contains only decision fact; delivery failure is surfaced as command error/diagnostic without a delivery-status blob.

#### Slice 3: Open parent request

1. Add `LifecycleGateCommand::OpenParentRequest { run_id, parent_agent_id, parent_frame_id, child_agent_id, child_frame_id, child_delivery_runtime_session_id, parent_delivery_runtime_session_id, turn_id, wait, payload }`.
2. Context resolver does all current runtime/lineage work before the command:
   - child frame from delivery trace;
   - child current delivery session validation;
   - parent lineage lookup;
   - parent current frame/current delivery runtime selection.
3. Resolver responsibilities:
   - create parent-owned `LifecycleGate` with `gate_kind = "companion_parent_request"`;
   - set `correlation_id = gate.id`;
   - persist request fact payload containing request/owner/session refs, `request_type`, `adoption_mode`, status/summary/wait/user payload;
   - do not include `parent_mailbox_delivery: { status: "pending" }`.
   - emit `GateDeliveryIntent::CompanionParentRequest`.
   - emit `GateNotificationIntent::CompanionReviewRequest`.
4. Delivery adapter delivers parent mailbox message and returns mailbox refs in `CompanionParentRequestOpenResult`.
5. Notification adapter injects best-effort event after delivery. Its failure only logs diagnostics.
6. Update `open_parent_request_creates_parent_owned_gate_and_delivery_event`, `open_parent_request_records_mailbox_failure_on_gate_payload`, and `open_parent_request_uses_parent_current_frame_after_delivery_refresh` around the new split. The failure test should no longer expect payload mutation.

#### Slice 4: Resolve parent request

1. Add `LifecycleGateCommand::ResolveParentRequest { gate_id, parent_agent_id, parent_frame_id, parent_delivery_runtime_session_id, child_agent_id, child_frame_id, child_delivery_runtime_session_id, resolved_turn_id, payload, resolved_by }`.
2. Context resolver responsibilities:
   - parse request id as gate id;
   - load gate or return `None`;
   - validate `gate_kind == "companion_parent_request"`;
   - resolve parent frame from runtime session;
   - verify gate owner matches parent frame;
   - verify parent and child current delivery runtime sessions;
   - read stable child refs from existing gate payload.
3. Resolver responsibilities:
   - reject closed gate;
   - construct durable resolution fact with gate id/request id/run/parent/child refs and decision payload;
   - resolve with `parent_agent:<id>` or `parent_agent:<id>` equivalent chosen naming;
   - do not include `child_mailbox_delivery`.
   - emit `GateDeliveryIntent::CompanionParentResponseToChild`.
   - emit `GateNotificationIntent::CompanionParentRequestResolved`.
4. Delivery adapter delivers the response to the child mailbox and returns mailbox refs in the service response.
5. Existing `resolve_parent_request_resolves_only_parent_owned_gate`, `resolve_parent_request_records_child_mailbox_failure_on_gate_payload`, and `resolve_parent_request_rejects_delivery_session_for_another_frame` should be split into resolver validation tests plus adapter/facade delivery tests.

#### Slice 5: Complete child result

1. Add `LifecycleGateCommand::CompleteChildResult { gate_id or correlation_id, request_id, child_agent_id, child_frame_id, child_delivery_runtime_session_id, parent_agent_id, parent_delivery_runtime_session_id, resolved_turn_id, payload, resolved_by }`.
2. Context resolver responsibilities:
   - resolve child frame from delivery trace;
   - find parent lineage;
   - find open child-owned gate by `correlation_id == request_id`;
   - select parent current delivery runtime;
   - validate child current delivery runtime.
3. Resolver responsibilities:
   - reject closed gate idempotently or return an already-resolved/no-delivery outcome so duplicate child result does not re-deliver;
   - normalize result status (`completed`, `blocked`, `needs_follow_up`);
   - persist resolution fact with summary/findings/follow-ups/artifact refs and stable parent/child refs;
   - resolve with `child_agent:<id>`;
   - do not include `parent_mailbox_delivery`.
   - emit `GateDeliveryIntent::CompanionChildResultToParent`.
   - emit parent/child notification intents.
4. Delivery adapter sends the parent mailbox message. Facade preserves duplicate protection: if resolver returns no delivery for a closed/already-resolved gate, mailbox adapter is not invoked.
5. Existing tests `complete_child_result_resolves_child_owned_gate_and_delivers_events`, `complete_child_result_records_mailbox_failure_on_gate_payload`, and `duplicate_child_result_does_not_deliver_second_parent_mailbox_message` are the main regression set.

### Static Checks To Add Or Update

- D3 payload status blob removal:
  ```powershell
  rg -n "gate\\.payload_json.*delivery|with_parent_mailbox_delivery_payload|with_human_mailbox_delivery_payload|with_child_mailbox_delivery_payload|parent_mailbox_delivery\"\\s*:\\s*\\{|human_mailbox_delivery|child_mailbox_delivery" crates/agentdash-application/src/companion crates/agentdash-application-workflow/src
  ```
  Expected after D3: no production helper/status blob writes. Tests may reference old names only if testing absence; prefer deleting old expectations.
- Direct gate mutation bypass:
  ```powershell
  rg -n "gate\\.payload_json\\s*=|gate\\.resolve\\(" crates/agentdash-application/src/companion crates/agentdash-application-workflow/src/orchestration crates/agentdash-application-lifecycle/src/lifecycle
  ```
  Expected after D3: production direct mutations only inside `LifecycleGateResolver` (and possibly legacy `LifecycleGateService` if explicitly out of scope).
- Shared resolver presence and use:
  ```powershell
  rg -n "LifecycleGateResolver|GateTransitionOutcome|GateDeliveryIntent|GateNotificationIntent|LifecycleGateCommand" crates/agentdash-application-workflow/src crates/agentdash-application/src/companion
  ```
  Expected after D3: workflow HumanGate and companion facade both import/use the shared resolver types.
- Route narrowing:
  ```powershell
  rg -n "CompanionGateControlService::with_session_eventing|AgentRunCompanionMailboxDelivery::from_runtime_services" crates/agentdash-api/src/routes/companion_gates.rs
  ```
  Expected after route slice: route calls a narrow human-response use case/factory instead of constructing the full delivery service graph inline.

Focused compile checks:

```powershell
cargo check -p agentdash-application-workflow
cargo check -p agentdash-application -p agentdash-api
```

Run both after the workflow slice and companion slice:

```powershell
cargo check -p agentdash-application -p agentdash-application-workflow -p agentdash-api
```

### Focused Tests To Add Or Update

- New resolver unit tests in `agentdash-application-workflow/src/gate/resolver.rs`:
  - open workflow human gate creates `orchestration_human_gate.v1` payload and opened outcome.
  - resolve workflow human gate rejects closed gate and persists decision fact.
  - respond human validates owner/open/request type and returns a human-response delivery intent without delivery status in payload.
  - open parent request creates parent-owned gate with stable request refs and no mailbox status blob.
  - resolve parent request rejects invalid owner/closed gate/malformed payload and returns child delivery intent.
  - complete child result normalizes status, resolves once, and duplicate completion produces no second delivery intent.
- Companion context resolver tests:
  - child runtime session resolves to current child frame and lineage parent.
  - parent request rejects missing parent lineage/current delivery.
  - resolve parent request rejects delivery session for another frame.
  - human response rejects gate without agent/frame owner.
- Companion delivery adapter tests:
  - human response intent maps to `MailboxSourceIdentity(namespace="companion", kind="human_response", route="human")`.
  - parent request intent maps to `kind="parent_request", route="parent"`.
  - child result intent maps to `kind="result", route="parent"`.
  - parent response intent maps to `kind="parent_response", route="child"`.
  - mailbox/receipt refs are returned to facade response, not written into `LifecycleGate.payload_json`.
- Companion notification adapter tests:
  - parent request/result/parent response notification intents produce the existing event envelopes.
  - notification failure is diagnostic-only and does not update gate state.
- Existing companion tests to update rather than delete:
  - `respond_resolves_gate_and_delivers_by_anchor_runtime_ref`
  - `respond_records_human_mailbox_delivery_failure`
  - `complete_child_result_resolves_child_owned_gate_and_delivers_events`
  - `open_parent_request_creates_parent_owned_gate_and_delivery_event`
  - `resolve_parent_request_resolves_only_parent_owned_gate`
  - `duplicate_child_result_does_not_deliver_second_parent_mailbox_message`
- Existing workflow tests to update:
  - `launcher_opens_human_gate_with_orchestration_node_contract`
  - add submit-human-decision regression asserting `NodeCompleted` still follows resolver-driven gate resolution.
- API/route focused test:
  - companion gate response route should no longer require parent request delivery wiring for the simple human response path.

### Related Specs

- `.trellis/spec/backend/session/architecture.md`
- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/agentrun-mailbox.md`
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`

### External References

- No external references used. This research is based on task artifacts, Trellis specs, and repository code only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell. This research uses the explicit task/output path provided in the assignment: `.trellis/tasks/06-30-lifecycle-gate-launch-owner-convergence/research/`.
- I did not run `git status` or any other git operation because this Trellis research role forbids git operations. Pre-existing workspace changes therefore were not independently enumerated. Relevant risk: current code already appears in a partially evolved branch (D4 may have changed surrounding launch code), so implementers should re-run the static checks immediately before editing and must not overwrite unrelated dirty files.
- No business code was modified and no compile/tests were run.
- The recommended resolver location is `agentdash-application-workflow/src/gate/` because current crate dependencies let both Workflow HumanGate and top-level Companion code consume it without a dependency cycle. If the team wants a more semantically neutral owner, the alternative is a new lower-level application crate, but that is larger than D3 needs.
