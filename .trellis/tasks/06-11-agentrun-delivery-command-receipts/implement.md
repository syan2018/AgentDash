# 实施计划

1. 定义 delivery command receipt domain type 和 repository trait。
2. 新增 forward migration 和 Postgres repository。
3. 更新 test-support memory persistence。
4. 在 ProjectAgentSessionStartService 入口创建/查询 start receipt。
5. 在 AgentRunMessageService 入口创建/查询 message receipt。
6. 将 receipt accepted 写入 launch accepted commit 或 command dispatch accepted 点。
7. 将 terminal failure 写入 receipt。
8. 增加 focused tests：
   - Project Agent start duplicate。
   - AgentRun message duplicate。
   - digest mismatch。
   - terminal failure retry。

## Validation

- `pnpm run migration:guard`
- `cargo test -p agentdash-application command_receipt`
- `cargo test -p agentdash-infrastructure command_receipt`

## Implementation Notes

- Added `AgentRunDeliveryCommandReceipt` domain model, status enum, accepted refs, claim result, and repository trait under the workflow domain.
- Added forward migration `0011_agent_run_delivery_command_receipts.sql` with an independent receipt table keyed by `(scope_kind, scope_key, client_command_id)`.
- Added PostgreSQL and application test-support memory implementations for the receipt repository.
- Wired `ProjectAgentSessionStartService` to claim a `project_agent_start` receipt before materialization; accepted duplicates now reuse the original run / agent / frame / runtime / turn refs.
- Wired `AgentRunMessageService` to claim an `agent_run_message` receipt before launch delivery; accepted duplicates now return the original turn refs without re-delivery.
- Request digest uses canonicalized JSON plus SHA-256 over command kind, target refs, input, subject ref, executor config, and relevant frame/runtime refs.
- Terminal delivery failures mark the receipt `terminal_failed`; retrying the same command id returns the same terminal failure instead of dispatching again.

## Validation Results

- `cargo check -p agentdash-domain -p agentdash-application -p agentdash-infrastructure -p agentdash-api` passed.
- `cargo test -p agentdash-application agent_message -- --nocapture` passed: 5 tests.
- `cargo test -p agentdash-application project_agent_session_start -- --nocapture` passed: 2 tests.
- `cargo test -p agentdash-infrastructure command_receipt -- --nocapture` passed: 2 tests; PostgreSQL roundtrip bodies skipped because `TEST_DATABASE_URL` / `DATABASE_URL` is not configured in this environment.
- `pnpm run migration:guard` passed.
- `cargo clippy -p agentdash-domain -p agentdash-application -p agentdash-infrastructure -p agentdash-api -- -D warnings` passed.
- `pnpm run contracts:check` passed.
