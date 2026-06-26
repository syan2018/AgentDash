# Research: Runner registration token planning review

- Query: 审阅子任务 `Runner 注册令牌与云端领取流程` 的 design/implement 可执行性，补齐 API、domain、db、auth、claim flow、DTO、错误、审计/日志、测试与父任务 handoff contract。
- Scope: internal
- Date: 2026-06-26

## Findings

### Files Found

- `.trellis/tasks/06-26-runner-enrollment-token/prd.md` - 需求要求 registration token 创建/查看/撤销/轮换、runner claim、过期/撤销、project/backend 可见性一致。
- `.trellis/tasks/06-26-runner-enrollment-token/design.md` - 当前设计已选择 project-scoped token，但缺少更细的 DB schema、token 校验策略、auth 边界、错误矩阵和测试契约。
- `.trellis/tasks/06-26-runner-enrollment-token/implement.md` - 当前实现清单覆盖大类，但还没有按仓储/迁移/DTO/route/测试的小提交拆分。
- `.trellis/tasks/06-26-runner-enrollment-token/implement.jsonl` - 已包含 backend database/repository/contracts/error specs。
- `.trellis/tasks/06-26-runner-enrollment-token/check.jsonl` - 已包含 backend quality 与 cross-layer thinking specs。
- `.trellis/spec/backend/database-guidelines.md` - schema 事实源、迁移历史、migration guard 与 repository readiness 规则。
- `.trellis/spec/backend/repository-pattern.md` - RepositorySet、单聚合 repository、跨聚合 command port 规则。
- `.trellis/spec/backend/error-handling.md` - Domain/Application/API 错误分层与 HTTP 映射。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md` - HTTP DTO 进入 `agentdash-contracts` 并生成 TypeScript 的契约。
- `crates/agentdash-domain/src/backend/entity.rs` - `BackendConfig` 已有 project scope、machine、capability slot、device、last_claimed_at 字段；`LocalBackendClaim` 是现有 ensure 输入。
- `crates/agentdash-domain/src/backend/repository.rs` - backend、runtime health、project backend access repository ports。
- `crates/agentdash-application/src/backend/management.rs` - desktop `/local-runtime/ensure` 的 application use case、稳定 backend id 生成、scope 限制、relay token 生成。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs` - backend UPSERT、auth token 查询、local backend ensure 的 PostgreSQL 实现。
- `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs` - project backend access CRUD 与 inventory repository 实现。
- `crates/agentdash-api/src/routes/backends.rs` - `/local-runtime/ensure` route、relay ws URL 推导和 ensure response。
- `crates/agentdash-api/src/routes/backend_access.rs` - project backend access route、project permission、active access helper。
- `crates/agentdash-api/src/relay/ws_handler.rs` - relay WebSocket token 校验和 backend register 校验。
- `crates/agentdash-api/src/routes.rs` - `/api` secured router 与 `/ws/backend` route 的 middleware 边界。
- `crates/agentdash-contracts/src/backend/contract.rs` - backend/project access DTO 当前归属。
- `crates/agentdash-contracts/src/generate_ts.rs` - backend contract DTO 生成入口。
- `crates/agentdash-infrastructure/migrations/0001_init.sql` - baseline 中已有 `backends`、`project_backend_access`、`backend_workspace_inventory` 表和索引。
- `crates/agentdash-infrastructure/migrations/0019_decouple_workspace_inventory_from_runtime_health.sql` - 将 project backend access 语义收敛到 `explicit_grant` / `workspace_registry` 的迁移示例。
- `crates/agentdash-api/src/routes/routines.rs` - webhook token 只返回一次、bcrypt hash 存储、Bearer 校验的相近模式。
- `crates/agentdash-domain/src/routine/entity.rs` - webhook token hash 存在 domain value object 中的示例。
- `crates/agentdash-api/src/rpc.rs` - `ApiError` HTTP 状态映射。
- `crates/agentdash-infrastructure/src/migration.rs` - migration runner 与 schema readiness 表清单。
- `package.json` - validation commands: `migration:guard`、`contracts:check`、`backend:check`、`backend:test`。

### Related Specs

- `.trellis/spec/backend/database-guidelines.md`: 新 schema 必须新增 migration，普通功能任务不能改已提交 migration；API bootstrap 在 repository 装配前运行 migrations 与 readiness。
- `.trellis/spec/backend/repository-pattern.md`: registration token 是新聚合 repository；claim 同时触碰 token、backend、project backend access，属于 application command/use case，不应塞进某个单一 repository。
- `.trellis/spec/backend/error-handling.md`: repository 层返回 `DomainError`；application/API 保留结构化错误语义；数据库错误不透出给客户端。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: token 管理和 runner claim request/response DTO 应进入 `agentdash-contracts::backend`，并加入 `generate_ts.rs` 的 `backend-contracts.ts` 生成入口。

### Code Patterns

- `/api` 的大多数业务 route 在 `secured_api` 下统一套 `authenticate_request` middleware；`/ws/backend` 不在 `/api` 下，用 query `token` 自行鉴权。Runner claim 若不使用用户 access token，必须明确放在不要求 `CurrentUser` 的 public `/api` route 分支，或在 route 内自定义 token auth，不能误放到 `secured_api` 后再期望 registration token 可用（`crates/agentdash-api/src/routes.rs:47`, `crates/agentdash-api/src/routes.rs:89`, `crates/agentdash-api/src/routes.rs:99`）。
- 现有 desktop ensure route 是 `POST /api/local-runtime/ensure`，handler 依赖 `CurrentUser`，并通过 request headers 构造 `relay_ws_url`（`crates/agentdash-api/src/routes/backends.rs:428`, `crates/agentdash-api/src/routes/backends.rs:434`, `crates/agentdash-api/src/routes/backends.rs:473`）。
- desktop ensure application use case 默认只允许 user scope；project/system scope 目前显式返回 `共享本机 runtime scope 尚未开放创建入口`，所以 runner project-scoped claim 不能直接复用这个函数而不改 scope 语义（`crates/agentdash-application/src/backend/management.rs:135`, `crates/agentdash-application/src/backend/management.rs:156`, `crates/agentdash-application/src/backend/management.rs:259`）。
- backend id 当前由 `machine_id + share_scope_kind + share_scope_id + capability_slot` 稳定 hash 得出；runner claim 应复用这个稳定语义，但 project-scoped `share_scope_kind=Project`、`share_scope_id=project_id`（`crates/agentdash-application/src/backend/management.rs:164`, `crates/agentdash-application/src/backend/management.rs:354`）。
- `ensure_local_backend` 会按 local machine/scope/slot 查找已有 backend，未 rotate 时保留已有 `auth_token`，rotate 时替换；runner claim 的重复领取/轮换策略必须明确是否传 `rotate_token`（`crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:143`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:174`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_repository.rs:179`）。
- relay WebSocket 鉴权只查询 `BackendRepository::get_backend_by_auth_token`，然后校验首条 register 的 `backend_id` 必须等于 token 绑定 backend；registration token 应只用于 claim 交换，不能直接作为 relay token 使用（`crates/agentdash-api/src/relay/ws_handler.rs:25`, `crates/agentdash-api/src/relay/ws_handler.rs:692`, `crates/agentdash-api/src/relay/ws_handler.rs:706`, `crates/agentdash-api/src/relay/ws_handler.rs:731`）。
- project backend access 已有 `get_active_for_project_backend` helper；claim 成功后应保证 token 所属 project 与 backend access 绑定，后续 workspace/canvas/extension 等路径会依赖这个 active access（`crates/agentdash-api/src/routes/backend_access.rs:421`, `crates/agentdash-api/src/routes/backend_access.rs:429`）。
- project backend access 当前手工 create route 先 list 再 create/update；runner claim 并发场景更适合 application command 里用事务或 repository-level idempotent ensure，避免唯一约束冲突直接泄露给 claim（`crates/agentdash-api/src/routes/backend_access.rs:129`, `crates/agentdash-api/src/routes/backend_access.rs:157`, `crates/agentdash-infrastructure/migrations/0001_init.sql:928`）。
- project-scoped backend 可见性已有 authz：`BackendAuthorizationService` 对 `BackendShareScopeKind::Project` 读取 `share_scope_id` 的 UUID 并检查项目权限；claim 创建出的 backend 必须写对 `share_scope_kind` 与 `share_scope_id`（`crates/agentdash-application/src/backend/authorization.rs:93`, `crates/agentdash-application/src/backend/authorization.rs:107`）。
- `BackendConfig` 已有 `owner_user_id`、`visibility`、`share_scope_kind`、`share_scope_id`、`capability_slot`、`device`、`last_claimed_at`，足够承载 project runner backend projection；design 需要固定这些字段在 runner claim 下的取值（`crates/agentdash-domain/src/backend/entity.rs:10`, `crates/agentdash-domain/src/backend/entity.rs:25`, `crates/agentdash-domain/src/backend/entity.rs:31`, `crates/agentdash-domain/src/backend/entity.rs:40`）。
- route-facing backend DTO 目前部分仍在 `crates/agentdash-api/src/dto/backend.rs`，但 project backend access DTO 已在 `agentdash-contracts::backend` 并生成到 `backend-contracts.ts`；新增跨端/runner wire DTO 应放到 contract crate（`crates/agentdash-api/src/dto/backend.rs:24`, `crates/agentdash-contracts/src/backend/contract.rs:209`, `crates/agentdash-contracts/src/generate_ts.rs:379`, `crates/agentdash-contracts/src/generate_ts.rs:398`）。
- migration readiness 清单是显式表列表；新增 `runner_registration_tokens` 表后需要更新 `REQUIRED_POSTGRES_TABLES`，否则 readiness 不会检查新表（`crates/agentdash-infrastructure/src/migration.rs:5`, `crates/agentdash-infrastructure/src/migration.rs:70`）。
- webhook routine 已有“明文 token 只返回一次、存 bcrypt hash、Bearer 校验”的先例；但 registration token 若要按 token hash 索引查找，需使用可索引的高熵 token digest 或 token id + hash，不宜照搬 bcrypt 扫全表（`crates/agentdash-api/src/routes/routines.rs:99`, `crates/agentdash-api/src/routes/routines.rs:104`, `crates/agentdash-api/src/routes/routines.rs:438`, `crates/agentdash-domain/src/routine/entity.rs:71`）。
- API 错误已有 `Unauthorized`、`Forbidden`、`NotFound`、`Conflict` 状态映射；claim 错误矩阵应显式落到这些变体，不依赖错误字符串解析（`crates/agentdash-api/src/rpc.rs:12`, `crates/agentdash-api/src/rpc.rs:59`, `crates/agentdash-api/src/rpc.rs:60`, `crates/agentdash-api/src/rpc.rs:62`）。
- validation 命令已存在：`pnpm run migration:guard`、`pnpm run contracts:check`、`pnpm run backend:check`、`pnpm run backend:test`（`package.json:41`, `package.json:45`, `package.json:37`, `package.json:39`）。

### Current Plan Gaps / Risks, Prioritized

#### P0 - Claim auth boundary is not executable yet

`POST /api/local-runtime/runner/claim` is currently described as a new endpoint, but the design does not say whether it is inside the authenticated `/api` router or a public route with registration-token auth. Because `secured_api` requires user access token, runner claim must be explicitly mounted outside that middleware path or given a custom extractor that does not use `CurrentUser`.

Recommended design text:

```text
Runner claim is a token-authenticated public API under /api. It does not accept browser access tokens and does not extract CurrentUser. Project-scoped token management routes remain under secured API and require ProjectPermission::Edit.
```

#### P0 - Registration token vs relay auth token must be separated

Relay WebSocket currently validates `backend.auth_token` and then requires register payload `backend_id` to match that backend. Registration token should only authorize the claim exchange. Claim returns a backend-specific relay `auth_token`; the runner then connects `/ws/backend?token=<auth_token>` and registers as the returned `backend_id`.

Recommended design text:

```text
Registration token is never accepted by /ws/backend. It can only call runner claim. Claim returns backend_id + relay_ws_url + auth_token; auth_token is the relay token stored on BackendConfig.
```

#### P0 - Token lookup/hash strategy is underspecified

The current design says "store hash", but not how claim locates a token without storing plaintext. Avoid bcrypt full-table scan for registration tokens. Use an opaque token with an identifier plus high-entropy secret, for example:

```text
Plain token format: adrt_<token_id>_<secret>
Stored fields: token_id, token_secret_hash, token_prefix, ...
Lookup: parse token_id, load row by id, constant-time compare SHA-256/HMAC-SHA-256(secret) with stored hash.
```

If the implementation chooses deterministic `token_hash` without token id, the secret must be high entropy and indexed. Bcrypt is useful for one-off verification but not for indexed lookup unless the code accepts scanning all active tokens, which should be avoided.

#### P0 - Cross-aggregate claim needs an application command boundary

Claim validates registration token, creates/updates `backends`, ensures `project_backend_access`, and updates token `last_used_at`. That crosses at least token repository, backend repository, and project backend access repository. Per repository spec, do this as an application use case / command port, with infrastructure support for transaction or idempotent repository methods. Do not put the whole flow into `BackendRepository` or `RunnerRegistrationTokenRepository`.

Recommended design text:

```text
Application layer owns RunnerRegistrationClaimService. Repositories remain aggregate-specific. The service validates token state, builds a LocalBackendClaim with project scope, ensures the backend, ensures active ProjectBackendAccess idempotently, updates token usage metadata, and returns the claim DTO.
```

#### P0 - Project-scoped backend field values must be fixed

The plan says project scoped, but not exact backend fields. Recommended first-version contract:

```text
share_scope_kind = Project
share_scope_id = project_id
visibility = Shared
owner_user_id = token.created_by_user_id for audit/display, or None if ownership must be purely project-scoped; choose one in design and keep project scope authoritative.
capability_slot = request.capability_slot or token.default_capability_slot or "default"
profile_id = "runner:<token_id>" or stable runner profile value if needed for diagnostics
device.executor_enabled = request.executor_enabled
device.client_version = request.client_version
```

Prefer `owner_user_id=created_by_user_id` for audit while making visibility/project scope the access boundary.

#### P0 - Repeat claim and token rotation semantics are incomplete

The acceptance criteria mention duplicate use strategy, but design does not decide it. For server runner, one-time token would make restart painful. Recommended first-version behavior:

```text
Registration tokens are reusable until expires_at or revoked_at, but each claim is idempotent for machine_id + project_id + capability_slot. Repeated claim reuses the stable backend and existing relay auth_token unless request.rotate_backend_token=true or token rotation endpoint is used.
```

If product requires single-use later, that must be a different token policy field because it changes runner operations.

#### P1 - Token management API needs metadata and no-secret response contract

List/get should never return secret. Create/rotate should return plaintext once. Metadata should include enough for UI/operator diagnosis:

```text
id, project_id, name, token_prefix, created_by_user_id, created_at, updated_at,
expires_at, revoked_at, last_used_at, last_claimed_backend_id,
default_capability_slot, machine_policy, status
```

`status` can be derived (`active | expired | revoked`) in DTO or returned as a field.

#### P1 - RepositorySet/bootstrap changes are missing from implement plan

New repository requires:

```text
agentdash-domain::backend::RunnerRegistrationTokenRepository
agentdash-infrastructure::persistence::postgres::PostgresRunnerRegistrationTokenRepository
agentdash-application::RepositorySet field
agentdash-api bootstrap repository construction
migration readiness REQUIRED_POSTGRES_TABLES update
```

#### P1 - DTO generation path is missing

Add runner token DTOs to `agentdash-contracts::backend::contract.rs`, import/export in `generate_ts.rs`, then run `pnpm run contracts:generate` and `pnpm run contracts:check`. Avoid adding long-lived route-local DTOs in `agentdash-api/src/dto/backend.rs` for this surface.

#### P1 - Error matrix should be explicit

Recommended error matrix:

| Condition | HTTP | Message shape |
| --- | --- | --- |
| Missing registration token | 401 | `registration token required` |
| Malformed token | 401 | `registration token invalid` |
| Unknown token id/hash mismatch | 401 | `registration token invalid` |
| Revoked token | 401 or 403 | `registration token revoked` if explicit operator feedback is preferred; otherwise generic invalid |
| Expired token | 401 | `registration token expired` |
| Token project missing/deleted | 404 or 403 | Prefer 403/invalid for claim to avoid project probing |
| Project scope mismatch in management route | 404 | token not found under project |
| Invalid machine/device payload | 400 | field-specific validation |
| Backend/project access ensure conflict after retry | 409 | stable conflict |
| DB/internal | 500 | fixed internal message |

For claim, avoid revealing whether a token id exists when hash mismatches. For management routes authenticated as project editor, precise not-found/revoked metadata is fine.

#### P1 - Audit/logging is not designed

No dedicated audit repository was found. Use structured diagnostics for create/revoke/rotate/claim without logging token plaintext or token hash. Persist minimal token usage metadata (`last_used_at`, `last_claimed_backend_id`, optional `last_claim_machine_id`) in token table. If durable audit is required, design should explicitly choose an existing event/audit store; none is currently obvious for this backend domain.

#### P1 - Test plan needs concrete files/levels

Minimum tests:

- domain/application unit tests for token creation, token parsing/hash comparison, active/expired/revoked state.
- postgres repository tests for create/list/get-by-id/get-by-token-id/revoke/rotate/update-last-used, unique constraints, no plaintext storage.
- application claim tests for idempotent repeated claim, revoked/expired failures, project-scoped backend fields, ProjectBackendAccess active ensure.
- API route tests for create/list/revoke/rotate and public claim auth/error mapping.
- relay auth regression proving registration token does not authenticate `/ws/backend`; only returned backend auth token does.
- migration readiness/migration guard.
- contract generation check.

### Recommended Design Structure

Paste-ready structure:

```markdown
# Runner 注册令牌与云端领取流程 - Design

## Decision Summary
- 第一版只支持 Project-scoped registration token。
- Registration token 只用于 claim；relay WebSocket 只接受 claim 返回的 backend auth_token。
- Token 可重复使用直到 expires_at 或 revoked_at；claim 按 machine_id + project_id + capability_slot 幂等复用 backend。

## API
### Authenticated Project Token Management
- POST /api/projects/{project_id}/runner-registration-tokens
- GET /api/projects/{project_id}/runner-registration-tokens
- POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/revoke
- POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/rotate
- Permission: ProjectPermission::Edit.
- Create/rotate returns plaintext token exactly once; list/revoke responses never return secret.

### Public Runner Claim
- POST /api/local-runtime/runner/claim
- Authentication: registration token only, not CurrentUser access token.
- Response returns backend_id, name, relay_ws_url, auth_token, machine fields, share scope fields, capability_slot.
- /ws/backend continues to authenticate with backend auth_token.

## Domain
- RunnerRegistrationToken entity:
  - id, project_id, name, token_secret_hash, token_prefix
  - created_by_user_id, created_at, updated_at
  - expires_at, revoked_at, last_used_at
  - default_capability_slot, machine_policy, last_claimed_backend_id
- Token status is derived: active, expired, revoked.
- Token plaintext format: adrt_<token_id>_<secret>.
- Secret hash comparison is constant-time and never logs plaintext/hash.

## DB
- Add migration NNNN_runner_registration_tokens.sql.
- Table runner_registration_tokens:
  - id text primary key
  - project_id text not null references projects(id) on delete cascade
  - name text not null
  - token_secret_hash text not null
  - token_prefix text not null
  - created_by_user_id text not null
  - expires_at timestamptz not null
  - revoked_at timestamptz null
  - last_used_at timestamptz null
  - last_claimed_backend_id text null
  - default_capability_slot text not null default 'default'
  - machine_policy text not null default '{}'
  - created_at/updated_at timestamptz not null
- Indexes:
  - project_id
  - expires_at where revoked_at is null
  - last_used_at
  - last_claimed_backend_id if used for operator view
- Update migration readiness required table list.

## Repository / Application Boundary
- Add RunnerRegistrationTokenRepository for token aggregate CRUD/state transitions.
- Add RunnerRegistrationClaimService in application layer:
  - parse and verify token
  - check active/not expired/not revoked
  - build project-scoped LocalBackendClaim
  - ensure backend
  - ensure active ProjectBackendAccess idempotently
  - update token usage metadata
  - return claim result
- If atomicity is required, use an explicit command port/unit-of-work rather than mixing cross-aggregate writes into a single repository.

## Claim Flow
1. Runner sends registration_token, machine_id, machine_label, runner_name, client_version, device, executor_enabled, optional capability_slot.
2. Server parses token_id/secret and loads token metadata.
3. Server verifies secret hash and active state.
4. Server derives backend_id from machine_id + Project + project_id + capability_slot.
5. Server ensures local backend with project scope:
   - share_scope_kind=Project
   - share_scope_id=project_id
   - visibility=Shared
   - owner_user_id=created_by_user_id
6. Server ensures ProjectBackendAccess(project_id, backend_id) active.
7. Server updates last_used_at and last_claimed_backend_id.
8. Server returns relay credentials.

## DTO
- Add DTOs under agentdash-contracts::backend:
  - RunnerRegistrationTokenCreateRequest
  - RunnerRegistrationTokenCreateResponse
  - RunnerRegistrationTokenMetadataResponse
  - RunnerRegistrationTokenRotateResponse
  - RunnerRegistrationTokenRevokeResponse
  - RunnerRegistrationClaimRequest
  - RunnerRegistrationClaimResponse
- Add them to generate_ts backend-contracts.ts.

## Errors
- Claim missing/malformed/unknown token -> 401.
- Expired/revoked token -> 401/403, fixed wording chosen in error matrix.
- Invalid claim payload -> 400.
- Project token management not found under project -> 404.
- Project editor permission failure -> 403.
- Idempotent backend/access conflict after retry -> 409.
- DB/internal -> fixed 500.

## Audit / Logs
- Log token id, project_id, backend_id, action, actor user id for management actions, and machine_id/capability_slot for successful claim.
- Never log plaintext token, token hash, or returned relay auth_token.
- Persist last_used_at and last_claimed_backend_id on token metadata.

## Tests
- Repository tests for token CRUD/state transitions/hash lookup.
- Application claim tests for success/idempotency/revoked/expired/invalid token/project scope/backend access.
- API tests for management routes and public claim route.
- Relay regression that registration token cannot connect /ws/backend.
- migration:guard, contracts:check, backend:check, backend:test.
```

### Recommended Implement Checklist, Split Into Small Commits

Paste-ready execution plan:

```markdown
# Runner 注册令牌与云端领取流程 - Implement

## Commit 1 - Domain contracts and repository ports
- Add RunnerRegistrationToken domain entity/value objects under backend domain.
- Add token status helpers: active/expired/revoked.
- Add token plaintext parse/build/hash helpers.
- Add RunnerRegistrationTokenRepository trait.
- Add application input/output structs for token management and claim.
- Unit test token parse/hash/status without DB.

## Commit 2 - Migration and Postgres repository
- Add new migration NNNN_runner_registration_tokens.sql.
- Add table, constraints, indexes, and FK to projects.
- Update migration readiness table list.
- Implement PostgresRunnerRegistrationTokenRepository.
- Add repository tests for create/list/get/revoke/rotate/update usage and no plaintext persistence.
- Run pnpm run migration:guard.

## Commit 3 - RepositorySet/bootstrap wiring
- Add token repo field to RepositorySet and derived repository-set conversions if needed.
- Construct PostgresRunnerRegistrationTokenRepository in API bootstrap.
- Ensure readiness checks include runner_registration_tokens.
- Run pnpm run backend:check.

## Commit 4 - Application services
- Implement authenticated token management service:
  - create returns plaintext once
  - list returns metadata only
  - revoke sets revoked_at
  - rotate revokes/replaces old token and returns new plaintext once
- Implement RunnerRegistrationClaimService:
  - token auth
  - active state checks
  - project-scoped backend ensure
  - idempotent ProjectBackendAccess ensure
  - token usage metadata update
- Add application tests for success and failure matrix.

## Commit 5 - Contract DTOs and generated TS
- Add DTOs in agentdash-contracts::backend.
- Export DTOs in generate_ts.rs backend-contracts.ts section.
- Replace route-local DTOs for this feature with contract DTOs.
- Run pnpm run contracts:generate.
- Run pnpm run contracts:check.

## Commit 6 - API routes
- Add secured project token management routes under /api/projects/{project_id}/runner-registration-tokens.
- Add public registration-token claim route under /api/local-runtime/runner/claim.
- Ensure management routes use ProjectPermission::Edit.
- Ensure claim route does not require CurrentUser access token.
- Add structured error mapping and no-secret logging.

## Commit 7 - Relay and backend access regressions
- Add test that registration token is rejected by /ws/backend.
- Add test that returned backend auth_token still authenticates relay and backend_id mismatch is rejected.
- Add project backend access assertion after claim.

## Commit 8 - End-to-end validation
- Run pnpm run migration:guard.
- Run pnpm run contracts:check.
- Run pnpm run backend:check.
- Run targeted backend tests for token repository/application/API/relay auth.
- Run pnpm run backend:test if time/risk requires broad verification.
```

### Parent Task Handoff Contract

Recommended output contract for the parent task:

```markdown
## Child Output Contract: Runner 注册令牌与云端领取流程

This child delivers the server-side enrollment token and claim contract for service runners.

### API Contract
- Authenticated:
  - POST /api/projects/{project_id}/runner-registration-tokens
  - GET /api/projects/{project_id}/runner-registration-tokens
  - POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/revoke
  - POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/rotate
- Public token-authenticated:
  - POST /api/local-runtime/runner/claim
- /ws/backend remains authenticated only by claim-returned backend auth_token.

### DTO Contract
- Generated DTOs are exported from packages/app-web/src/generated/backend-contracts.ts.
- Create/rotate responses include plaintext registration token exactly once.
- List/revoke/metadata responses never include plaintext token or token hash.
- Claim response includes backend_id, relay_ws_url, auth_token, machine fields, share scope fields, capability_slot.

### Domain/DB Contract
- First version token scope is project-only.
- registration token table stores no plaintext token.
- Claim creates/reuses a project-scoped local backend:
  - share_scope_kind=project
  - share_scope_id={project_id}
  - visibility=shared
  - capability_slot stable from request/token/default
- Claim ensures active ProjectBackendAccess(project_id, backend_id).

### Security Contract
- Registration token is not a user access token.
- Registration token is not a relay token.
- Claim route does not require browser session auth.
- Project token management requires ProjectPermission::Edit.
- Logs never include plaintext registration token, token hash, or relay auth_token.

### Validation Contract
- migration guard passes.
- contract check passes.
- backend check passes.
- Tests cover create/list/revoke/rotate, claim success, invalid/malformed/expired/revoked token, repeated claim idempotency, project backend access, and relay token separation.

### Integration Notes For Downstream Runner Task
- Runner receives and persists only the returned backend_id, relay_ws_url, auth_token, machine identity, and capability slot after claim.
- Runner startup first calls claim with registration token, then connects /ws/backend using returned auth_token.
- Runner should treat registration token as enrollment credential and backend auth_token as relay credential.
```

### External References / Versions

- No external web documentation was needed for this planning review.
- Local dependency versions observed:
  - `sqlx = 0.8` with PostgreSQL/chrono/uuid/json features (`Cargo.toml:55`).
  - `chrono = 0.4`, `uuid = 1.0` (`Cargo.toml:72`, `Cargo.toml:73`).
  - `ts-rs = 11.1` for contract generation (`Cargo.toml:120`).
  - `bcrypt = 0.16` available in `agentdash-api` for webhook token precedent (`crates/agentdash-api/Cargo.toml:58`).
  - `sha2 = 0.10` available in application/domain/api/local crates for deterministic hashing (`crates/agentdash-application/Cargo.toml:46`, `crates/agentdash-api/Cargo.toml:61`, `crates/agentdash-domain/Cargo.toml:14`).

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task pointer. The user explicitly supplied `.trellis/tasks/06-26-runner-enrollment-token`, so research was written there.
- No existing runner registration token model/repository/table was found.
- No dedicated durable audit-log repository was found for backend enrollment actions; current recommendation uses structured diagnostics plus token metadata fields unless a parent task requires a new audit store.
- I did not modify `prd.md`, `design.md`, `implement.md`, `implement.jsonl`, `check.jsonl`, specs, or product code.
