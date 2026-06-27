# Research: implementation-scope

- Query: 为 CB04-A MCP preset incoming conversion 迁移确认代码级落点、调用点、owner、首波写入文件和 focused validation。
- Scope: internal
- Date: 2026-06-21

## Findings

## Source Material

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/prd.md` | 要求 contracts 保留 MCP preset DTO/wire/outbound projection，incoming DTO -> domain/application command 转换迁到 API adapter 或 application command builder。 |
| `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/design.md` | 指定 API adapter/application command builder 持有 transport/runtime binding/route policy parsing，可用 route-local mapper。 |
| `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/implement.md` | 指定先定位 reverse conversions 与 route/application callers，再保留 outbound DTO projection。 |
| `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/implement.jsonl` | 已登记 owner-map、CB03 research、frontend-backend contracts spec。 |
| `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/check.jsonl` | 已登记 owner-map 与 frontend-backend contracts spec。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md` | Owner rule：API adapter owns route request parsing into application command/read inputs；contract DTO owns wire shape and outbound projection only。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md` | 明确 `contracts/integration/mcp_preset.rs` outbound projection keep，DTO -> transport/runtime binding/route policy conversion split 到 CB04-A。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | `agentdash-contracts` 是 HTTP DTO/TS generation owner；route 需要 domain/application model 时 API layer owns mapping。 |
| `.trellis/spec/backend/architecture.md` | API 层负责请求/响应 DTO 和错误映射，业务编排进入 application 层。 |
| `.trellis/spec/backend/directory-structure.md` | Interface -> Application -> Domain 分层与依赖方向。 |
| `.trellis/spec/backend/error-handling.md` | API/application 错误边界使用结构化错误，不靠字符串解析语义。 |

## Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-contracts/src/integration/mcp_preset.rs` | MCP preset request/response DTO、TS derive、outbound projection，以及当前所有 contract-owned reverse conversions。 |
| `crates/agentdash-api/src/routes/mcp_presets.rs` | Project MCP preset CRUD/probe HTTP adapter；create/update 当前直接调用 contract reverse `Into::into`。 |
| `crates/agentdash-application/src/mcp_preset/service.rs` | Application command input structs and CRUD service；`UpdateMcpPresetInput.runtime_binding` is tri-state patch owner at application boundary. |
| `crates/agentdash-application/src/mcp_preset/probe.rs` | Probe use case and required/optional runtime binding behavior without runtime context。 |
| `crates/agentdash-application/src/runtime_gateway/setup_actions.rs` | `mcp.probe_transport` setup action；currently parses JSON directly into domain transport/runtime binding input。 |
| `crates/agentdash-application/src/mcp_preset/runtime.rs` | Runtime binding resolution and route policy use at launch/runtime server construction。 |
| `crates/agentdash-domain/src/mcp_preset/value_objects.rs` | Domain transport, runtime binding source/target/rule/config and route policy value objects。 |
| `crates/agentdash-domain/src/mcp_preset/entity.rs` | `McpPreset` aggregate and `with_runtime_binding` persistence-facing domain field。 |
| `crates/agentdash-contracts/src/generate_ts.rs` | MCP preset DTO generation exports; should remain contract-owned。 |
| `crates/agentdash-contracts/src/integration/shared_library.rs` | Reuses `McpRoutePolicy` in shared-library MCP server template payload; no reverse domain conversion found here。 |

## Current Reverse Conversions

All current contract-owned reverse conversions live in `crates/agentdash-contracts/src/integration/mcp_preset.rs`.

| Impl / Function | Type Direction | Current Calls | Target Owner |
| --- | --- | --- | --- |
| `impl From<McpHttpHeader> for domain::McpHttpHeader` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:23` | DTO header -> domain header | Nested by `McpTransportConfigDto -> domain::McpTransportConfig` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:112` and `:116`; reached by API create/update route conversions at `crates/agentdash-api/src/routes/mcp_presets.rs:120` and `:170`。 | API adapter mapper, because HTTP/SSE headers are request transport payload parsing. |
| `impl From<McpEnvVar> for domain::McpEnvVar` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:47` | DTO env var -> domain env var | Nested by stdio transport conversion at `crates/agentdash-contracts/src/integration/mcp_preset.rs:126`; reached by API create/update conversions at `crates/agentdash-api/src/routes/mcp_presets.rs:120` and `:170`。 | API adapter mapper, because stdio env belongs to incoming transport command parsing. |
| `impl From<McpTransportConfigDto> for domain::McpTransportConfig` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:107` | DTO transport -> domain transport | Direct: create route `req.transport.into()` at `crates/agentdash-api/src/routes/mcp_presets.rs:120`; update route `req.transport.map(Into::into)` at `crates/agentdash-api/src/routes/mcp_presets.rs:170`。 Probe route currently avoids this impl by serializing `ProbeMcpPresetRequest` to JSON at `crates/agentdash-api/src/routes/mcp_presets.rs:278` and letting application setup action parse domain transport from JSON at `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:63`-`:72`, `:131`-`:140`。 | API adapter for create/update/probe request DTO mapping; optionally delegate pure field mapping to an adapter-owned mapper module. |
| `impl From<McpRuntimeBindingConfigDto> for domain::McpRuntimeBindingConfig` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:151` | DTO binding config -> domain binding config | Direct: create route `req.runtime_binding.map(Into::into)` at `crates/agentdash-api/src/routes/mcp_presets.rs:122`; update route nested tri-state map at `crates/agentdash-api/src/routes/mcp_presets.rs:172`-`:174`。 Probe route currently serializes DTO and application parses `runtime_binding` into domain at `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:63`-`:72`。 | API adapter or application command builder; preserve update tri-state semantics at command boundary. |
| `impl From<McpRuntimeBindingRuleDto> for domain::McpRuntimeBindingRule` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:178` | DTO binding rule -> domain binding rule | Nested by binding config reverse conversion at `crates/agentdash-contracts/src/integration/mcp_preset.rs:155`; reached by create/update route conversions and by probe JSON parse through setup action。 | Same mapper owner as binding config; rule `required` is command/runtime semantic and must not remain contract-owned. |
| `impl From<McpRuntimeBindingSourceDto> for domain::McpRuntimeBindingSource` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:216` | DTO binding source -> domain source enum | Nested by binding rule reverse conversion at `crates/agentdash-contracts/src/integration/mcp_preset.rs:181`。 Probe required diagnostic later reads sources in `crates/agentdash-application/src/mcp_preset/probe.rs:81`-`:114`。 | Application command builder or API mapper; source path semantics feed runtime context requirement checks. |
| `impl From<McpRuntimeBindingTargetDto> for domain::McpRuntimeBindingTarget` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:253` | DTO binding target -> domain target enum | Nested by binding rule reverse conversion at `crates/agentdash-contracts/src/integration/mcp_preset.rs:182`。 Runtime target/transport compatibility is enforced in `crates/agentdash-application/src/mcp_preset/runtime.rs:196`-`:239`。 | Application command builder or API mapper; target semantics affect runtime binding and transport compatibility. |
| `impl From<McpRoutePolicy> for domain::McpRoutePolicy` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:283` | DTO route policy -> domain route policy | Direct: create route `req.route_policy.into()` at `crates/agentdash-api/src/routes/mcp_presets.rs:121`; update route `req.route_policy.map(Into::into)` at `crates/agentdash-api/src/routes/mcp_presets.rs:171`。 Route policy behavior is consumed by `McpRoutePolicy::uses_relay` in `crates/agentdash-domain/src/mcp_preset/value_objects.rs:108`-`:116` and runtime server construction in `crates/agentdash-application/src/mcp_preset/runtime.rs:78`-`:86`。 | API adapter or application command builder; route policy is command/runtime behavior, not wire-only DTO behavior. |

Outbound conversions that should stay contract-owned:

| Impl | Reason |
| --- | --- |
| `domain::McpHttpHeader -> McpHttpHeader` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:14` | Nested response projection. |
| `domain::McpEnvVar -> McpEnvVar` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:38` | Nested response projection. |
| `domain::McpTransportConfig -> McpTransportConfigDto` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:81` | Response/read projection into wire DTO. |
| `domain::McpRuntimeBindingConfig/Rule/Source/Target -> DTO` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:142`, `:168`, `:199`, `:242` | Response projection preserving persisted binding declaration. |
| `domain::McpRoutePolicy -> McpRoutePolicy` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:273` | Response projection of persisted policy. |
| `domain::McpPresetSource -> McpPresetSourceTag` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:300` and `domain::McpPreset -> McpPresetResponse` at `:355` | Browser-facing response projection. |

## Call Sites And Migration Targets

### Create Preset

Current path:

- HTTP route takes `CreateMcpPresetRequest` at `crates/agentdash-api/src/routes/mcp_presets.rs:102`。
- Route builds `CreateMcpPresetInput` at `crates/agentdash-api/src/routes/mcp_presets.rs:115`-`:123` using contract-owned reverse conversions for `transport`, `route_policy`, and `runtime_binding`。
- Application service validates key/display/transport and persists domain aggregate at `crates/agentdash-application/src/mcp_preset/service.rs:70`-`:91`。

Migration target:

- Add adapter-owned mapping from `CreateMcpPresetRequest` to `CreateMcpPresetInput` in `agentdash-api` route-local mapper or application command builder.
- If placed in application, avoid depending on `agentdash-contracts`; prefer an application-owned command builder input or keep mapping in API to avoid deepening the existing transitional application -> contracts dependency.

### Update Preset

Current path:

- HTTP route takes `UpdateMcpPresetRequest` at `crates/agentdash-api/src/routes/mcp_presets.rs:152`。
- Route builds `UpdateMcpPresetInput` at `crates/agentdash-api/src/routes/mcp_presets.rs:166`-`:175`。
- `description` and `runtime_binding` are both tri-state fields in contracts at `crates/agentdash-contracts/src/integration/mcp_preset.rs:408`-`:419`。
- Application update applies patch semantics: `None` unchanged, `Some(value)` replace, `Some(None)` clear for `runtime_binding` at `crates/agentdash-application/src/mcp_preset/service.rs:123`-`:132`。

Migration target:

- API adapter/application command builder must preserve tri-state `runtime_binding: Option<Option<McpRuntimeBindingConfig>>` when mapping request DTO to `UpdateMcpPresetInput`。
- Add tests at the adapter/builder boundary for omitted binding unchanged, `null` clears, object replaces. Contract-level serde test may stay for DTO shape only.

### Probe Transport

Current path:

- HTTP route takes `ProbeMcpPresetRequest` at `crates/agentdash-api/src/routes/mcp_presets.rs:267`。
- Route serializes the whole request DTO to JSON at `crates/agentdash-api/src/routes/mcp_presets.rs:278`-`:279`。
- Runtime gateway setup action parses that JSON into application-local untagged `McpProbeTransportInput` using domain `McpTransportConfig` and `McpRuntimeBindingConfig` at `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:63`-`:72`, then calls `probe_transport_without_runtime_context` at `:142`-`:148`。
- Required runtime binding is rejected as `Unsupported` before static probe at `crates/agentdash-application/src/mcp_preset/probe.rs:67`-`:79`; source diagnostics are built at `:81`-`:114`。

Migration target:

- Do not leave route as DTO JSON pass-through. API adapter should parse `ProbeMcpPresetRequest` into an application command/input before invoking runtime gateway or a dedicated application probe command.
- Preferred first-wave minimum: add API-owned conversion helpers for `ProbeMcpPresetRequest -> (McpTransportConfig, Option<McpRuntimeBindingConfig>)`, serialize only an application-owned setup action input if runtime gateway remains the execution path.
- `McpProbeTransportInput` in `setup_actions.rs` should become an application/runtime-gateway command type rather than a compatibility untagged parser for both raw transport and request-shaped JSON, unless another non-HTTP caller still requires raw transport. No compatibility path is required for this pre-release project.

## Code Patterns

- Contract DTOs derive serde and TS in `crates/agentdash-contracts/src/integration/mcp_preset.rs:56`-`:79`, `:133`-`:140`, `:160`-`:166`, `:188`-`:197`, `:233`-`:240`, `:264`-`:271`, `:385`-`:445`。
- Generated TS exports are centralized in `crates/agentdash-contracts/src/generate_ts.rs:468`-`:483`; these exports should remain intact after removing reverse impls.
- API route already owns permission/path parsing and application input assembly in `crates/agentdash-api/src/routes/mcp_presets.rs:98`-`:126`, `:148`-`:179`, `:263`-`:299`; this is the natural owner for DTO -> command mapping.
- Application service inputs already use domain types and encode command shape in `crates/agentdash-application/src/mcp_preset/service.rs:15`-`:36`。
- Runtime required/optional binding behavior is application-owned in `crates/agentdash-application/src/mcp_preset/probe.rs:67`-`:79`; runtime binding application to launch transport is application-owned in `crates/agentdash-application/src/mcp_preset/runtime.rs:70`-`:83` and `:89`-`:136`。
- Domain route policy behavior is in `crates/agentdash-domain/src/mcp_preset/value_objects.rs:95`-`:116`; mapping into that value belongs to route/application command boundary.

## Suggested First-Wave Write Set

Recommended files to touch together:

| File | Why |
| --- | --- |
| `crates/agentdash-api/src/routes/mcp_presets.rs` | Replace inline `Into::into` call sites with adapter-owned mapper functions for create/update/probe; add focused route/mapper tests. |
| `crates/agentdash-contracts/src/integration/mcp_preset.rs` | Remove DTO -> domain reverse impls only; keep DTO definitions, serde tests, TS derives and outbound `domain -> DTO` projections. |
| `crates/agentdash-application/src/runtime_gateway/setup_actions.rs` | If probe still goes through runtime gateway, replace request-shaped/domain serde parser with explicit application/runtime input shape and adjust tests. |
| `crates/agentdash-application/src/mcp_preset/mod.rs` or a new `crates/agentdash-application/src/mcp_preset/command.rs` | Only if choosing application command builder ownership for probe/create/update mapping helpers without depending on contracts. |

Files that may need test-only or import cleanup:

| File | Why |
| --- | --- |
| `crates/agentdash-application/src/mcp_preset/service.rs` | Add/adjust focused tests for runtime binding update tri-state if not covered at API mapper level. |
| `crates/agentdash-application/Cargo.toml` | Do not remove `agentdash-contracts` dependency in this task; CB03 says it is transitional and shared by other active migrations. |

Files not to parallel-touch with this first wave:

| File / Area | Reason |
| --- | --- |
| `crates/agentdash-contracts/src/generate_ts.rs` | Keep generated export surface stable; only touch if removing reverse impls somehow exposes compile drift, which is unlikely. |
| `packages/app-web/src/generated/mcp-preset-contracts.ts` | Generated output; let `pnpm run contracts:check` detect drift rather than manual edits. |
| Frontend MCP preset helpers under `packages/app-web/src/features/mcp-shared/` and panels | CB04-A is backend incoming conversion owner migration; frontend request shape should not change. |
| `crates/agentdash-contracts/src/integration/shared_library.rs` | Only reuses `McpRoutePolicy` as wire DTO for shared-library template payload; no reverse conversion found. |
| AgentRun workspace snapshot/session context/capability/routine/LLM/settings/backend access files | Explicitly excluded by this task design as other CB04 work items. |
| Migrations under `crates/agentdash-infrastructure/migrations/` | No schema change expected; runtime_binding already exists in migration `0012_mcp_preset_runtime_binding.sql`。 |

## Focused Validation Commands

```powershell
cargo test -p agentdash-contracts mcp_preset --lib
cargo test -p agentdash-api mcp_preset --lib
cargo test -p agentdash-application mcp_preset --lib
cargo test -p agentdash-application mcp_probe_provider --lib
pnpm run contracts:check
```

Optional narrower probes while iterating:

```powershell
cargo test -p agentdash-application required_runtime_binding_without_runtime_context_returns_unsupported --lib
cargo test -p agentdash-application optional_runtime_binding_without_runtime_context_keeps_static_probe --lib
cargo test -p agentdash-application mcp_probe_provider_returns_unsupported_for_required_runtime_binding --lib
```

## Risks

### Patch Semantics

- `UpdateMcpPresetRequest.runtime_binding` is `Option<Option<McpRuntimeBindingConfigDto>>` at `crates/agentdash-contracts/src/integration/mcp_preset.rs:417`-`:419`。
- Application update applies the tri-state at `crates/agentdash-application/src/mcp_preset/service.rs:130`-`:132`。
- Migration must preserve omitted = unchanged, `null` = clear, object = replace. Mapper tests should target this directly, because replacing `map(|runtime_binding| runtime_binding.map(...))` with a helper is an easy place to collapse `None` and `Some(None)` accidentally.

### Runtime Binding Required / Optional

- Ordinary probe has no runtime context, so required runtime binding must return `Unsupported`, not `Ok`/`Error`, at `crates/agentdash-application/src/mcp_preset/probe.rs:67`-`:79`。
- Optional runtime binding continues static probe at `crates/agentdash-application/src/mcp_preset/probe.rs:74`-`:78`。
- Runtime source/target enum mapping must stay exactly aligned with application diagnostics and runtime binding resolution in `crates/agentdash-application/src/mcp_preset/probe.rs:101`-`:114` and `crates/agentdash-application/src/mcp_preset/runtime.rs:196`-`:239`。

### Route Policy

- `McpRoutePolicy` wire enum defaults to `Auto` in contracts at `crates/agentdash-contracts/src/integration/mcp_preset.rs:264`-`:271`。
- Domain route policy owns behavior: auto uses relay for stdio and direct for HTTP/SSE at `crates/agentdash-domain/src/mcp_preset/value_objects.rs:108`-`:116`。
- Migration must preserve create defaulting and update patch semantics: create omitted policy remains Auto via DTO serde default; update omitted policy remains unchanged.

### Probe Route Policy / Runtime Gateway Shape

- Current probe route serializes the whole contract request DTO into runtime gateway input at `crates/agentdash-api/src/routes/mcp_presets.rs:278`-`:294`。
- Current setup action accepts both request-shaped input and bare domain transport via untagged serde at `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:63`-`:72`。
- If the task removes compatibility paths as requested, confirm no non-route caller depends on bare transport input. The only found tests for bare transport are in `setup_actions.rs:529`-`:565` and can be updated to the new command shape.

## External References

- None. This was an internal codebase/spec research pass; no network references were needed.

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/directory-structure.md`
- `.trellis/spec/backend/domain-payload-typing.md`
- `.trellis/spec/backend/error-handling.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell; this research uses the user-provided task path `.trellis/tasks/06-21-cb04-mcp-preset-incoming-conversion/` as explicit output location.
- No business code was modified and no validation commands were run; this file is an implementation-scope research artifact only.
- No reverse conversion was found in `shared_library.rs`; it imports `McpRoutePolicy` only as a DTO field for `McpServerTemplatePayloadDto`。
- No database migration appears necessary for this task; existing runtime binding persistence is already represented by `0012_mcp_preset_runtime_binding.sql`。
