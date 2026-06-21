# Research: implementation-scope

- Query: 为 CB04-E Routine / LLM Provider / Settings reverse conversion cleanup 确认代码级落点、调用点、迁移 owner、分批顺序、写入文件集合、冲突边界和 focused validation commands。
- Scope: internal
- Date: 2026-06-21

## Findings

## Source Material

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/prd.md` | 要求 contracts 保留 DTO definitions/outbound projection，将 Routine dispatch strategy、LLM provider credentials/protocol values、Settings scope reverse parsing 迁移到 route/application command mapper。 |
| `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/design.md` | 明确 wire DTO 属于 contracts，DTO -> domain value parsing 属于 route/application command mapper；不得触碰 MCP preset/backend access conversion。 |
| `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/implement.md` | 初始步骤与验证命令。 |
| `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/implement.jsonl` | 指向 parent owner-map、CB03 owner map、cross-layer contracts spec。 |
| `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/check.jsonl` | 指向 parent owner-map、cross-layer contracts spec。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/owner-map.md` | Owner rule：API adapter owns route request parsing into application command/read inputs；contract DTO owns wire shape and outbound projection。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/cb03-owner-map.md` | CB04-E 候选：Routine / LLM Provider / Settings reverse conversion 迁移到对应 API route/application command mapper。 |

## Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-contracts/src/runtime/routine.rs` | Routine request/response DTO 与 `RoutineDispatchStrategyDto` outbound/reverse conversion。 |
| `crates/agentdash-api/src/routes/routines.rs` | Routine HTTP route，create/update 当前直接通过 `.into()` 将 dispatch strategy DTO 转为 domain `DispatchStrategy`。 |
| `crates/agentdash-contracts/src/integration/llm_provider.rs` | LLM Provider generated DTO；包含 protocol/credential mode outbound 与 reverse conversion。 |
| `crates/agentdash-api/src/routes/llm_providers.rs` | LLM Provider route，create/update/admin probe 当前消费 DTO -> domain conversion；response projection 仍在 route helper。 |
| `crates/agentdash-application/src/llm_provider.rs` | LLM Provider application input 已使用 domain `WireProtocol` / `LlmCredentialMode`，因此 DTO parsing 可留在 API adapter。 |
| `crates/agentdash-contracts/src/system/settings.rs` | Settings DTO；包含 `SettingsScopeKind` outbound/reverse conversion。 |
| `crates/agentdash-api/src/routes/settings.rs` | Settings route，query scope 当前通过 `.map(DomainSettingScopeKind::from)` 消费 contract reverse conversion。 |
| `crates/agentdash-contracts/src/generate_ts.rs` | 生成入口导出 `RoutineDispatchStrategyDto`、`LlmProviderProtocol`、`LlmCredentialModeDto`、`SettingsScopeKind`；迁移不得改变 wire DTO/export。 |

## Reverse Conversions By Subdomain

### Routine

Reverse conversion:

- `impl From<RoutineDispatchStrategyDto> for DispatchStrategy` in `crates/agentdash-contracts/src/runtime/routine.rs:230`.

Current callers:

- `create_routine` maps `req.dispatch_strategy` with `strategy.into()` at `crates/agentdash-api/src/routes/routines.rs:115`.
- `update_routine` maps `req.dispatch_strategy` with `dispatch_strategy.into()` at `crates/agentdash-api/src/routes/routines.rs:180`.

Owner migration:

- Move DTO -> domain mapping into `crates/agentdash-api/src/routes/routines.rs`, likely as a route-local helper such as `routine_dispatch_strategy_into_domain(strategy: RoutineDispatchStrategyDto) -> DispatchStrategy`.
- Keep outbound `impl From<DispatchStrategy> for RoutineDispatchStrategyDto` at `crates/agentdash-contracts/src/runtime/routine.rs:220`, because `RoutineResponse::from` still projects domain response state to DTO at `crates/agentdash-contracts/src/runtime/routine.rs:39`.

Code pattern:

- Routine route already owns request parsing for `RoutineTriggerConfigRequest` via helpers at `crates/agentdash-api/src/routes/routines.rs:334` and `crates/agentdash-api/src/routes/routines.rs:358`, including command-specific validation. Dispatch strategy parsing should follow this local-helper pattern.

### LLM Provider

Reverse conversions:

- `impl From<LlmProviderProtocol> for domain::WireProtocol` in `crates/agentdash-contracts/src/integration/llm_provider.rs:27`.
- `impl From<LlmCredentialModeDto> for domain::LlmCredentialMode` in `crates/agentdash-contracts/src/integration/llm_provider.rs:56`.

Current callers:

- `create_provider` maps `req.protocol.into()` and `req.credential_mode.map(Into::into)` into `CreateLlmProviderInput` at `crates/agentdash-api/src/routes/llm_providers.rs:155`.
- `update_provider` maps `req.protocol.map(Into::into)` and `req.credential_mode.map(Into::into)` into `UpdateLlmProviderInput` at `crates/agentdash-api/src/routes/llm_providers.rs:199`.
- `probe_models` maps `req.protocol.into()` before executor probe at `crates/agentdash-api/src/routes/llm_providers.rs:473`.

Owner migration:

- Move DTO -> domain mapping into `crates/agentdash-api/src/routes/llm_providers.rs`, likely as route-local `llm_provider_protocol_into_domain` and `llm_credential_mode_into_domain`.
- Keep application input structs typed with domain values in `crates/agentdash-application/src/llm_provider.rs:10` and `crates/agentdash-application/src/llm_provider.rs:27`; the API adapter should pass already-parsed domain `WireProtocol` / `LlmCredentialMode`.
- Keep outbound `impl From<domain::WireProtocol> for LlmProviderProtocol` at `crates/agentdash-contracts/src/integration/llm_provider.rs:16`, `impl From<domain::LlmCredentialMode> for LlmCredentialModeDto` at `crates/agentdash-contracts/src/integration/llm_provider.rs:46`, and other outbound-only conversions for credential source / verification status at `:75` and `:94`, because route response helpers project domain state to generated DTO at `crates/agentdash-api/src/routes/llm_providers.rs:824` and `crates/agentdash-api/src/routes/llm_providers.rs:928`.

Code pattern:

- API route already owns provider command assembly by constructing `CreateLlmProviderInput` / `UpdateLlmProviderInput` at `crates/agentdash-api/src/routes/llm_providers.rs:155` and `:199`. Reverse DTO parsing should sit at that construction boundary.
- `credential source` and `verification status` are not audited reverse conversions here; they are response projection only in current code.

### Settings

Reverse conversion:

- `impl From<SettingsScopeKind> for agentdash_domain::settings::SettingScopeKind` in `crates/agentdash-contracts/src/system/settings.rs:23`.

Current callers:

- `resolve_scope` maps `query.scope.map(DomainSettingScopeKind::from)` at `crates/agentdash-api/src/routes/settings.rs:163`.

Owner migration:

- Move DTO -> domain mapping into `crates/agentdash-api/src/routes/settings.rs`, likely as `settings_scope_kind_into_domain(scope: SettingsScopeKind) -> DomainSettingScopeKind`.
- Keep outbound `impl From<agentdash_domain::settings::SettingScopeKind> for SettingsScopeKind` at `crates/agentdash-contracts/src/system/settings.rs:13`, because list/update responses project domain setting scope into DTO at `crates/agentdash-api/src/routes/settings.rs:68` and `crates/agentdash-api/src/routes/settings.rs:114`.

Code pattern:

- `resolve_scope` already owns command/read access semantics: system access gate, user scope identity binding, project id validation, and project permission check at `crates/agentdash-api/src/routes/settings.rs:157`. Scope reverse parsing belongs in this helper, not contracts.

## Suggested Batching

Can be changed together in one small implementation batch:

- Routine dispatch strategy reverse conversion.
- Settings scope reverse conversion.

Reason:

- Both are single enum mappings with direct API route callers.
- Both target only one contracts file plus one route file.
- Both have obvious existing route helper locations and low blast radius.

Should be a separate batch within the same task, or at least a separate commit/hunk reviewed independently:

- LLM Provider protocol and credential mode reverse conversion.

Reason:

- It touches larger route/application command assembly around provider create/update/probe and intersects credential/OAuth/provider response helpers in the same file.
- It has multiple call sites and more imported enum names, while outbound response projection must remain unchanged.

Do not include in this task:

- MCP preset DTO -> domain conversions in `crates/agentdash-contracts/src/integration/mcp_preset.rs`.
- Backend access status/mode conversions in `crates/agentdash-contracts/src/backend/contract.rs` and `crates/agentdash-api/src/routes/backend_access.rs`.
- Session message ref or context usage helper migrations.

## Suggested Write Set And Conflict Boundaries

Primary write set:

| File | Expected edits | Conflict boundary |
| --- | --- | --- |
| `crates/agentdash-contracts/src/runtime/routine.rs` | Remove only `impl From<RoutineDispatchStrategyDto> for DispatchStrategy`; possibly drop `DispatchStrategy` import only if outbound impl can import/qualify it cleanly. | Avoid changing request/response DTO definitions, serde tags, generated TS exports, trigger config mapping, execution response projection. |
| `crates/agentdash-api/src/routes/routines.rs` | Add route-local dispatch strategy mapper and replace two `.into()` call sites. | Avoid changing trigger config semantics, webhook token generation, cron scheduler notification, repository calls. |
| `crates/agentdash-contracts/src/integration/llm_provider.rs` | Remove only reverse `impl From<LlmProviderProtocol> for domain::WireProtocol` and reverse `impl From<LlmCredentialModeDto> for domain::LlmCredentialMode`; preserve outbound projection impls and DTO definitions. | Avoid changing `LlmCredentialSourceDto`, `LlmCredentialVerificationStatusDto`, OAuth DTOs, probe request shape, generated export list. |
| `crates/agentdash-api/src/routes/llm_providers.rs` | Add route-local protocol/credential mode mappers and replace create/update/probe reverse `.into()` call sites. | Avoid changing secret masking/encryption, provider credential resolver, OAuth exchange, effective provider response projection. |
| `crates/agentdash-contracts/src/system/settings.rs` | Remove only reverse `impl From<SettingsScopeKind> for domain SettingScopeKind`; keep outbound scope projection. | Avoid changing `SettingsScopeQuery`, `SettingResponse`, `UpdateSettingsRequest/Response` wire shapes. |
| `crates/agentdash-api/src/routes/settings.rs` | Add route-local scope mapper and replace `DomainSettingScopeKind::from` in `resolve_scope`. | Avoid changing system/user/project authorization behavior or masking behavior. |

Optional test write set:

| File | Expected edits | Conflict boundary |
| --- | --- | --- |
| `crates/agentdash-api/src/routes/routines.rs` | Add unit tests for route-local dispatch mapper, especially `PerEntity { entity_key_path }`. | Keep tests focused on mapper semantics; avoid full repository/AppState setup. |
| `crates/agentdash-api/src/routes/llm_providers.rs` | Add unit tests for protocol and credential mode mappers. | Existing tests at `crates/agentdash-api/src/routes/llm_providers.rs:1031` cover OAuth helpers; append mapper tests without needing network/probe. |
| `crates/agentdash-api/src/routes/settings.rs` | Add unit tests for scope mapper or `resolve_scope` default/path if feasible. | Existing tests at `crates/agentdash-api/src/routes/settings.rs:206` cover system access only; avoid requiring repository setup unless already available. |

Parallel implementation notes:

- Routine and Settings can be implemented by separate agents without overlapping files.
- LLM Provider must be single-owner because `crates/agentdash-api/src/routes/llm_providers.rs` is large and mixes create/update/probe/OAuth/response helpers.
- Contracts files are independent across the three domains, but `crates/agentdash-contracts/src/generate_ts.rs` should not need edits unless compile reveals imports tied to removed impls; generated TypeScript output should remain unchanged.

## Focused Validation Commands

Recommended fast checks after implementation:

```powershell
cargo test -p agentdash-api routes::routines --lib
cargo test -p agentdash-api routes::llm_providers --lib
cargo test -p agentdash-api routes::settings --lib
pnpm run contracts:check
```

Fallback broader checks if Rust test filters do not match module paths:

```powershell
cargo test -p agentdash-api routine --lib
cargo test -p agentdash-api llm_provider --lib
cargo test -p agentdash-api settings --lib
cargo check -p agentdash-contracts
cargo check -p agentdash-api
```

Generated-contract invariant:

```powershell
pnpm run contracts:check
```

The generated TypeScript should not drift because DTO definitions and `generate_ts.rs` exports remain unchanged.

## Related Specs

- `.trellis/workflow.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/backend/index.md`
- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/domain-payload-typing.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/cross-layer/shared-library-contract.md`
- `.trellis/spec/backend/capability/llm-model-config.md`

## External References

- None. This research is codebase-internal and does not depend on third-party API behavior.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)` / `Source: none`; this research used the user-provided explicit task directory `.trellis/tasks/06-21-cb04-routine-llm-settings-reverse-conversion/`.
- No business code was modified in this research turn.
- No validation command was executed; commands above are implementation/check guidance.
- Existing tests do not appear to directly lock these reverse conversion helpers. Focused mapper tests should be added in API route modules if implementation removes the contracts reverse impls.
