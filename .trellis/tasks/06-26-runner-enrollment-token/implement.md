# Runner 注册令牌与云端领取流程 - Implement

## Commit 1 - Domain Contracts And Repository Ports

- Add `RunnerRegistrationToken` domain entity/value object under backend domain。
- Add token status helpers: active/expired/revoked。
- Add token plaintext parse/build/hash helpers。
- Add `RunnerRegistrationTokenRepository` trait。
- Add application input/output structs for token management and claim。
- Unit test token parse/hash/status without DB。

Validation:

- Targeted domain tests。
- `cargo check -p agentdash-domain` if available through workspace check。

## Commit 2 - Migration And Postgres Repository

- Add migration `NNNN_runner_registration_tokens.sql`。
- Add table, constraints, indexes, project FK。
- Update migration readiness required table list。
- Implement `PostgresRunnerRegistrationTokenRepository`。
- Add repository tests for create/list/get/revoke/rotate/update usage/no plaintext persistence。

Validation:

- `pnpm run migration:guard`
- targeted repository tests。

## Commit 3 - RepositorySet And Bootstrap Wiring

- Add token repository field to application repository set / app state construction。
- Construct Postgres token repository in API bootstrap。
- Ensure readiness checks include `runner_registration_tokens`。
- Keep token repository aggregate-specific; do not place claim orchestration inside backend repo。

Validation:

- `pnpm run backend:check`

## Commit 4 - Application Services

- Implement token management service:
  - create returns plaintext once。
  - list returns metadata only。
  - revoke sets `revoked_at`。
  - rotate revokes/replaces old token and returns new plaintext once。
- Implement `RunnerRegistrationClaimService`:
  - token auth。
  - active/expiry/revoke checks。
  - project-scoped backend ensure。
  - idempotent ProjectBackendAccess ensure。
  - token usage metadata update。
  - claim response assembly。
- Add application tests for success/failure matrix。

Validation:

- targeted application tests。
- `pnpm run backend:check`

## Commit 5 - Contract DTOs And Generated TS

- Add DTOs in `agentdash-contracts::backend`。
- Export DTOs in `generate_ts.rs` backend contracts section。
- Route code consumes contract DTOs rather than long-lived route-local DTOs。
- Run contracts generation and check。

Validation:

- `pnpm run contracts:generate`
- `pnpm run contracts:check`

## Commit 6 - API Routes

- Add secured project token management routes:
  - create/list/revoke/rotate。
- Add public token-authenticated claim route:
  - `POST /api/local-runtime/runner/claim`。
- Ensure management routes require project backend management permission。
- Ensure claim route does not require `CurrentUser` access token。
- Add structured error mapping and no-secret logging。

Validation:

- API route tests for management and claim。
- auth/error mapping tests。

## Commit 7 - Relay And Backend Access Regressions

- Add regression test that registration token is rejected by `/ws/backend`。
- Add test that returned backend auth token authenticates relay。
- Add test that relay register backend_id mismatch is rejected。
- Add assertion that claim creates active ProjectBackendAccess。

Validation:

- targeted relay/backend access tests。

## Commit 8 - End-To-End Validation

- Run `pnpm run migration:guard`。
- Run `pnpm run contracts:check`。
- Run `pnpm run backend:check`。
- Run targeted token repository/application/API/relay tests。
- Run `pnpm run backend:test` if risk or touched modules justify broad verification。

## Handoff Checklist

Before marking this child ready for downstream runner work, write the following into the parent task or final child summary:

- Final claim endpoint and auth model。
- DTO names and generated TS location。
- Claim response fields。
- Fatal vs retryable claim errors。
- Backend field values for project-scoped runner。
- ProjectBackendAccess side effect。
- Validation commands run and notable test names。

## Risk Checks

- Token plaintext appears only in create/rotate responses。
- Database contains no plaintext token。
- Registration token cannot authenticate relay。
- Claim route is not accidentally protected by browser access-token middleware。
- Claim side effects are idempotent under repeated service restarts。
