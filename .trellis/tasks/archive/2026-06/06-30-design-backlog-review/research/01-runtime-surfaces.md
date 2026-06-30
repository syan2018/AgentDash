# Research: Runtime surfaces D1 D5 D6 D12

- Query: Runtime surfaces 设计 research, 覆盖 D1 AgentRun visible capability/admission boundary、D5 command availability policy、D6 AgentRuntimeDelegate split、D12 relay prompt typed payload
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-design-backlog-review/implement.jsonl` - 当前设计任务注入的 spec/research 清单。
- `.trellis/tasks/06-30-design-backlog-review/prd.md` - D1-D12 总目标与每项必须输出的设计字段。
- `.trellis/tasks/06-30-design-backlog-review/design.md` - review 模板、决策状态定义和 runtime surfaces 分组。
- `.trellis/tasks/06-30-design-backlog-review/implement.md` - research dispatch 与后续 synthesis/validation 计划。
- `.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md` - D1/D5/D6/D12 的 canonical backlog 来源。
- `.trellis/tasks/06-30-module-adversarial-review/research/06-agent-runtime-session-surface.md` - D5/D6 的前置证据，尤其 command availability 和 delegate 过宽。
- `.trellis/tasks/06-30-module-adversarial-review/research/08-authority-capability-runtime.md` - D1 的前置证据，记录 quick convergence 前 capability/admission 污染。
- `.trellis/tasks/06-30-module-adversarial-review/research/10-local-runtime-relay-surface.md` - D12 的前置证据，记录 relay prompt ACP JSON 往返。
- `.trellis/tasks/06-30-architecture-quick-convergence/prd.md` - quick convergence scope 与 D1 residual。
- `.trellis/tasks/06-30-architecture-quick-convergence/implement.md` - quick convergence 完成项与 residual: tool invocation execution entry 未完整消费 `admit_tool`。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession/AgentRun/frame surface、canonical `UserInputBlock`、AgentRun mailbox/turn language。
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability/admission 是工具暴露与执行准入唯一 runtime 边界。
- `.trellis/spec/backend/permission/architecture.md` - tool-level grant 只进入 AgentRun admission projection, surface-changing grant 写 AgentFrame revision。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - relay/local runtime typed protocol 与 domain handler 边界。
- `crates/agentdash-application-ports/src/agent_run_surface.rs` - `AgentRunEffectiveCapabilityPort` 与 `admit_tool` port contract。
- `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs` - current effective view、grant projection、runtime session adapter。
- `crates/agentdash-application-ports/src/runtime_session_live.rs` - current RuntimeSession live ports, including capability state projection and mailbox delegate wrapper port。
- `crates/agentdash-application-runtime-session/src/session/hub/tool_builder.rs` - runtime tool assembly entry still asks for runtime-session capability state。
- `crates/agentdash-agent/src/agent_loop/tool_call.rs` - real tool invocation entry calls delegate `before_tool_call` before executing tools。
- `crates/agentdash-agent-types/src/runtime/delegate.rs` - current wide `AgentRuntimeDelegate` trait。
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs` - mailbox wrapper forwards unrelated delegate hooks and only owns turn boundary behavior。
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs` - launch planner composes hook delegate and mailbox wrapper by nesting delegates。
- `crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs` - current `ConversationCommandAvailabilityResolver` and generated command/stale guard model。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs` - command policy already reuses `ConversationCommandAvailabilityResolver` for server-side checks。
- `crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs` - parallel workspace/shell runtime command state projection remains。
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts` - frontend consumes backend command precondition but still adds local workspace-ready gates。
- `packages/app-web/src/features/session/ui/SessionChatViewParts.tsx` - composer uses command set but combines it with local input/cancelling gates。
- `crates/agentdash-application-ports/src/backend_transport.rs` - relay prompt app port currently carries `prompt_blocks: Option<serde_json::Value>` while steer carries `Vec<UserInputBlock>`。
- `crates/agentdash-relay/src/protocol/prompt.rs` - relay wire `CommandPromptPayload.prompt_blocks` is raw JSON。
- `crates/agentdash-application/src/relay_connector.rs` - cloud side converts canonical user input back to ACP `ContentBlock` JSON for relay prompt。
- `crates/agentdash-local/src/handlers/prompt.rs` - local side parses relay ACP `ContentBlock` JSON back into canonical `Vec<UserInputBlock>`。
- `crates/agentdash-agent-protocol/src/backbone/user_input.rs` - canonical `UserInputBlock` alias and one model-boundary conversion to `ContentPart`。

### Code Patterns

- Spec states tool schema exposure must consume AgentRun final visible capability view and tool execution must consume AgentRun admission decision (`.trellis/spec/backend/capability/architecture.md`), and Permission spec says tool-level Grant only enters AgentRun admission projection (`.trellis/spec/backend/permission/architecture.md`).
- Current port shape exists: `AgentRunEffectiveCapabilityRequest`, `AgentRunEffectiveCapabilityView`, `AgentRunAdmissionDecision`, and `AgentRunEffectiveCapabilityPort::admit_tool` are defined in `crates/agentdash-application-ports/src/agent_run_surface.rs:192`, `:204`, `:236`, `:309`, and `:315`.
- Current quick convergence result removed the most dangerous D1 pollution: `AgentRunEffectiveCapabilityService::execution_capability_state_for_runtime_session` loads frame-scoped projection through `grant_projection_for_runtime_session`, but returns `base_state.clone()` rather than mutating visible `CapabilityState` (`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:266`, `:272`, `:281`, `:293`, `:278`).
- Residual D1 is real: `rg` found no `impl AgentRunEffectiveCapabilityPort` and no product-path `admit_tool` call outside service tests; only the trait and service helper exist (`crates/agentdash-application-ports/src/agent_run_surface.rs:315`, `crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:244`).
- The actual tool invocation entry is in the agent loop: `execute_tool_calls` calls `prepare_tool_call`, then `execute_prepared_tool_call`, and `prepare_tool_call` invokes `delegate.before_tool_call` before execution (`crates/agentdash-agent/src/agent_loop/tool_call.rs:22`, `:75`, `:111`, `:343`, `:382`, `:503`, `:546`, `:554` from targeted search output). This is the production entry that must receive AgentRun admission.
- D5 is partly converged: `ConversationCommandAvailabilityResolver::resolve` owns command availability and stale guard construction (`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:416`, `:447`, `:457`, `:460`, `:471`, `:623`, `:739`), and `AgentConversationSnapshotResolver` uses that resolver for UI commands (`crates/agentdash-application-agentrun/src/agent_run/conversation_snapshot.rs:511`, `:520`).
- Command policy already consumes the same resolver, then checks submitted command id/kind/stale guard/current enabled command (`crates/agentdash-application-agentrun/src/agent_run/workspace/command_policy.rs:60`, `:156`, `:192`, `:356`, `:400`, `:409`, `:416`, `:423`, `:430`, `:593`).
- D5 residual is an old parallel status/control projection: `AgentRunWorkspaceProjection` derives `state_code`, `delivery_status`, and `runtime_command_state` from `SessionExecutionState` separately from conversation command availability (`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:12`, `:16`, `:26`, `:37`, `:78`, `:93`). Workspace query still derives that projection next to `conversation` (`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:163`, `:218`, `:240`, `:290`, `:298`, `:535`).
- Frontend is mostly command-driven but still keeps local gates: `commandPrecondition` forwards backend stale guard (`packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceCommands.ts:97`), but handlers also reject when `workspaceStatus !== "ready"` (`:250`, `:299`, `:326`, `:380`). Composer uses backend command enablement and local content/cancelling gates (`packages/app-web/src/features/session/ui/SessionChatViewParts.tsx:545`, `:553`, `:559`, `:572`, `:579`).
- D6 current trait is too wide: `AgentRuntimeDelegate` includes compaction, context transform, before/after tool call, after turn, before stop, and provider request observer methods in one trait (`crates/agentdash-agent-types/src/runtime/delegate.rs:25`, `:26`, `:32`, `:38`, `:50`, `:52`, `:58`, `:64`, `:70`; provider observer appears later in the same trait from full file search).
- D6 wrapper evidence: `AgentRunMailboxRuntimeDelegate` forwards compaction/tool/context methods to `inner`, while only `after_turn`/`before_stop` add mailbox routing/drain semantics (`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:357`, `:364`, `:369`, `:424`, `:439`, `:469`, `:483`, `:508`). Launch planner nests hook runtime delegate inside mailbox wrapper (`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:147`, `:161`, `:163`, `:255`).
- D12 canonical input already exists on non-relay prompt paths: RuntimeSession `UserPromptInput` carries `Option<Vec<UserInputBlock>>` and resolves to `PromptPayload::Input` without ACP deserialization (`crates/agentdash-application-runtime-session/src/session/types.rs:25`, `:27`, `:173`, `:182`); AgentRun boundary mirrors the same shape (`crates/agentdash-application-agentrun/src/agent_run/runtime_session_boundary.rs:26`, `:27`, `:40`, `:49`).
- D12 relay is the outlier: app port `RelayPromptRequest.prompt_blocks` and wire `CommandPromptPayload.prompt_blocks` are raw `serde_json::Value` (`crates/agentdash-application-ports/src/backend_transport.rs:119`, `:122`; `crates/agentdash-relay/src/protocol/prompt.rs:18`, `:23`). Cloud converts `UserInputBlock` to ACP JSON with fallback for non-data URL image/LocalImage/Skill/Mention (`crates/agentdash-application/src/relay_connector.rs:112`, `:118`, `:433`, `:441`, `:447`, `:450`, `:460`, `:463`, `:466`), and local parses ACP JSON back to canonical input (`crates/agentdash-local/src/handlers/prompt.rs:199`, `:203`, `:378`, `:388`, `:394`).

### D1. AgentRun visible capability 与 admission decision 的完整生产边界

**Decision state: `self-decided`.**

Quick convergence already fixed two critical parts: tool-level grant no longer writes into schema-facing visible `CapabilityState`, and runtime projection now queries active grants by `launch_frame_id` through `list_active_by_frame` rather than by run. The remaining production gap is exactly the design-backlog residual: real tool invocation still has no complete consumption of `AgentRunEffectiveCapabilityPort::admit_tool`.

Current wrong path to delete/converge:

- Delete the idea that `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session` is an authorization boundary. It currently returns a `CapabilityState` (`crates/agentdash-application-ports/src/runtime_session_live.rs:48`) and quick convergence made it return the base state unchanged (`crates/agentdash-application-agentrun/src/agent_run/effective_capability.rs:266`, `:278`). Keeping this as an execution-admission concept invites future code to re-mutate capability state.
- Do not let provider-specific guards such as VFS `capability_state.is_capability_tool_enabled` become the tool-level grant admission source. Those guards can remain declarative baseline checks for tool exposure/tool-local invariants, but the Grant system must be enforced once at AgentRun admission.
- Do not add another per-provider “permission check” abstraction. The first-principles owner is AgentRun because it is the only boundary with run/agent/frame/runtime anchor plus grant projection.

Recommended owner/contract:

- Owner: `agentdash-application-agentrun` owns `AgentRunEffectiveCapabilityService` and implements the port in `agentdash-application-ports/src/agent_run_surface.rs`.
- Schema exposure contract: runtime assembly asks for `AgentRunEffectiveCapabilityView` for the current runtime session/anchor. It receives visible `CapabilityState`, VFS, MCP, workspace module refs, and `grant_projection` only as read metadata, never a mutated state.
- Execution admission contract: tool invocation entry calls `admit_tool(AgentRunAdmissionRequest)` before the actual `Tool::execute`. The request must include runtime session id or a resolved AgentRun target plus `tool_name`, `capability_key`, optional cluster, turn/tool-call provenance. Denial returns a tool result or runtime decision with a typed reason, not a provider panic.
- Bridge point: introduce a narrow tool-admission delegate/facet at the agent loop boundary. Since `prepare_tool_call` already calls `delegate.before_tool_call` before execution, the first implementation can wire AgentRun admission through the runtime delegate composition. D6 should later make this an explicit `RuntimeToolPolicyDelegate` facet instead of another method on the wide trait.

Implementation slices:

1. Add a product implementation of `AgentRunEffectiveCapabilityPort` around `AgentRunEffectiveCapabilityService`, repositories, and execution anchor lookup. Include both `effective_capability` and `admit_tool`.
2. Replace `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session` with a schema-exposure API that returns an `AgentRunEffectiveCapabilityView`, or remove the port if runtime assembly can consume `AgentRunEffectiveCapabilityPort` directly through the AgentRun target.
3. Add a runtime tool-admission adapter at launch/prepared-turn construction. It resolves current runtime session target and calls `admit_tool` from `before_tool_call`.
4. Update the real execution path so a denied decision prevents `execute_prepared_tool_call_inner` from calling `tool.execute`.
5. Keep provider-level `CapabilityState` checks only as declarative visible-tool and local invariant checks; document/test that they are not Grant admission.

Validation strategy:

- Unit: tests for `AgentRunEffectiveCapabilityPort` implementation: visible state unchanged for tool-level grant, frame-scoped grant projection, `admit_tool` allow/deny with and without active grant.
- Runtime unit: agent loop test where a visible tool exists but `admit_tool` denies a specific capability/tool path, asserting `tool.execute` is not called.
- Runtime integration-focused: launch a session with a tool-level grant active only on frame A and verify frame B runtime invocation is denied.
- Static/rg check: product code contains at least one non-test call to `AgentRunEffectiveCapabilityPort::admit_tool`; no code reintroduces grant projection into `CapabilityState`.

### D5. Command availability resolver / command policy 统一

**Decision state: `self-decided`.**

Current code has already moved the hardest part into the right shape: `AgentConversationSnapshot`/`ConversationCommandAvailabilityResolver` owns UI-visible commands and stale guards, and command policy reuses the same resolver before accepting cancel/mailbox/resume/composer commands. D5 should therefore avoid adding a new `CommandAvailabilityService` in parallel. The design work is to finish deleting old parallel command/status concepts.

Current wrong path to delete/converge:

- Delete or narrow `AgentRunWorkspaceProjection::runtime_command_state`. It is another derivation from `SessionExecutionState` with status/message language parallel to conversation availability (`crates/agentdash-application-agentrun/src/agent_run/workspace/projection.rs:26`, `:93`).
- Keep `AgentRunWorkspaceProjection.delivery_status` only for shell/list display, not control policy. It may remain a non-control field consumed by `shell_model` (`crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:518`, `:535`).
- Remove frontend local `workspaceStatus !== "ready"` as command authority. It can guard “projection is loading” UX, but the backend command object and server stale guard must be the only command-availability contract.

Recommended owner/contract:

- Owner: `ConversationCommandAvailabilityResolver` in AgentRun conversation snapshot owns command list, keyboard map, stale guard, disabled code/reason, and replacement command hints.
- Command policy contract: all mutating routes submit `AgentRunCommandPreconditionModel`; policy re-resolves the same `ConversationCommandAvailabilityInput`, checks id/kind/stale guard equality, then checks the command is enabled. It does not derive independent allow/deny logic.
- Shell contract: `AgentRunWorkspaceShellModel` may carry `workspace_status`, `delivery_status`, title, and last turn for chrome/list display only. It must not expose a control command state.

Implementation slices:

1. Audit generated contracts and API mappers for `runtime_command_state` / old command-state fields. If unused, remove them from workspace projection types; if still used by a status badge, rename/narrow to non-control shell status.
2. Move any remaining `state_code` or `delivery_status` command decisions into `ConversationCommandAvailabilityResolver`. Keep the resolver’s input as a pure fact object so policy and snapshot stay identical.
3. Update frontend command handlers to rely on `command.enabled` and backend stale response. Retain local content checks and client-side loading prevention only as UX, not as semantic command availability.
4. Add contract-level tests that every command route requires a precondition and returns stale refresh detail on mismatch.

Validation strategy:

- Rust unit: resolver snapshots for idle/running/cancelling/terminal states, asserting command ids, keyboard map, disabled codes, and stale guards.
- Rust policy tests: stale snapshot/frame/runtime/turn mismatch, disabled command, and terminal command rejection use the resolver output.
- Frontend tests: composer/cancel/mailbox handlers send `commandPrecondition(command)` and refresh on `stale_command`; no test should assert workspace shell status as command authority.
- Static/rg check: `runtime_command_state` does not appear in user command paths; `ConversationCommandAvailabilityResolver::resolve` is the only server-side availability derivation.

### D6. AgentRuntimeDelegate 拆 delegate set

**Decision state: `self-decided`.**

The wide trait is an owner smell, not a product decision. Mailbox turn-boundary behavior, hook runtime context/tool/compaction behavior, and provider observers are different reasons to change. The current nested wrapper works functionally but forces every new delegate concern through the mailbox adapter.

Current wrong path to delete/converge:

- Delete the “wrapper forwards every method” pattern. `AgentRunMailboxRuntimeDelegate` only owns turn-boundary delivery/drain, but it forwards compaction/context/tool/provider methods because the trait is monolithic (`crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:357`, `:364`, `:369`, `:424`, `:469`).
- Delete implicit ordering by nesting `mailbox_port.runtime_delegate(input.session_id, hook_runtime_delegate)` (`crates/agentdash-application-runtime-session/src/session/launch/planner.rs:161`, `:163`). Ordering should be visible in launch/prepared-turn data, not encoded by wrapper shape.

Recommended owner/contract:

- Owner: `agentdash-agent-types` defines delegate facets and a `AgentRuntimeDelegateSet` value. RuntimeSession launch owns composition order because launch knows hook runtime, mailbox port, provider observer, and prepared turn.
- Facets:
  - `RuntimeCompactionDelegate`: evaluate/after/failed compaction.
  - `RuntimeContextTransformDelegate`: context transform.
  - `RuntimeToolPolicyDelegate`: before/after tool call. This is where D1 admission adapter belongs.
  - `RuntimeTurnBoundaryDelegate`: after turn and before stop. Mailbox implements this.
  - `RuntimeProviderObserverDelegate`: before provider request / observer-only hooks.
- Compatibility is not needed in this pre-release project; convert the call sites directly rather than keeping the wide trait as a fallback.

Implementation slices:

1. Introduce facet traits and `AgentRuntimeDelegateSet` with optional `Arc<dyn ...>` fields or vectors where multiple observers are valid.
2. Update agent loop config to consume the delegate set. Each call site invokes only the relevant facet. If multiple tool policy delegates are present, define deterministic order and short-circuit semantics.
3. Convert hook runtime delegate into the facets it actually implements.
4. Convert mailbox adapter into `RuntimeTurnBoundaryDelegate` only. Remove all unrelated forwarding code.
5. Update launch plan/prepared turn to carry `runtime_delegate_set` and explicitly compose hook/mailbox/admission facets.

Validation strategy:

- Unit: mailbox delegate implements only turn boundary and drains AgentRun boundary on `before_stop`; no compaction/tool/context forwarding exists.
- Agent loop tests: compaction-only, tool-policy-only, turn-boundary-only, and provider-observer-only delegates are invoked only at their relevant lifecycle points.
- Ordering tests: hook tool policy and D1 admission policy order is deterministic; denial short-circuits tool execution.
- Static/rg check: `AgentRuntimeDelegate` monolithic trait removed or reduced to a transitional private facade only during the same implementation slice, and no `inner.evaluate_compaction` forwarding remains in mailbox adapter.

### D12. Relay prompt typed payload

**Decision state: `self-decided`.**

The canonical contract is already decided in specs and code: user input is `UserInputBlock`, API input, AgentRun mailbox, RuntimeSession launch, and connector delivery use it. Relay prompt is the remaining raw JSON/ACP island. Because the project is pre-release, this should be a direct contract replacement, not a compatibility layer.

Current wrong path to delete/converge:

- Delete `RelayPromptRequest.prompt_blocks: Option<serde_json::Value>` and `CommandPromptPayload.prompt_blocks: Option<serde_json::Value>` (`crates/agentdash-application-ports/src/backend_transport.rs:119`, `:122`; `crates/agentdash-relay/src/protocol/prompt.rs:18`, `:23`).
- Delete paired relay conversions: cloud canonical `UserInputBlock` -> ACP ContentBlock JSON (`crates/agentdash-application/src/relay_connector.rs:433`, `:441`) and local ACP JSON -> canonical (`crates/agentdash-local/src/handlers/prompt.rs:378`, `:388`, `:394`).
- Keep ACP `ContentBlock` conversion only at true ACP edges, not inside AgentDash relay.

Recommended owner/contract:

- Owner: `agentdash-relay` owns the wire DTO, and `agentdash-agent-protocol` owns the canonical user input type.
- Wire shape: `CommandPromptPayload { input: Vec<UserInputBlock>, ... }`; field name should match existing generated API contracts (`input`) rather than `prompt_blocks`.
- App port shape: `RelayPromptRequest { input: Vec<UserInputBlock>, ... }`, aligned with `RelaySteerRequest.input`.
- Cloud relay connector passes `PromptPayload::Input(input)` through as-is. `PromptPayload::Text` is converted once to `text_user_input_blocks`.
- Local prompt handler constructs `UserPromptInput { input: Some(payload.input), ... }` directly.

Implementation slices:

1. Change relay protocol prompt DTO and app transport DTO from `prompt_blocks` to `input: Vec<UserInputBlock>`.
2. Update cloud transport adapter and local prompt handler to pass typed input directly.
3. Remove `user_input_blocks_to_relay_content_blocks`, `relay_prompt_blocks_to_user_input`, and their ContentBlock round-trip tests. Replace tests with relay serialization tests covering text, image data URL, local image/skill/mention variants as typed `UserInput`.
4. Regenerate generated TypeScript contracts if relay protocol is exported to frontend/tooling.
5. Audit remaining `ContentBlock` references. Keep frontend session display/file-reference ACP blocks only if they are display or true ACP adapter concerns; do not feed them into relay prompt.

Validation strategy:

- Rust protocol serde test: `command.prompt` round-trips `input: [UserInput::Text, UserInput::Image, UserInput::Skill, UserInput::Mention]`.
- Cloud/local unit: relay connector sends `RelayPromptRequest.input`; local handler receives same typed input and `resolve_prompt_payload` emits `PromptPayload::Input`.
- Static/rg check: no `prompt_blocks` in relay/app transport/local prompt code; no relay path calls `content_blocks_to_codex_user_input`.
- Existing canonical conversion tests in `agentdash-agent-protocol` remain the only model-boundary `UserInputBlock -> ContentPart` checks.

### Cross-item convergence order

1. D12 first. It is local to relay prompt DTOs and removes raw JSON from runtime prompt transport.
2. D5 second. The resolver/policy shape is already mostly correct; delete old command-state projection before adding more command behavior.
3. D1 third. It needs a stable tool-policy execution hook and should be wired with the smallest current delegate adapter first.
4. D6 fourth, or paired with D1 if implementation capacity allows. The final delegate-set split should absorb D1 admission into `RuntimeToolPolicyDelegate` and remove the temporary wide-trait placement.

## Related Specs

- `.trellis/spec/backend/session/architecture.md` - RuntimeSession is delivery/trace substrate; AgentRun frame surface is capability/context/VFS/MCP fact source; canonical `UserInputBlock` is the single session input representation.
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability/admission owns final visible capability and execution admission.
- `.trellis/spec/backend/permission/architecture.md` - tool-level grant is admission-only; surface-changing grant writes AgentFrame revision.
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - local relay command router stays thin and relay protocol payloads should be typed by domain.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - control decisions must be made at the owning boundary, not inferred across layers.
- `.trellis/spec/guides/code-reuse-thinking-guide.md` - remove duplicate derivations before extracting new abstractions.

## External References

- None. This research used internal Trellis specs, prior task research, quick convergence notes, and targeted source inspection only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, so this research used the user-provided task path `.trellis/tasks/06-30-design-backlog-review` and wrote only under that task's `research/` directory.
- No business code, specs, task plans, or git state were modified.
- No Rust build/test was run; this was intentionally static design research with targeted `rg`/read evidence.
- Line references are from the current working tree at research time and can shift if concurrent agents edit code.
