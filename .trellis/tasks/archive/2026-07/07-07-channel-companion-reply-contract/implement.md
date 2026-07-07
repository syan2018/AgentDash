# Implementation Plan

## Planning Decision

This task stays Companion-first. It fixes the model-visible companion reply loop and keeps Channel as a lightweight vocabulary constraint only. Do not create a generic `agentdash-domain::channel` module, do not migrate `MailboxSourceIdentity`, and do not add channel persistence in this slice.

The externally visible fix requires tool schema, dispatch prompt, skill docs and runtime resolver to change together, so this remains one Trellis task rather than independent child tasks.

## First Principle Guardrail

The acceptance path is: the Agent receives the smallest complete instruction needed to call `companion_respond` correctly.

Any implementation step that adds IDs or internal owner facts to the Agent context must justify why the Agent itself needs that value. Recovery, audit, gate resolution and mailbox routing are runtime concerns and belong in hidden `CompanionReplyContract` metadata.

## Work Packages

### W1. Lock The Model-Facing Contract Tests

Add or update tests before refactoring:

- `build_companion_dispatch_prompt` should render a minimal `companion_respond` envelope.
- singleton child dispatch prompt should not expose `dispatch_id`, `gate_id`, `run_id`, `agent_id`, `frame_id`, or `session_id`.
- `companion_response_payload_schema` should accept custom payload objects that old `anyOf` would reject.
- `companion-system` examples should not mention `request_id`.

Evidence files:

- `crates/agentdash-application/src/companion/tools.rs`
- `crates/agentdash-domain/src/companion/skills/companion-system/SKILL.md`
- `crates/agentdash-domain/src/companion/skills/companion-system/references/response-adoption.md`

### W2. Add Companion Reply Contract Module

Add `crates/agentdash-application/src/companion/reply_contract.rs`.

Implement:

- `CompanionReplyContract`
- `CompanionReplyTarget`
- `CompanionPayloadExpectation`
- `ModelReplyInstruction`
- `ModelReplySelector`
- renderer/helper to produce the minimal `companion_respond` argument example
- helper to produce repair hints for invalid or ambiguous selectors

Keep this module application-owned. It may serialize into existing gate/action metadata, but it is not a domain-wide channel abstraction.

Tests:

- `ModelReplyInstruction` serialization excludes target owner refs.
- minimal argument example contains `payload` only for singleton child dispatch.
- alias selector example appears only when the instruction explicitly contains an alias.

### W3. Build Reply Contracts In Companion Dispatch Paths

Update companion dispatch/request creation so it builds a `CompanionReplyContract` before prompt rendering.

Update:

- `CompanionDispatchPlan` or adjacent dispatch input to carry the model instruction.
- `CompanionChildDispatchRequest` / gate payload to persist internal reply contract metadata.
- `CompanionLaunchSource` only as needed to carry rendered prompt or compact reply instruction facts.
- `build_companion_dispatch_prompt` to consume `ModelReplyInstruction`, not raw runtime refs.

Tests:

- child dispatch gate payload contains recoverable reply contract metadata.
- prompt uses the same `ModelReplyInstruction` as the persisted contract.
- prompt contains no internal GUIDs on the default path.

### W4. Refactor `companion_respond`

Replace model-facing params:

```rust
pub struct CompanionRespondParams {
    pub reply_to: Option<CompanionReplySelectorParam>,
    pub payload: serde_json::Value,
}
```

Behavior:

- omitted `reply_to` resolves exactly one active reply contract from current runtime/session/frame context;
- `reply_to.kind=current` resolves current active contract, optionally by channel name;
- `reply_to.kind=alias` resolves a prompt-visible alias;
- zero/multiple matches return repairable errors with valid tool-call examples;
- resolved contract executes exactly one owner operation.

Remove the current multi-try behavior where one `request_id` attempt probes parent request gate, pending action and child result path.

### W5. Open Payload Schema, Keep Semantic Validation

Replace `companion_response_payload_schema` with an open object schema.

Keep:

- `payload_object_error`
- `PayloadTypeRegistry::validate_response`
- request/response type matching when the resolved reply contract knows the originating request type

Tests:

- absent `payload.type` is accepted by schema and runtime object validation.
- unregistered `payload.type` is accepted by schema and semantic registry.
- registered response type missing required fields still fails semantic validation.

### W6. Align Tool Description, Tool Schema Delta And Skill Docs

Update every model-visible surface:

- `CompanionRespondTool::description`
- `CompanionRespondParams` field descriptions
- runtime Tool Schema Delta text if current rendering makes optional `reply_to` confusing
- `build_companion_dispatch_prompt`
- `inject_companion_role_fragment`
- `companion-system/SKILL.md`
- `references/response-adoption.md`

Required wording:

- default: finish the assigned work, then call `companion_respond` with `payload`;
- `reply_to` is optional and only used when the prompt lists multiple reply targets;
- no gate/frame/run/session GUIDs in model-facing docs.

### W7. Runtime Owner Integration

Persist reply contract metadata in existing owner facts only where needed:

- child dispatch LifecycleGate payload;
- pending action metadata if pending actions still use `companion_respond`;
- parent request gate payload if parent/human response flows require it.

Keep wait policy correlation refs and mailbox source identity unchanged.

### W8. Spec Update After Behavior Stabilizes

Update durable knowledge only after code behavior is stable:

- `.trellis/spec/backend/session/session-startup-pipeline.md` for companion launch prompt using hidden reply contracts and minimal model instruction.
- `.trellis/spec/backend/domain-payload-typing.md` for open companion response schema plus semantic registry validation.
- `.trellis/spec/backend/session/agentrun-mailbox.md` only if companion result delivery text needs the minimal-model-visible-output clarification.
- companion skill docs in `crates/agentdash-domain/src/companion/skills/companion-system/`.

Do not write broad Channel architecture docs in this task; future Channel work should be based on observed implementation pressure after this loop is cleaned up.

## Validation Commands

Run targeted checks first:

```powershell
cargo test -p agentdash-application companion
cargo test -p agentdash-agent tool_call
```

If touched code crosses gate/mailbox integration:

```powershell
cargo test -p agentdash-application-agentrun mailbox
cargo test -p agentdash-application-workflow companion
```

If docs/contracts/generated types change:

```powershell
cargo test -p agentdash-contracts
pnpm --filter app-web typecheck
```

Run broader checks only if touched files cross package boundaries in a way targeted tests cannot cover.

## Risk Points

- `CompanionLaunchSource` lives in `agentdash-application-ports`; avoid moving heavy companion/channel types into ports unless launch really needs them.
- `AgentTool` currently exposes only description and JSON Schema. Prefer concise schema plus prompt/skill instruction updates in this task; a generic repair-hint trait can be a follow-up if several tools need it.
- Existing tests may assert `request_id`; update them to assert minimal model-facing calls and hidden contract resolution instead.
- Do not serialize full `CompanionReplyContract` into prompt. Prompt rendering must consume `ModelReplyInstruction`.
- Resist adding generic Channel actor/message/delivery-owner types in this slice.

## Ready-To-Start Checklist

- [ ] `prd.md` and `design.md` reviewed.
- [ ] `implement.jsonl` contains real spec/research context for implementation.
- [ ] `check.jsonl` contains real spec/research context for verification.
- [ ] User agrees with the reduced MVP scope: Companion reply instruction loop first; Channel remains vocabulary-only in this task.
