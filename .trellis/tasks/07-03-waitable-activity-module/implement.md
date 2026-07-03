# Implementation Plan

## Ordered Steps

1. Re-read parent task artifacts, this child PRD/design, and specs.
2. Decide repository shape for `WaitActivity` and write migration if needed.
3. Add domain/application models and repository trait.
4. Implement wait service register/update/wait/notify.
5. Add runtime tool provider exposing `wait`.
6. Register exec running `terminal_id` refs as activities and update on read/status/terminal state.
7. Replace companion/subagent/human private polling with WaitService path.
8. Add mailbox wake adapter and source dedup strategy.
9. Extend workspace snapshot / waiting projection for exec and activity refs.
10. Regenerate frontend contracts if DTOs changed and update UI/tests.
11. Run backend/frontend focused tests and no-/sessions search.

## Validation Commands

```powershell
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

- Activity persistence and lifecycle must be durable enough for mailbox wake semantics.
- `wait` must not drain or consume mailbox messages in a way that bypasses scheduler.
- Gate resolution and companion result dedup must remain idempotent.
- If generated DTOs change, Rust/TypeScript contract generation must be kept in sync.

## Done

This child task is done when an Agent can use one generic `wait` tool to observe exec, companion/subagent, human response and mailbox wake readiness, and all source completions/failures/cancellations update a common activity projection without private per-tool wait protocols.
