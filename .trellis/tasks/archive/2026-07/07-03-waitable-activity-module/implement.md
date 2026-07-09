# Implementation Plan

## Ordered Steps

1. Re-read parent task artifacts, this child PRD/design, specs, and sub-agent review reports.
2. Freeze the first-delivery shape in task docs:
   - `wait` is one runtime tool provider;
   - `activity_ref` reuses `terminal_id` / `gate_id` / `mailbox_message_id`;
   - no first-delivery `wait_activities` ledger table unless a source has no stable root;
   - RuntimeSession remains delivery/trace ref, not owner;
   - mailbox scheduler remains delivery authority.
3. Implement `agentdash-application::wait_activity`:
   - `WaitActivityService`;
   - `WaitRuntimeToolProvider`;
   - `WaitTool`;
   - `WaitActivityRequest`, `WaitActivityResult`, `WaitActivityItem`;
   - bounded preview / refs / cursor / next instruction helpers.
   - module layout:
     - `mod.rs`: declarations and public re-exports only;
     - `provider.rs`: runtime tool catalog binding;
     - `tool.rs`: Agent-facing schema and `RuntimeTool` implementation;
     - `service.rs`: wait orchestration and scope resolution;
     - `types.rs`: request/result/item/context/error types;
     - `sources/exec.rs`, `sources/lifecycle_gate.rs`, `sources/mailbox.rs`: source adapters.
4. Register `WaitRuntimeToolProvider` in `build_session_runtime_tool_composer`, after existing runtime providers, and export it from `agentdash-application`.
5. Add exec source adapter:
   - read `SessionTerminalCache`;
   - use `terminal_id` as `activity_ref` and `source_ref`;
   - map terminal states to `running/completed/failed/cancelled/lost`;
   - return `next = { tool: "shell_exec", operation: "read", terminal_id }`;
   - keep stdout/stderr in `shell_exec read`.
6. Add LifecycleGate source adapter:
   - resolve explicit `gate_id` refs through `LifecycleGateRepository::get`;
   - list current AgentRun open gates through `list_open_for_agent`;
   - retain observed gate refs during a wait call so resolved gates do not disappear after leaving the open-gate projection;
   - filter explicit refs by AgentRun scope while allowing same-run child gates for parent/subagent waits;
   - reuse existing kind mapping for human/subagent/companion/workflow;
   - timeout returns `timed_out` without resolving or closing the gate.
7. Add mailbox source adapter:
   - observe relevant wake/result messages by explicit `mailbox_message_id` or current AgentRun scope;
   - reuse `MailboxSourceIdentity` and existing source dedup conventions;
   - do not drain mailbox or launch/steer/resume from wait.
8. Replace companion/subagent/human private polling wrappers:
   - keep user-facing `companion_request wait=true` and human wait semantics;
   - route blocking wait through `WaitActivityService.wait(activity_refs=[gate_id])`;
   - keep gate payload and mailbox wake delivery as the result/body source.
9. Extend workspace waiting projection only if necessary:
   - keep `ConversationWaitingItemView` for first delivery when possible;
   - project current delivery runtime running/starting exec terminal rows as `kind="exec"` with `source_ref=terminal_id`;
   - regenerate frontend contracts only if DTO shape changes.
10. Add focused tests:
    - wait runtime tool appears in assembled tool catalog;
    - wait timeout does not cancel exec/gate;
    - completed exec returns `shell_exec read` continuation;
    - resolved gate returns bounded summary/refs;
    - companion/human wait uses WaitService path and preserves timeout semantics;
    - mailbox wake/result dedup remains idempotent;
    - workspace projection renders exec/human/subagent wait refs without frontend private payload knowledge.
11. Run backend/frontend focused tests and no-/sessions search.

## Validation Commands

```powershell
cargo test -p agentdash-application wait_activity
cargo test -p agentdash-application runtime_tools
cargo test -p agentdash-application-agentrun mailbox
cargo test -p agentdash-application companion
cargo test -p agentdash-application-runtime-session tool
cargo test -p agentdash-api lifecycle_agents
pnpm --filter app-web test -- MailboxMessageRow
pnpm --filter app-web test -- conversationCommandState
rg -n "/sessions" crates packages
```

Exact package/test targets should be refined after implementation files are selected.

## Risk Points

- First delivery intentionally reuses stable source roots instead of adding a redundant activity ledger. Add a dedicated `wait_activities` repository only when a source lacks a durable/canonical root or when historical wait attempts become a product requirement.
- `activity_ref` must stay natural and singular: `terminal_id`, `gate_id`, or `mailbox_message_id`. Do not introduce public `session_id` / `terminal_id` pairs for exec.
- `wait` must not drain or consume mailbox messages in a way that bypasses scheduler.
- `wait` timeout must not resolve LifecycleGate, terminate shell processes, cancel companion work, or mutate mailbox state.
- Gate resolution and companion result dedup must remain idempotent.
- If generated DTOs change, Rust/TypeScript contract generation must be kept in sync.
- RuntimeSession can be used to resolve the current AgentRun owner through `RuntimeSessionExecutionAnchor`, but it must not become the activity owner or route key.
- No `/sessions/*` wait/control endpoint is allowed.

## Done

This child task is done when an Agent can use one generic `wait` tool to observe exec, companion/subagent, human response and mailbox wake readiness, and all source completions/failures/cancellations update a common activity projection without private per-tool wait protocols.

The first delivery is acceptable without a new durable `wait_activities` table if:

- exec readiness is observable through `terminal_id` / terminal cache and continued through `shell_exec`;
- companion/subagent/human readiness is observable through `LifecycleGate`;
- mailbox wake/result readiness is observable through mailbox messages and scheduler-owned delivery;
- `wait` returns bounded summaries, refs, cursor/next instructions, and never moves large result bodies through the wait result.
