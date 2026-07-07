# Implementation Plan

## Order

1. Stabilize control effect outbox.
   - Add dedup and claim fields to schema, SPI records, store trait and Postgres repository.
   - Implement insert-or-get and claim-and-return.
   - Change `observe_runtime_terminal` to materialize all rows before executing any row.
   - Remove delivery-convergence-time insertion of required wait producer effect.
   - Add unit/repository tests for dedup, claim, retry and dead-letter.

2. Make gate wait policy atomic.
   - Extend gate opening command/path used by companion wait so `GateWaitPolicyEnvelope` is part of initial gate payload.
   - Remove post-open `declare_child_wait_obligation` path.
   - Rename outward-facing wait obligation symbols/logs/phases to gate wait policy / gate producer terminal fallback.
   - Route gate mailbox wake through the final outbox boundary or remove Noop `MailboxWakeDelivery` effect kind.

3. Make hook effects durable only when replayable.
   - Change post-turn handler execution to return `Result`.
   - Register durable handler identity and registry where replay is supported.
   - Skip durable outbox rows for non-replayable hook effects and log diagnostic.
   - Remove or demote `HookRuntimeProjectionChanged` if it has no replay executor.

4. Clarify relay/runtime terminal naming.
   - Rename runtime session terminal kinds/types away from generic terminal resource names.
   - Rename interactive terminal state events/payloads to PTY or interactive terminal terminology.
   - Update frontend generated/consuming types and reducers.

5. Integration and cleanup.
   - Update specs if final contracts differ from current docs.
   - Run targeted Rust tests, migration guard, TypeScript generation/tests as required by changed files.
   - Run full relevant quality checks before finish.

## Sub-agent Dispatch

- Worker A owns outbox schema/store/service and must land first.
- Worker B owns gate wait policy and mailbox wake, integrating after Worker A's outbox contract is stable.
- Worker C owns hook durability, sharing only effect kind/store contracts with Worker A.
- Worker D owns relay/runtime terminal naming and frontend projection, avoiding outbox files.

## Validation Commands

Run the smallest targeted checks after each slice, then broaden:

```powershell
cargo test -p agentdash-application-agentrun
cargo test -p agentdash-application-workflow
cargo test -p agentdash-application-runtime-session
cargo check -p agentdash-api
pnpm run migration:guard
cargo run -p agentdash-agent-protocol --bin generate_backbone_protocol_ts
pnpm --filter app-web test -- session
```

Adjust command list if package names or tests reveal narrower targets.

## Risk Points

- Avoid touching unrelated dirty worktree changes; current branch already has broad uncommitted changes.
- Do not leave effect kinds that replay as Noop while implying durable side effects.
- Do not introduce compatibility fallback for old wait payloads.
- Do not let RuntimeSession trace meta become AgentRun workspace state authority.
