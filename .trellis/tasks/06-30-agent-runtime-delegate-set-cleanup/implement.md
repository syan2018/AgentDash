# Implementation Plan

## Operating Rules

- Follow Trellis workflow and start this task before code edits.
- Every subagent prompt must start with `Active task: .trellis/tasks/06-30-agent-runtime-delegate-set-cleanup`.
- Cleanup-first constraint: this review exists to converge architecture from first principles. Removing broad forwarding wrappers and duplicated lifecycle ownership is more important than adding feature surface.
- Do not add compatibility paths or another broad delegate abstraction to hide the old split.
- Implementation subagents must not run large Rust builds or broad suites. Use scoped `rg`, `cargo fmt --check --package <touched>`, and small targeted tests only. Expensive Rust compilation belongs to check/integration.
- While subagents run, the main session should not interrupt them repeatedly; wait or work on non-overlapping synthesis/docs.

## Work Items

1. Agent types and agent loop facet migration
   - Add facet traits and `AgentRuntimeDelegateSet` in `agentdash-agent-types`.
   - Update `agentdash-agent` config and call sites in streaming, tool_call and run_loop to call facets.
   - Keep default behavior equivalent when a facet is absent.
   - Update runtime alignment test helpers enough for new facet shape.

2. RuntimeSession hook/admission facet conversion
   - Convert `HookRuntimeDelegate` into a facet provider.
   - Convert `AgentRunAdmissionRuntimeDelegate` into a tool-policy-only facet.
   - Preserve admission-before-inner-hook order.
   - Update launch plan types and planner composition for hook plus admission tool policy facets.

3. AgentRun mailbox turn-boundary conversion
   - Convert `AgentRunMailboxRuntimeDelegate` into a turn-boundary-only facet.
   - Remove non-mailbox forwarding methods.
   - Preserve hook steering routing through mailbox and boundary drain behavior.
   - Update runtime mailbox port signature if needed so it composes only turn-boundary facet.

4. Specs and verification
   - Update backend session/hook/mailbox specs with facet owner rules.
   - Static search for broad forwarding impls in mailbox/admission code.
   - Run targeted tests listed below.

## Suggested Subagent Split

- Implement A: `agentdash-agent-types` + `agentdash-agent` facet API and agent loop call sites.
- Implement B: runtime-session hook/admission conversion and launch planner/plan composition.
- Implement C: AgentRun mailbox turn-boundary conversion.
- Check: D6-focused review that verifies old broad forwarding paths are actually removed, not hidden.

## Expected Write Scope

- `crates/agentdash-agent-types/src/runtime/delegate.rs`
- `crates/agentdash-agent-types/src/lib.rs`
- `crates/agentdash-agent/src/agent.rs`
- `crates/agentdash-agent/src/agent_loop.rs`
- `crates/agentdash-agent/src/agent_loop/streaming.rs`
- `crates/agentdash-agent/src/agent_loop/tool_call.rs`
- `crates/agentdash-agent/tests/runtime_alignment.rs`
- `crates/agentdash-application-runtime-session/src/session/hook_delegate.rs`
- `crates/agentdash-application-runtime-session/src/session/admission_delegate.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/planner.rs`
- `crates/agentdash-application-runtime-session/src/session/launch/plan.rs`
- `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs`
- `crates/agentdash-application-ports/src/runtime_session_live.rs` if mailbox port signature must change.
- Specs under `.trellis/spec/backend/session/` and `.trellis/spec/backend/hooks/`.

## Validation Commands

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-agent-runtime-delegate-set-cleanup
git diff --check
cargo fmt --check --package agentdash-agent-types --package agentdash-agent --package agentdash-application-runtime-session --package agentdash-application-agentrun
cargo test -p agentdash-agent --test runtime_alignment
cargo test -p agentdash-application-runtime-session hook_delegate --lib
cargo test -p agentdash-application-runtime-session admission_delegate --lib
cargo test -p agentdash-application-agentrun mailbox_runtime_adapter --lib
cargo check -p agentdash-executor
rg -n "impl AgentRuntimeDelegate for AgentRunMailboxRuntimeDelegate|impl AgentRuntimeDelegate for AgentRunAdmissionRuntimeDelegate|DynAgentRuntimeDelegate" crates/agentdash-application-agentrun crates/agentdash-application-runtime-session crates/agentdash-agent crates/agentdash-agent-types
```

If targeted `cargo test -p agentdash-agent runtime_alignment --test runtime_alignment` is too expensive, check worker may replace it with narrower test filters covering changed delegate behavior and report the skipped scope.

## Verification Result

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-agent-runtime-delegate-set-cleanup`: passed.
- `git diff --check`: passed.
- `cargo fmt --check --package agentdash-agent-types --package agentdash-agent --package agentdash-spi --package agentdash-executor --package agentdash-application-ports --package agentdash-application-runtime-session --package agentdash-application-agentrun`: passed.
- `cargo test -p agentdash-agent --test runtime_alignment`: passed, 28 tests.
- `cargo test -p agentdash-application-runtime-session admission_delegate --lib`: passed, 4 tests.
- `cargo test -p agentdash-application-runtime-session hook_delegate --lib`: passed, 13 tests.
- `cargo test -p agentdash-application-agentrun mailbox_runtime_adapter --lib`: passed, 4 tests.
- `cargo check -p agentdash-executor`: passed.
- Static search for exact old broad symbols `AgentRuntimeDelegate`, `DynAgentRuntimeDelegate`, `set_runtime_delegate`, and broad impls in touched production crates: no matches.
- Static search confirmed mailbox adapter only has turn-boundary methods and admission adapter only has tool-policy methods.

Known unrelated warning: `agentdash-workspace-module::workspace_module::tools` has an unused `resolve_workspace_module_visibility` import.
