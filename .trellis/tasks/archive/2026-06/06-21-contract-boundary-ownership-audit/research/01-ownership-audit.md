# Research: Contract Boundary CB01/CB02 Ownership Audit

- Query: 完成 Contract Boundary 的 CB01 application -> contracts import audit 与 CB02 contracts crate conversion audit，标注 owner 并提出后续可拆实现任务。
- Scope: internal
- Date: 2026-06-21

## Findings

## Scope And Commands

`python ./.trellis/scripts/task.py current --source` 在当前 shell 返回 `Current task: (none)` / `Source: none`；本文件按用户显式指定路径 `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/01-ownership-audit.md` 写入。

本轮使用的 `rg` 命令：

```powershell
rg -n "agentdash_contracts|agentdash-contracts" crates/agentdash-application
rg -n "agentdash_(domain|spi|agent_protocol|agent_types)|impl (From|TryFrom)<|impl .*From<|\.into\(\)|try_into\(\)" crates/agentdash-contracts/src
rg -n "pub (struct|enum)|impl (From|TryFrom)<|impl .*From<" crates/agentdash-contracts/src
rg --files crates/agentdash-application crates/agentdash-contracts/src
rg -n "impl From<[^>]*(agentdash_domain|domain::|User|Group|spi_auth::|PersistedSessionEvent|ProjectionSourceRange|SessionLineage|MessageRef|AgentContextEnvelope|Routine|RoutineExecution|DispatchStrategy|RoutineExecutionStatus|AgentRuntimeRefs|OrchestrationBindingRefs)" crates/agentdash-contracts/src
rg -n "impl From<[^>]*(Dto|Request|Response|Mcp|Llm|SettingsScopeKind|SessionMessageRefDto|RoutineDispatchStrategyDto).* for (agentdash_domain|domain::|DispatchStrategy|MessageRef)" crates/agentdash-contracts/src
rg -n "use agentdash_(domain|spi|agent_protocol|agent_types)|use .* as domain|use .* as spi_auth|codex_app_server_protocol as codex" crates/agentdash-contracts/src
rg -n "from_domain\(|to_domain|try_from\(|TryFrom<|fn .*_to_dto|fn .*_from_" crates/agentdash-contracts/src
rg -n "ConversationEffectiveExecutorConfigView|ConversationModelConfigSource" crates/agentdash-application/src/agent_run/project_agent_start.rs
rg -n "AgentConversationSnapshot|ConversationModelConfig|ConversationCommand|ConversationDiagnostic|ValidationSeverity|ResolvedVfsSurface|LifecycleSubjectAssociationDto|SubjectRefDto|contract_vfs::" crates/agentdash-application/src/agent_run/conversation_snapshot.rs crates/agentdash-application/src/agent_run/workspace/query.rs crates/agentdash-application/src/agent_run/workspace/types.rs crates/agentdash-application/src/agent_run/workspace/command_policy.rs
rg -n "CapabilityCatalog|CapabilityScopeDto|ToolClusterDto|ToolDescriptorDto|ToolSourceDto|PlatformMcpScopeDto" crates/agentdash-application/src/capability/tool_catalog.rs
rg -n "WorkspaceModule(CanvasHostAction|Descriptor|Operation|OperationDispatch|Kind|Presentation|Status|Summary|UiEntry|StatusKind)|WorkspaceModulePresentation|WorkspaceModuleDescriptor" crates/agentdash-application/src/workspace_module/mod.rs crates/agentdash-application/src/workspace_module/tools.rs
rg -n "SessionContextUsageItemResponse|context_usage_items_from_context_frame|SessionEventResponse|SessionNdjsonEnvelope" crates/agentdash-application/src/session/eventing.rs crates/agentdash-contracts/src/runtime/session.rs
```

## Files Found

| Path | Description |
| --- | --- |
| `AGENTS.md` | 项目级协作约束；中文沟通、预研期不做兼容回退、小规模迭代避免过度测试。 |
| `.trellis/workflow.md` | Trellis 研究产物必须持久化到 task `research/`，Phase 1.2 research 规则。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/prd.md` | 本审计目标与验收：application -> contracts import、contracts conversion 边界、owner map。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/design.md` | 待评估边界规则：read model 与 wire DTO 分离、API adapter 默认映射、incoming command conversion 是否离开 contracts。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/index.md` | CB01/CB02/CB03/CB04 拆分。 |
| `.trellis/tasks/06-21-module-topology-coupling-review/research/10-contract-boundary-deep-dive.md` | 前序 deep dive，指出 application 直接构造 browser-facing DTO 与 contracts 依赖 domain/SPI/protocol。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | contract crate、generated TS、NDJSON envelope 与 route-local DTO 规则。 |
| `.trellis/spec/frontend/type-safety.md` | generated wire 单源、mapper 边界、view model 与 wire DTO 分层规则。 |
| `crates/agentdash-application/Cargo.toml` | application 直接依赖 `agentdash-contracts`。 |
| `crates/agentdash-contracts/Cargo.toml` | contracts 依赖 `agentdash-domain` / `agentdash-spi` / `agentdash-agent-protocol` / `agentdash-agent-types`。 |

## CB01: agentdash-application -> agentdash-contracts Imports

### Summary

`crates/agentdash-application` 的所有 `agentdash_contracts` import/callsite 集中在以下模块：

| Module | Import / callsite | Current usage | Suggested owner | Action |
| --- | --- | --- | --- | --- |
| crate manifest | `crates/agentdash-application/Cargo.toml:10` | application crate 直接依赖 contracts。 | migration candidate | 依赖本身应随迁移收敛；短期因 existing projection 仍存在可保留，后续以模块级迁移减少依赖面。 |
| AgentRun conversation snapshot | `conversation_snapshot.rs:3-15` imports `vfs::ResolvedVfsSurface` and many `workflow::*` DTOs。 | application 直接解析 model config、execution state、command set、diagnostics，并返回 `AgentConversationSnapshot`。 | migration candidate | 需要拆出 application read model，再由 API adapter 映射成 `AgentConversationSnapshot`。 |
| AgentRun conversation snapshot tests | `conversation_snapshot.rs:678-681` imports VFS contract test DTO。 | 仅测试当前 contract-shaped snapshot。 | migration candidate | 随 snapshot owner 迁移调整测试 fixture；不是独立业务 owner。 |
| ProjectAgent start | `project_agent_start.rs:3-5` imports `ConversationEffectiveExecutorConfigView` / `ConversationModelConfigSource`。 | start dispatch carry browser-facing executor config view；line `77` 是 dispatch 字段，line `267`/`677`/`762` 使用 source/view。 | application read model -> migration candidate | executor config 解析语义属于 application read model；wire view 应由 API adapter 或 contract mapper生成。 |
| AgentRun workspace types | `workspace/types.rs:7-9` imports `AgentConversationSnapshot` / `ConversationEffectiveExecutorConfigView` / `SubjectRefDto`。 | `AgentRunWorkspaceSnapshot.conversation` 直接是 contract DTO (`types.rs:34`)；list projection subject ref 直接是 `SubjectRefDto` (`types.rs:52`)。 | migration candidate | workspace snapshot 应保持 application read model，API adapter 输出 `AgentRunWorkspaceView` / list DTO。 |
| AgentRun workspace query | `workspace/query.rs:1-5` imports `contract_vfs` and workflow DTOs。 | query service 组装 `AgentConversationSnapshot` (`query.rs:203-219`)、`SubjectRefDto` (`query.rs:294-297`)、`LifecycleSubjectAssociationDto` (`query.rs:591-605`)、VFS contract conversion (`query.rs:608-715`)。 | API adapter / allowed projection conversion now, migration candidate later | VFS/read model -> wire mapping目前在 application；建议迁到 API adapter 或专门 projection mapper，保留 contracts DTO 为 wire owner。 |
| AgentRun workspace command policy | `workspace/command_policy.rs:1-4`, `:534`, `:538`, `:557`。 | policy uses generated command/status DTO as precondition and reconstructs policy snapshot (`command_policy.rs:140`, `:168-169`)；test imports stale guard。 | application read model / migration candidate | command policy should validate against application command precondition/read model；HTTP contract precondition belongs API adapter。 |
| Capability tool catalog | `capability/tool_catalog.rs:6-9` imports catalog DTOs。 | service returns `CapabilityCatalogResponse` (`tool_catalog.rs:50-56`) and maps SPI tool/capability enums to DTO (`tool_catalog.rs:137-183`)。 | API adapter / migration candidate | catalog facts are application projection; DTO construction should move to API adapter or contract projection mapper. |
| Workspace module tools | `workspace_module/tools.rs:13-16` imports workspace module DTOs。 | Agent tools expose module descriptors/operations in tool result details (`tools.rs:54`, `:179-207`) and dispatch on DTO operation union (`tools.rs:725-865`)。 | allowed projection conversion / contract DTO | This is not only browser HTTP; agent tool JSON also uses this contract. Keep DTO as shared contract, but keep domain aggregation logic outside contract crate. |
| Workspace module aggregate | `workspace_module/mod.rs:17-21`, `:508` test import。 | aggregate constructs `WorkspaceModuleDescriptor`, `WorkspaceModulePresentation`, operation/summary/status/ui entries (`mod.rs:183-190`, `:240-284`, `:287-467`) from extension runtime projection and Canvas domain. | allowed projection conversion now, migration candidate for API-facing parts | Application currently owns aggregation. Contract DTO remains wire owner; long term consider application read model + API/agent adapter. |
| Session eventing | `session/eventing.rs:8-10` imports session contract context usage helper and response item. | eventing returns `Vec<SessionContextUsageItemResponse>` (`eventing.rs:327-365`) by calling helper from contracts (`runtime/session.rs:806`). | migration candidate | Context usage projection logic should not live in contract crate if it inspects SPI `ContextFrame`; keep DTO in contracts, move projector to application/API adapter. |

### Code Patterns

- Application assembling full wire projection: `AgentConversationSnapshotResolver::resolve` returns contract DTO at `crates/agentdash-application/src/agent_run/conversation_snapshot.rs:270` and constructs `AgentConversationSnapshot` at `:295`.
- Application retaining its own read model while embedding contract DTO: `AgentRunWorkspaceSnapshot` has local shell/projection fields but `conversation: AgentConversationSnapshot` at `crates/agentdash-application/src/agent_run/workspace/types.rs:21-35`.
- Application-local to contract VFS mapper: `resolved_surface_to_contract` and nested enum/value mappers at `crates/agentdash-application/src/agent_run/workspace/query.rs:608-715`.
- Application command policy coupled to generated status/command DTO: `is_terminal_snapshot` compares against `agentdash_contracts::workflow::ConversationExecutionStatus::Terminal` at `crates/agentdash-application/src/agent_run/workspace/command_policy.rs:532-535`.
- SPI/platform facts mapped to contract DTO inside application service: capability catalog maps `ToolSource` / `ToolCluster` / `CapabilityScope` to DTOs at `crates/agentdash-application/src/capability/tool_catalog.rs:137-183`.
- Workspace module DTO construction is deliberately a projection aggregate: module docs say it aggregates extension runtime + visible canvas read model at `crates/agentdash-application/src/workspace_module/mod.rs:1-5`, then constructs contract descriptors at `:183-190`, `:287-467`.
- Session context usage helper is imported from contracts into application eventing at `crates/agentdash-application/src/session/eventing.rs:8-10`, and used at `:359-363`.

## CB02: agentdash-contracts domain/SPI/protocol Conversion Audit

### Crate Boundary

`agentdash-contracts` is described as shared wire DTO crate (`crates/agentdash-contracts/Cargo.toml:2-3`) but directly depends on internal model/protocol crates (`Cargo.toml:15-18`): `agentdash-agent-types`, `agentdash-domain`, `agentdash-agent-protocol`, and `agentdash-spi`.

### Outbound Projection Conversion

These conversions turn domain/SPI/protocol facts into browser/agent-facing DTOs. They are acceptable only when treated as projection conversion; the recommended owner is `allowed projection conversion` for narrow value-object/projection mapping, or `API adapter` / `migration candidate` when the conversion contains application-level read model decisions.

| Module | Conversion | Classification | Suggested owner | Action |
| --- | --- | --- | --- | --- |
| `context/contract.rs` | `MountCapability`, `ContextContainer*`, `ContextSource*`, `SessionRequiredContextBlock`, `SessionComposition` -> contract DTOs at lines `15`, `34`, `55`, `85`, `114`, `139`, `161`, `183`, `204`, `223`。 | outbound projection conversion | allowed projection conversion | Can remain temporarily as simple domain-to-wire projection; watch for application policy creeping into contracts. |
| `backend/contract.rs` | `BackendType`, `BackendVisibility`, `BackendShareScopeKind`, `RuntimeHealthStatus`, `BackendConfig`, `ProjectBackendAccess*`, `BackendWorkspaceInventory*` -> response DTOs at lines `13`, `30`, `48`, `69`, `143`, `184`, `210`, `283`, `311`, `335`, `375`。 | outbound projection conversion | API adapter / migration candidate | Responses are cross-feature browser DTOs; contract DTO is correct owner, but domain conversion should move to API adapter if it starts assembling inventory/access read models. |
| `project/contract.rs` | `ChangeKind` -> `ProjectStateChangeKind` (`:24`), `ProjectStateChange::from_domain` (`:53`), project config/visibility/role/subject grant conversions (`:115`, `:130`, `:149`, `:176`, `:193`, `:210`, `:278`)。 | outbound projection conversion | allowed projection conversion | Keep only projection/value mapping. Project stream envelope already belongs contract per spec. |
| `story/contract.rs` | `StoryContext`, `StoryStatus`, `StoryPriority`, `StoryType`, `Story` -> story DTOs at `:16`, `:47`, `:70`, `:92`, `:121`。 | outbound projection conversion | allowed projection conversion | Straight projection conversion; acceptable while DTO stays contract-owned. |
| `workspace/contract.rs` | Workspace identity/status/policy and `WorkspaceBinding` / `Workspace` -> response DTOs at `:16`, `:35`, `:53`, `:76`, `:102`, `:135`。 | outbound projection conversion | allowed projection conversion | Straight response projection; acceptable if no command validation enters contract. |
| `integration/skill_asset.rs` | `SkillAssetSource`, `SkillAssetFileKind`, `SkillAssetFile`, `SkillAsset` -> DTO at `:19`, `:59`, `:97`, `:139`。 | outbound projection conversion | allowed projection conversion | Response projection; create/update command conversion should stay outside contracts. |
| `integration/llm_provider.rs` | `WireProtocol`, `LlmCredentialMode`, `LlmCredentialSource`, `LlmCredentialVerificationStatus` -> DTO at `:16`, `:46`, `:75`, `:94`。 | outbound projection conversion | allowed projection conversion | Value projection is fine; incoming reverse mappings below are separate. |
| `integration/mcp_preset.rs` | domain MCP value/preset -> DTO at `:14`, `:38`, `:81`, `:142`, `:168`, `:199`, `:242`, `:273`, `:300`, `:318`, `:355`。 | outbound projection conversion | allowed projection conversion | Saved preset response projection can stay; reverse command conversion should move. |
| `runtime/routine.rs` | `Routine`, `RoutineExecution`, trigger config, dispatch strategy, execution status, runtime refs -> DTO at `:39`, `:74`, `:188`, `:220`, `:251`, `:270`, `:288`。 | outbound projection conversion | allowed projection conversion / migration candidate | Response projection is fine; `DispatchStrategy` reverse conversion below is incoming command conversion. |
| `runtime/session.rs` | `PersistedSessionEvent`, `ProjectionSourceRange`, lineage enums/records, `MessageRef`, `AgentContextEnvelope`, `ContextFrame` helper -> DTO at `:47`, `:153`, `:329`, `:348`, `:365`, `:434`, `:505`, `:806`。 | outbound projection conversion | allowed projection conversion for event/read DTO; migration candidate for `ContextFrame` projector | `SessionNdjsonEnvelope` is correct contract DTO owner. Context usage analysis over SPI `ContextFrame` is application projection logic and should move out. |
| `system/auth.rs` | SPI/domain auth identity/group/user -> auth/current-user/directory DTO at `:16`, `:33`, `:66`, `:168`, `:195`。 | outbound projection conversion | API adapter / allowed projection conversion | Auth wire DTO is contract-owned; mapping from SPI auth identity can stay narrow, but directory read model assembly belongs API/application. |
| `task/contract.rs` | `TaskPlanStatus`, `TaskPriority`, subject ref helper -> task DTOs at `:18`, `:43`, `:65`, `:229`。 | outbound projection conversion | allowed projection conversion | Run-scoped task plan/projection DTO belongs contracts; keep as simple projection. |
| `system/settings.rs` | `SettingScopeKind` -> DTO at `:13`。 | outbound projection conversion | allowed projection conversion | Narrow enum projection. |
| `agent/run_mailbox.rs` | Uses `codex::UserInput` directly in `AgentRunComposerSubmitRequest.input` at `:187-190`。 | protocol contract reuse, not conversion | contract DTO | Explicit spec decision: session turn control reuses canonical Codex input shape. |
| `agent/project_agent.rs` | Uses `codex::UserInput` in `CreateProjectAgentRunRequest.input` at `:60-64`。 | protocol contract reuse, not conversion | contract DTO | Explicit spec decision: ProjectAgent draft start shares canonical user input. |
| `runtime/session.rs` | `SessionEventResponse.notification: BackboneEnvelope` at `:43-44`; imports protocol/SPI/types at `:6-18`。 | protocol envelope projection | contract DTO / allowed projection conversion | Session NDJSON envelope is contract-owned; embedded protocol event is runtime protocol fact. |

### Incoming Command Conversion

These conversions turn wire DTOs into domain/SPI/internal values. They are the main migration candidates because contracts crate becomes command adapter/validation owner.

| Module | Conversion | Classification | Suggested owner | Action |
| --- | --- | --- | --- | --- |
| `backend/contract.rs` | `ProjectBackendAccessStatus` -> `agentdash_domain::backend::ProjectBackendAccessStatus` at `:194`; `ProjectBackendAccessMode` -> domain at `:218`。 | incoming command/value conversion | migration candidate | Move to API adapter / application command builder when create/update access requests are handled. |
| `integration/mcp_preset.rs` | `McpHttpHeader`, `McpEnvVar`, `McpTransportConfigDto`, `McpRuntimeBindingConfigDto`, `McpRuntimeBindingRuleDto`, `McpRuntimeBindingSourceDto`, `McpRuntimeBindingTargetDto`, `McpRoutePolicy` -> domain at `:23`, `:47`, `:107`, `:151`, `:178`, `:216`, `:253`, `:283`。 | incoming command conversion | migration candidate | Highest-priority CB02 migration; create/update/probe request -> domain command mapping should live in API adapter/application, not DTO crate. |
| `integration/llm_provider.rs` | `LlmProviderProtocol` -> `domain::WireProtocol` at `:27`; `LlmCredentialModeDto` -> `domain::LlmCredentialMode` at `:56`。 | incoming command/value conversion | migration candidate | Move to API adapter/application command mapper for provider create/update. |
| `runtime/routine.rs` | `RoutineDispatchStrategyDto` -> `DispatchStrategy` at `:230`。 | incoming command conversion | migration candidate | Move to routine API/application command mapper; keep DTO in contracts. |
| `runtime/session.rs` | `SessionMessageRefDto` -> `MessageRef` at `:374`。 | incoming command/value conversion | migration candidate, lower risk | Move near session fork/rollback command handling if it is request-facing; can remain temporarily if treated as trivial value parser. |
| `system/settings.rs` | `SettingsScopeKind` -> `agentdash_domain::settings::SettingScopeKind` at `:23`。 | incoming command/value conversion | migration candidate | Move to settings route/application command mapper. |

### Contract DTO Only / No Internal Conversion

- `surface/workspace_module.rs`, `surface/vfs.rs`, `surface/canvas.rs`, `extension/runtime.rs`, `extension/package.rs`, `extension/management.rs`, `extension/external_marketplace.rs`, `common_response/contract.rs`, `system/permission.rs`, `system/companion.rs`, and most `runtime/workflow.rs` are DTO definition surfaces in this search result, not explicit domain/SPI/protocol conversion sites.
- `generate_ts.rs` imports `BackboneEnvelope` for TypeScript export generation; this is generation plumbing, not a conversion owner.

## Owner Map

| Item | Current owner | Proposed owner | Label |
| --- | --- | --- | --- |
| `AgentConversationSnapshot` assembly | application | application read model + API adapter maps to contract DTO | migration candidate |
| AgentRun workspace shell/list/detail read model | application with embedded contract DTO | application read model | application read model |
| AgentRun workspace HTTP/browser wire DTO | application currently constructs | API adapter + contract DTO | API adapter / contract DTO |
| VFS `ResolvedVfsSurface` conversion in workspace query | application | API adapter or narrow projection mapper | migration candidate |
| Command policy precondition/status DTO | application consumes contract DTO | application command precondition model; API adapter parses request DTO | migration candidate |
| Capability catalog response | application service returns contract DTO | application read model + API adapter maps DTO | migration candidate |
| Workspace module descriptor/presentation | application aggregation returns contract DTO | allowed projection conversion shared by browser and agent tool output | allowed projection conversion |
| Session event NDJSON envelope | contracts | contracts | contract DTO |
| Session context usage projector over SPI `ContextFrame` | contracts helper called by application | application projection service; contracts keeps response DTO | migration candidate |
| Domain/SPI enum/value response mapping in contracts | contracts | contracts as allowed projection conversion, until policy appears | allowed projection conversion |
| DTO -> domain conversions in contracts | contracts | API adapter/application command builders | migration candidate |
| Direct protocol `codex::UserInput` fields | contracts | contracts | contract DTO |

## Follow-up Implementation Task Candidates

1. **Move incoming command conversion out of MCP preset contracts**
   - Scope: `crates/agentdash-contracts/src/integration/mcp_preset.rs` reverse `From<Dto> for domain::*`; API routes/application command builders own request -> domain mapping.
   - Acceptance direction: contract crate still defines request/response DTOs and domain -> response projection; create/update/probe route code explicitly maps generated request DTO to domain command/value.

2. **Introduce AgentRun workspace application read model and API adapter mapper**
   - Scope: `agent_run/conversation_snapshot.rs`, `agent_run/workspace/types.rs`, `agent_run/workspace/query.rs`, `agent_run/workspace/command_policy.rs`.
   - Acceptance direction: application returns read model structs without `agentdash_contracts`; API adapter maps read model to `AgentConversationSnapshot` / `AgentRunWorkspaceView` / list DTO.

3. **Move Session context usage projection helper from contracts to application**
   - Scope: `agentdash-contracts/src/runtime/session.rs::context_usage_items_from_context_frame` and `agentdash-application/src/session/eventing.rs`.
   - Acceptance direction: contracts keeps `SessionContextUsageItemResponse`; application owns SPI `ContextFrame` analysis and DTO mapping happens at API adapter or projection boundary.

4. **Capability catalog read model split**
   - Scope: `agentdash-application/src/capability/tool_catalog.rs`.
   - Acceptance direction: service returns application catalog model based on SPI facts; API adapter maps to `CapabilityCatalogResponse`, `ToolDescriptorDto`, `CapabilityScopeDto`.

5. **Routine / LLM / Settings reverse conversion cleanup**
   - Scope: `runtime/routine.rs::From<RoutineDispatchStrategyDto>`, `integration/llm_provider.rs` reverse conversions, `system/settings.rs` reverse conversion.
   - Acceptance direction: route/application command handlers own wire request -> domain conversion; contracts crate has no `From<*Dto> for domain::*` except explicitly approved protocol value parsers.

6. **Backend access/inventory conversion owner review**
   - Scope: `backend/contract.rs` outbound projections and reverse status/mode conversions.
   - Acceptance direction: keep response DTOs in contracts, move command/status parsing to API adapter, and decide whether access/inventory read model assembly lives in application.

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/prd.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/design.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/index.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/research/10-contract-boundary-deep-dive.md`

## Caveats / Not Found

- 未运行编译、测试、`pnpm run contracts:check` 或前端检查；本轮是只读静态审计。
- `rg` 没发现 `crates/agentdash-application` 除上述文件外的 `agentdash_contracts` 直接 import/callsite。
- `contracts` crate 中有大量 DTO 定义不涉及 internal conversion；本报告只列 domain/SPI/protocol conversion 与 direct protocol reuse。
- `From<domain> for DTO` 是否迁移不应一刀切；简单 enum/value projection 可暂作 allowed projection conversion，含 request/domain command 语义的 reverse conversion 是优先 migration candidate。
