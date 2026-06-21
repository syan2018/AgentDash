# Research: CB04-F backend access command conversion owner review

- Query: 为 CB04-F Backend access command conversion owner review 做代码级审计，检查 contracts backend access/status/mode reverse conversions 和 API/application call sites。
- Scope: internal
- Date: 2026-06-21

## Findings

## Scope And Source Material

`python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)` / `Source: none`；本报告按用户显式指定目录 `.trellis/tasks/06-21-cb04-backend-access-command-conversion/` 写入。

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-21-cb04-backend-access-command-conversion/prd.md` | CB04-F 目标：审计 backend access/status/mode reverse conversion，必要时把 command parsing 收口到 API adapter/application command boundary。 |
| `.trellis/tasks/06-21-cb04-backend-access-command-conversion/design.md` | 边界规则：response DTO projection 可留在 contracts；command/status parsing 属于 API/application boundary。 |
| `.trellis/tasks/06-21-cb04-backend-access-command-conversion/implement.md` | 执行步骤和 validation baseline；明确先 audit call sites 再决定迁移。 |
| `.trellis/tasks/06-21-cb04-backend-access-command-conversion/implement.jsonl` | 当前 implement context：parent owner-map、CB03 owner-map research、frontend-backend contracts spec。 |
| `.trellis/tasks/06-21-cb04-backend-access-command-conversion/check.jsonl` | 当前 check context：owner-map 和 frontend-backend contracts spec。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md` | Owner rules：API adapter owns request parsing；allowed projection is narrow outward-only mapping。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md` | CB04-F candidate：backend access response projection 可保留，status/mode reverse conversion 需 review then migrate。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | Contract crate 是 HTTP DTO / generated TS owner；route 需要内部模型时 API layer owns mapping。 |
| `.trellis/spec/cross-layer/project-backend-workspace-routing.md` | Project backend access、inventory registration 与 workspace candidates 的跨层契约。 |
| `crates/agentdash-contracts/src/backend/contract.rs` | Backend/access/inventory wire DTO、generated enum、domain -> DTO projection，以及两个 DTO -> domain reverse impl。 |
| `crates/agentdash-api/src/routes/backend_access.rs` | Backend access HTTP routes；唯一发现的 request DTO -> domain status/mode call site。 |
| `crates/agentdash-application/src/workspace/backend_sync.rs` | Workspace candidate application read model，携带 domain inventory status。 |
| `crates/agentdash-domain/src/backend/entity.rs` | Backend access/status/mode 和 inventory status/source domain enum/source of truth。 |
| `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs` | DB string <-> domain enum mapping；属于 persistence adapter，不是 contracts reverse conversion。 |
| `packages/app-web/src/services/backendAccess.ts` | Frontend service 直接消费 generated request/response DTO。 |
| `packages/app-web/src/generated/backend-contracts.ts` | Generated TypeScript backend contract output。 |

## Conversion Classification

### Outbound projection

这些转换是 internal/domain/application fact -> browser-facing DTO，符合 owner-map 的 allowed projection conversion：

| Conversion | Classification | Evidence |
| --- | --- | --- |
| `agentdash_domain::backend::BackendType` -> `contracts::backend::BackendType` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:13` |
| `BackendVisibility` / `BackendShareScopeKind` / `RuntimeHealthStatus` -> contract enum | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:30`, `:48`, `:69` |
| `BackendConfig` -> `BackendResponse` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:143` |
| domain `ProjectBackendAccessStatus` -> DTO `ProjectBackendAccessStatus` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:184` |
| domain `ProjectBackendAccessMode` -> DTO `ProjectBackendAccessMode` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:210` |
| `ProjectBackendAccess` -> `ProjectBackendAccessResponse` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:283`; route list/create/update all return via `ProjectBackendAccessResponse::from` at `crates/agentdash-api/src/routes/backend_access.rs:64`, `:159`, `:177`, `:225` |
| domain `BackendWorkspaceInventoryStatus` / `Source` -> DTO enum | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:311`, `:335` |
| `BackendWorkspaceInventory` -> `BackendWorkspaceInventoryResponse` | outbound projection | `crates/agentdash-contracts/src/backend/contract.rs:375`; route returns via `BackendWorkspaceInventoryResponse::from` at `crates/agentdash-api/src/routes/backend_access.rs:276`, `:356`, `:404` |
| application `WorkspaceInventoryCandidate` -> contract `WorkspaceInventoryCandidate` | API adapter outbound projection | application model defined at `crates/agentdash-application/src/workspace/backend_sync.rs:15`; API mapper at `crates/agentdash-api/src/routes/backend_access.rs:620` maps status with outbound `BackendWorkspaceInventoryStatus::from` at `:631`; contract DTO field is at `crates/agentdash-contracts/src/workspace/contract.rs:163` |

保留理由：以上转换没有 request intent、patch semantics 或 validation/defaulting；它们只把 stable domain/application facts 投影成 generated wire DTO。`frontend-backend-contracts.md` 也允许 `agentdash-api` 使用 contract crate 作为 route output，并由 generated TS 进入 frontend。

### Incoming command parsing

发现的 incoming/request paths：

| Request / call site | Classification | Evidence |
| --- | --- | --- |
| `CreateProjectBackendAccessRequest` | incoming request DTO, but no enum reverse conversion | request DTO at `crates/agentdash-contracts/src/backend/contract.rs:226`; route trims `backend_id` and builds `ProjectBackendAccess::new` at `crates/agentdash-api/src/routes/backend_access.rs:120`, `:162`; status/mode stay domain defaults from `ProjectBackendAccess::new` at `crates/agentdash-domain/src/backend/entity.rs:349` |
| `UpdateProjectBackendAccessRequest.status` | incoming command parsing; reverse conversion currently lives in contracts | request field at `crates/agentdash-contracts/src/backend/contract.rs:247`; reverse impl at `:194`; route writes command value into domain entity via `status.into()` at `crates/agentdash-api/src/routes/backend_access.rs:196` |
| `UpdateProjectBackendAccessRequest.access_mode` | incoming command parsing; reverse conversion currently lives in contracts | request field at `crates/agentdash-contracts/src/backend/contract.rs:250`; reverse impl at `:218`; route writes command value into domain entity via `access_mode.into()` at `crates/agentdash-api/src/routes/backend_access.rs:199` |
| `RegisterBackendWorkspaceInventoryRequest` | incoming request DTO with string normalization only | request DTO at `crates/agentdash-contracts/src/backend/contract.rs:406`; route validates `root_ref` at `crates/agentdash-api/src/routes/backend_access.rs:382`; no DTO enum reverse conversion |
| `BrowseAccessDirectoryRequest` | route-local request DTO, not contract reverse conversion | imported from `crate::dto` at `crates/agentdash-api/src/routes/backend_access.rs:37`; route maps to runtime gateway input at `:465` |

Frontend call sites use generated DTOs directly and do not add another backend access conversion layer: `packages/app-web/src/services/backendAccess.ts:2` imports generated request/response types, `:25` posts create payload, `:32` patches update payload, and `:66` posts register inventory payload. Generated status/mode values are emitted in `packages/app-web/src/generated/backend-contracts.ts:34`, `:38`, `:44`.

Persistence mappings are not part of this contract-boundary migration: `str_to_access_status`, `str_to_access_mode`, `str_to_inventory_status`, and `str_to_inventory_source` live in the Postgres repository at `crates/agentdash-infrastructure/src/persistence/postgres/project_backend_access_repository.rs:423`, `:438`, `:451`, `:467`; those convert DB storage strings to domain enums inside the persistence adapter.

## Required Reverse Conversion Migration

There are reverse conversions that participate in request command parsing:

- `impl From<ProjectBackendAccessStatus> for agentdash_domain::backend::ProjectBackendAccessStatus` at `crates/agentdash-contracts/src/backend/contract.rs:194`
- `impl From<ProjectBackendAccessMode> for agentdash_domain::backend::ProjectBackendAccessMode` at `crates/agentdash-contracts/src/backend/contract.rs:218`
- Their only observed call sites are `status.into()` and `access_mode.into()` inside `update_project_backend_access` at `crates/agentdash-api/src/routes/backend_access.rs:196` and `:199`

Recommendation: migrate these two reverse impls out of `agentdash-contracts` if CB04-F proceeds to code. This is not a wide architectural migration; it is a route-adapter cleanup. The DTO enums should remain contract-owned for generated request/response shape, and the domain -> DTO impls should remain because response projection is narrow outward mapping.

No application-layer command builder currently consumes `UpdateProjectBackendAccessRequest`, so migration should not introduce an application dependency on contracts. Keep the mapping route-local unless a dedicated backend access command use case is created later.

## Suggested Write Set

Recommended implementation file set:

| File | Action |
| --- | --- |
| `crates/agentdash-contracts/src/backend/contract.rs` | Remove only the two DTO -> domain impls for `ProjectBackendAccessStatus` and `ProjectBackendAccessMode`; keep DTO definitions, TS derives, and domain -> DTO projection impls. |
| `crates/agentdash-api/src/routes/backend_access.rs` | Add small route-local mapping helpers for update request status/mode and replace `status.into()` / `access_mode.into()`. |

No generated TypeScript change is expected because wire DTO names/fields/enum variants stay identical. `crates/agentdash-contracts/src/generate_ts.rs` should not need edits unless compile errors reveal export coupling.

If the task is accepted as audit-only, the alternate conclusion is: no broad code migration is needed; only the two route-local reverse enum helpers are worth cleaning up for owner-map consistency.

## Focused Validation Commands

```powershell
cargo check -p agentdash-contracts -p agentdash-api
cargo test -p agentdash-api backend_access --lib
cargo test -p agentdash-contracts backend
pnpm run contracts:check
pnpm run frontend:check
```

Notes:

- `cargo test -p agentdash-api backend_access --lib` may currently discover no focused route tests; if none exist, use `cargo test -p agentdash-api --lib backend`.
- `pnpm run contracts:check` should remain green with no generated diff; if it changes `backend-contracts.ts`, the migration accidentally changed wire shape.

## First-Wave Parallel Suitability

Suitable for a low-risk parallel implementation wave after this research result, but not a priority blocker.

Reasons:

- The code write set is two Rust files and does not overlap CB04-A MCP preset or CB04-E Routine/LLM/Settings reverse conversion.
- No database migration, frontend contract shape change, or generated TS change is expected.
- The only reverse command parsing call sites are in one route function.

Coordination caveat: avoid running in parallel with another worker editing `crates/agentdash-contracts/src/backend/contract.rs` or broad contract generation plumbing.

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md`

## Caveats / Not Found

- 未修改业务代码，未运行 validation commands；本轮只做 owner review research。
- 未发现 application 层直接消费 backend access request DTO。
- 未发现除 `update_project_backend_access` 之外的 backend access status/mode DTO -> domain call site。
- 未发现需要迁移的 inventory status/source reverse conversion；当前 contracts 只保留 domain -> DTO projection，DB string parsing 属于 infrastructure persistence adapter。
