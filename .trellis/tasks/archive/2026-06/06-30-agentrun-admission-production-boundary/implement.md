# Implementation Plan

## Operating Rules

- Follow Trellis workflow and keep this task as the active task before code edits.
- Subagent prompts must start with `Active task: .trellis/tasks/06-30-agentrun-admission-production-boundary`.
- Every subagent must prioritize cleanup of old wrong paths over new feature surface. The original review goal is first-principles convergence: remove duplicated facts, concept forks, and misleading boundaries instead of layering a parallel abstraction over them.
- Implementation subagents must not run large Rust builds or broad suites. They may run `rg`, inspect tests, run `cargo fmt --check` on touched packages, or run small targeted tests only when cheap. Main/check phase owns expensive Rust compilation.
- Do not touch unrelated dirty worktree changes.

## Work Items

1. Product port implementation
   - Inspect repositories already injected near `runtime_session_effective_capability_port`.
   - Implement `AgentRunEffectiveCapabilityPort` in `agentdash-application-agentrun`.
   - Reuse `AgentRunEffectiveCapabilityService` for projection/admission instead of duplicating grant logic.

2. Runtime delegate admission bridge
   - Locate RuntimeSession launch composition where hook delegate and mailbox delegate are created.
   - Add AgentRun admission delegate/adapter so `before_tool_call` calls port `admit_tool`.
   - Preserve existing hook delegate behavior; define deterministic order. Prefer admission before provider/tool execution and avoid provider-local permission checks.

3. RuntimeSession visible-state port cleanup
   - Audit `RuntimeSessionEffectiveCapabilityPort` callers.
   - Delete or rename capability-state-only execution terminology if the change is local enough.
   - If retained, make the naming/comments/spec state it is schema-facing visible state only.

4. Tests
   - Add/extend AgentRun effective capability tests for product adapter allow/deny and frame-scoped grants.
   - Add/extend agent loop/runtime delegate test proving deny prevents tool execution.
   - Add static/rg check for non-test `admit_tool` call and no grant projection mutation into `CapabilityState`.

5. Specs
   - Update `.trellis/spec/backend/capability/architecture.md` and/or `.trellis/spec/backend/capability/tool-capability-pipeline.md`.
   - Record why AgentRun admission is the production owner and why provider `CapabilityState` gates are only exposure/local invariants.

## Suggested Subagent Split

- Implement A: AgentRun port/product adapter and tests.
- Implement B: RuntimeSession/agent-loop delegate bridge and deny-prevents-execute test.
- Check: targeted review after merge, focused on old-path cleanup and admission call placement.

## Validation Commands

Run after implementation, adjusted to touched packages:

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-agentrun-admission-production-boundary
git diff --check
cargo fmt --check --package agentdash-application-agentrun --package agentdash-application-runtime-session --package agentdash-agent --package agentdash-agent-types --package agentdash-application-ports
cargo test -p agentdash-application-agentrun effective_capability --lib
cargo test -p agentdash-agent runtime_alignment --test runtime_alignment
rg -n "admit_tool\\(" crates/agentdash-application-agentrun crates/agentdash-application-runtime-session crates/agentdash-agent crates/agentdash-agent-types
rg -n "execution_capability_state_for_runtime_session|grant_projection.*CapabilityState|apply_to_execution_capability_state" crates/agentdash-application-agentrun crates/agentdash-application-runtime-session crates/agentdash-application-ports
```

If the broad runtime alignment test is too slow locally, run the narrow test name that covers denial short-circuit and document the skipped broader command.

## Verification Result

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-agentrun-admission-production-boundary`
- `git diff --check`
- `cargo fmt --check --package agentdash-application-ports --package agentdash-application-agentrun --package agentdash-application-runtime-session --package agentdash-agent --package agentdash-api`
- `cargo test -p agentdash-application-agentrun effective_capability --lib`
- `cargo test -p agentdash-application-runtime-session admission_delegate --lib`
- `cargo test -p agentdash-agent deny_decision_keeps_tool_unexecuted --test runtime_alignment`
- `cargo check -p agentdash-application-runtime-session`
- `cargo check -p agentdash-api`
- Static `rg` checks confirmed:
  - no `execution_capability_state_for_runtime_session` residual;
  - no grant projection write-back into `CapabilityState`;
  - production `admit_tool` call exists in RuntimeSession admission delegate.

Observed warning: `agentdash-workspace-module` currently has an unrelated unused import warning for `resolve_workspace_module_visibility`. It was not changed in this D1 slice.

Broad workspace Rust compile, clippy and full test suites were intentionally skipped for this implementation slice because the requested workflow limits large Rust builds until broader check/integration phases.
