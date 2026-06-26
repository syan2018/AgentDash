# Runner 注册令牌与云端领取流程 - Design

## Decision Summary

第一版只支持 Project-scoped runner registration token。Registration token 只用于无 UI runner 的 enrollment/claim，不是用户 access token，也不是 relay WebSocket token。Runner claim 成功后返回 `backend_id`、`relay_ws_url`、`auth_token`；之后 runner 只用 claim 返回的 `auth_token` 连接 `/ws/backend`。

Token 可重复使用直到 `expires_at` 或 `revoked_at`，以支持服务器重启、服务迁移和凭据恢复。重复 claim 按 `machine_id + project_id + capability_slot` 幂等复用 backend；除非明确请求 rotate backend token，否则保留既有 relay auth token。

## API Surface

Authenticated project token management routes：

- `POST /api/projects/{project_id}/runner-registration-tokens`
- `GET /api/projects/{project_id}/runner-registration-tokens`
- `POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/revoke`
- `POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/rotate`

这些 route 位于 secured API 下，要求当前用户具备项目 backend 管理权限。Create/rotate 响应只返回一次明文 token；list/revoke/metadata 永远不返回明文 token 或 token hash。

Public token-authenticated runner claim route：

- `POST /api/local-runtime/runner/claim`

Runner claim 不提取 `CurrentUser`，不接受浏览器用户 access token。它使用 request body 或 Authorization header 中的 runner registration token 完成自定义鉴权。这个 endpoint 仍在 `/api` 命名空间下，但不能被 secured API middleware 包住。

`/ws/backend` 保持现状，只接受 backend auth token。Registration token 不能连接 relay。

## Token Format And Storage

明文 token 格式：

```text
adrt_<token_id>_<secret>
```

存储字段：

- `id`
- `project_id`
- `name`
- `token_secret_hash`
- `token_prefix`
- `created_by_user_id`
- `expires_at`
- `revoked_at`
- `last_used_at`
- `last_claimed_backend_id`
- `default_capability_slot`
- `machine_policy`
- `created_at`
- `updated_at`

Claim 时解析 token id 与 secret，通过 token id 定位记录，再用 constant-time compare 校验 secret hash。不要存明文，不要把 hash、secret、完整 token 写入日志。不要使用需要扫全表的 bcrypt 查找策略；如果实现选择确定性 digest，secret 必须是高熵随机值。

Token status 为派生状态：`active | expired | revoked`。

## Database

新增 migration：`runner_registration_tokens` 表。

推荐结构：

```sql
CREATE TABLE runner_registration_tokens (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  token_secret_hash TEXT NOT NULL,
  token_prefix TEXT NOT NULL,
  created_by_user_id TEXT NOT NULL,
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ NULL,
  last_used_at TIMESTAMPTZ NULL,
  last_claimed_backend_id TEXT NULL,
  default_capability_slot TEXT NOT NULL DEFAULT 'default',
  machine_policy JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);
```

索引：

- `project_id`
- active token lookup / active token list
- `expires_at` where `revoked_at IS NULL`
- `last_used_at`
- `last_claimed_backend_id` if used by operator view

更新 migration readiness 表清单，确保 boot readiness 能发现缺表。

## Domain And Repository Boundary

新增 `RunnerRegistrationToken` 聚合与 `RunnerRegistrationTokenRepository`。Repository 只负责 token 聚合 CRUD、state transition 与 usage metadata，不直接创建 backend，也不授予 ProjectBackendAccess。

新增 application command/service：`RunnerRegistrationClaimService`。

Claim service 负责跨聚合编排：

1. parse token and verify secret。
2. check active/not expired/not revoked。
3. build project-scoped `LocalBackendClaim`。
4. ensure local backend。
5. idempotently ensure active `ProjectBackendAccess(project_id, backend_id)`。
6. update token `last_used_at` and `last_claimed_backend_id`。
7. return claim DTO。

跨聚合写入不塞进 `BackendRepository` 或 token repository。若实现需要事务一致性，应引入 application command port / unit-of-work 风格的基础设施能力，或提供幂等 repository helper 并对 conflict 做稳定重试/映射。

## Backend Projection Contract

Runner claim 创建或复用 project-scoped backend：

- `share_scope_kind = Project`
- `share_scope_id = project_id`
- `visibility = Shared`
- `owner_user_id = token.created_by_user_id`，用于审计/显示，访问边界仍以 project scope 为准。
- `capability_slot = request.capability_slot ?? token.default_capability_slot ?? "default"`
- `machine_id` / `machine_label` 来自 runner request。
- `client_version` 来自 runner request。
- `executor_enabled` 写入 device/capability projection。

Backend id 复用现有稳定语义：`machine_id + Project + project_id + capability_slot`。如果实现发现现有算法无法直接支持 project-scoped runner，必须回到本 design 更新，不要在 runner 侧本地生成 backend id。

## Claim Flow

1. Runner 发送 `registration_token`、`machine_id`、`machine_label`、`runner_name`、`client_version`、`device`、`executor_enabled`、可选 `capability_slot`。
2. Server 解析 token id/secret，并加载 token metadata。
3. Server constant-time 校验 secret hash。
4. Server 校验 token 未过期、未撤销、所属 project 可用。
5. Server 依据 project scope 与 capability slot ensure backend。
6. Server ensure active ProjectBackendAccess。
7. Server 更新 token usage metadata。
8. Server 返回 runner relay 凭据。
9. Runner 持久化 `backend_id`、`relay_ws_url`、`auth_token` 后连接 `/ws/backend?token=<auth_token>`。

## DTO Contract

DTO 放入 `agentdash-contracts::backend` 并生成 TypeScript：

- `RunnerRegistrationTokenCreateRequest`
- `RunnerRegistrationTokenCreateResponse`
- `RunnerRegistrationTokenMetadataResponse`
- `RunnerRegistrationTokenRotateResponse`
- `RunnerRegistrationTokenRevokeResponse`
- `RunnerRegistrationClaimRequest`
- `RunnerRegistrationClaimResponse`

Create/rotate response 包含明文 registration token 一次。Metadata/list/revoke response 不包含明文 token、secret、hash 或 relay auth token。

Claim response 至少包含：

- `backend_id`
- `name`
- `relay_ws_url`
- `auth_token`
- `machine_id`
- `machine_label`
- `share_scope_kind`
- `share_scope_id`
- `capability_slot`
- `registration_source = runner_registration_token`
- `claimed_at`

## Error Matrix

Claim route：

| Condition | HTTP | Notes |
| --- | --- | --- |
| missing registration token | 401 | fixed message |
| malformed token | 401 | do not reveal token id validity |
| unknown token / hash mismatch | 401 | same class as invalid |
| expired token | 401 | stable code for runner operator |
| revoked token | 401 or 403 | choose one and keep stable |
| invalid machine/device payload | 400 | field-specific validation |
| project/scope denied | 403 | avoid project probing in public claim |
| backend/access conflict after retry | 409 | stable conflict |
| internal/db | 500 | no DB detail |

Authenticated management routes can return precise `404` for token not found under project and `403` for project permission failure.

## Audit And Logs

Persist minimal operational metadata on token row:

- `last_used_at`
- `last_claimed_backend_id`
- optional `last_claim_machine_id` if needed by operator view

Use structured diagnostics for create/revoke/rotate/claim：

- action
- token id
- project id
- actor user id for management actions
- backend id on successful claim
- machine id and capability slot on successful claim

Never log plaintext token, token hash, relay `auth_token`, Authorization header, or full query token.

## Tests

- Token parse/build/hash/status unit tests。
- Repository tests: create/list/get-by-id/get-by-token-id/revoke/rotate/update usage/no plaintext storage。
- Application claim tests: success, idempotent repeated claim, expired/revoked/invalid token, project-scoped backend fields, ProjectBackendAccess ensure。
- API tests: management routes, public claim route auth and error mapping。
- Relay regression: registration token cannot authenticate `/ws/backend`; returned backend auth token can。
- Migration readiness and migration guard。
- Contracts generate/check。

## Handoff

This child delivers the server-side enrollment contract for `local-runner-daemon`:

- claim endpoint path and auth model。
- request/response DTO names and fields。
- retryable vs fatal claim errors。
- backend identity and access side effects。
- token storage and redaction rules。
- validation commands and tests that prove token/relay separation。
