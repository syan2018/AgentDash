# Research: CB03 Owner Map

- Query: 完成 Contract Boundary CB03 owner map，给出 application read model / API adapter / contract DTO / allowed projection conversion 的 owner rules，并从 CB01/CB02 audit 归类主要模块与 CB04 高风险迁移候选。
- Scope: internal
- Date: 2026-06-21

## Findings

## Scope And Source Material

`python ./.trellis/scripts/task.py current --source` 在当前 shell 返回 `Current task: (none)` / `Source: none`；本文件按用户显式指定 task 目录 `.trellis/tasks/06-21-contract-boundary-ownership-audit/` 整理。

本轮读取的任务与规范材料：

| Path | Description |
| --- | --- |
| `AGENTS.md` | 项目级协作约束；中文沟通、预研期保持正确状态、不做兼容回退。 |
| `.trellis/workflow.md` | Trellis research 产物需持久化；当前 agent 仅做设计/审计，不实现代码。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/prd.md` | 本任务目标：审计 application、contracts、API adapter 与 generated contracts 的 DTO owner 和转换边界。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/design.md` | 需要评估 read model / wire DTO 分离、API adapter 默认映射、contracts crate conversion 边界。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/index.md` | CB01/CB02 已完成，CB03 阻塞于 CB01/CB02，CB04 阻塞于 CB03。 |
| `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/01-ownership-audit.md` | CB01/CB02 的 import-level 和 conversion-level 审计输入。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | Contract crate 和 generated TS 的职责、API layer mapping 约定、NDJSON envelope 与 route-local DTO 规则。 |

## Owner Rules

### Application Read Model

Application read model owns backend-internal read/query state, use case facts, policy preconditions, and aggregation decisions before they become browser-facing JSON.

Rules:

- Application read model may depend on domain/SPI/application services and may encode use case state, command availability, stale guard, execution state, capability facts, VFS resolution facts, and workspace shell/list/detail projection.
- Application read model should not use `agentdash-contracts` as its internal shape when the same facts are consumed by application policy, scheduler, command handling, or multiple adapters.
- Application read model may expose structs that are close to wire shape only when the type is still clearly backend-owned and not generated/browser-facing.
- Application tests should assert application semantics against read models; route/API tests should assert final contract DTO shape.

Key evidence:

- `AgentConversationSnapshotResolver::resolve` currently returns a contract DTO directly at `crates/agentdash-application/src/agent_run/conversation_snapshot.rs:270` and constructs `AgentConversationSnapshot` at `crates/agentdash-application/src/agent_run/conversation_snapshot.rs:295`.
- `AgentRunWorkspaceSnapshot` is application-local but embeds `AgentConversationSnapshot` and `SubjectRefDto` at `crates/agentdash-application/src/agent_run/workspace/types.rs:21`, `:34`, `:52`.
- Command policy currently reasons over `AgentConversationSnapshot` and generated execution status at `crates/agentdash-application/src/agent_run/workspace/command_policy.rs:298`, `:532`, `:538`.

### API Adapter

API adapter owns route request parsing into application command/read inputs and application read model to contract DTO mapping.

Rules:

- API adapter is the default boundary for `application read model -> contract DTO`.
- API adapter owns `wire request DTO -> domain/application command` conversion when conversion affects command intent, validation, patch semantics, idempotency, or use case defaults.
- API adapter may delegate pure field mapping to small mapper modules, but those modules should remain adapter/projection code, not domain/application service owners.
- API adapter should preserve generated contract DTO as route input/output shape for cross-feature browser APIs.

Key evidence:

- Spec says `agentdash-api` uses contract crate as route input/output, and when route needs an application/domain model internally, the API layer owns mapping into contract DTOs in `.trellis/spec/cross-layer/frontend-backend-contracts.md`.
- Spec says route-local DTO is only for tiny transport wrappers; cross-feature/front-end/stream DTO belongs in contract crate.

### Contract DTO

Contract DTO owns serde wire shape, generated TypeScript source, NDJSON/HTTP envelope shape, browser-facing request/response unions, and shared runtime protocol value objects intentionally exposed to clients.

Rules:

- Contract DTO may depend on serialization/generation shape and protocol facts that are deliberately part of the wire contract.
- Contract DTO should stay free of application policy, command construction, repository/application use case assembly, and backend-only read model decisions.
- Reusing canonical protocol input is allowed when the spec explicitly defines that protocol shape as the contract, such as `Vec<codex::UserInput>` for session turn control.
- NDJSON stream envelopes remain contract-owned because cursor, event fact, and reducer input must evolve together.

Key evidence:

- `AgentRunComposerSubmitRequest.input` uses `Vec<codex::UserInput>` at `crates/agentdash-contracts/src/agent/run_mailbox.rs:189`; spec records this as canonical session turn control input.
- `CreateProjectAgentRunRequest.input` uses `Vec<codex::UserInput>` at `crates/agentdash-contracts/src/agent/project_agent.rs:63`; spec records ProjectAgent draft start sharing the same input shape.
- `SessionEventResponse.notification` embeds `BackboneEnvelope` at `crates/agentdash-contracts/src/runtime/session.rs:43`, `:44`; session NDJSON envelope is contract-owned.

### Allowed Projection Conversion

Allowed projection conversion is narrow, outward-only mapping from stable domain/SPI/protocol facts into DTO fields when the mapping is mostly structural and does not decide use case behavior.

Rules:

- Allowed direction is internal fact/value object -> contract DTO.
- Acceptable examples are enum/value projection, stream/event projection, response DTO projection for stable read facts, and protocol envelope projection.
- Not acceptable as allowed projection conversion: DTO -> domain/application command conversion, patch/update semantics, command validation, policy decisions, or multi-service read aggregation.
- If conversion starts inspecting multiple sources, applying use case defaults, filtering for policy, or returning application-consumed state, classify as application read model or API adapter migration.

Key evidence:

- `crates/agentdash-contracts/src/context/contract.rs:15` maps `MountCapability` to `VfsCapabilityDto`, a narrow outbound projection.
- `crates/agentdash-contracts/src/runtime/session.rs:329`, `:348`, `:365` map lineage/message ref facts to DTOs, while the reverse `SessionMessageRefDto -> MessageRef` at `:374` is a migration candidate.
- `crates/agentdash-contracts/src/integration/mcp_preset.rs:81`, `:142`, `:168`, `:199`, `:242` are outbound MCP preset projections, while reverse DTO -> domain conversions at `:107`, `:151`, `:178`, `:216`, `:253` are command-adapter candidates.

## Owner Map

| Area / Module | Current Owner | Proposed Owner | Action | Rationale |
| --- | --- | --- | --- | --- |
| `crates/agentdash-application/Cargo.toml` direct `agentdash-contracts` dependency | application crate | transitional dependency while migrating specific modules | document | The dependency is an effect of multiple active projection paths; remove only after module-level migrations reduce the import surface. |
| `agent_run/conversation_snapshot.rs` | application constructs `AgentConversationSnapshot` contract DTO | application read model + API adapter mapper to contract DTO | migrate | Resolver derives execution/config/command/diagnostic state and should not expose generated DTO as internal application shape. |
| `agent_run/project_agent_start.rs` effective executor config | application carries `ConversationEffectiveExecutorConfigView` | application read model for config source/effective config; API adapter maps if browser-facing | migrate | Start flow uses executor config as dispatch/application state, not only browser response JSON. |
| `agent_run/workspace/types.rs` | application read shell embeds contract DTOs | application read model | migrate | Workspace snapshot is internal application projection but currently embeds `AgentConversationSnapshot` and `SubjectRefDto`. |
| `agent_run/workspace/query.rs` conversation snapshot + subject association | application constructs contract DTOs | application read model + API adapter | migrate | Query service assembles workspace state; DTO creation should move to adapter/projection boundary. |
| `agent_run/workspace/query.rs` VFS `ResolvedVfsSurface` mapping | application-local contract mapper | API adapter or narrow projection mapper | migrate | Mapping is outward projection to contract VFS; acceptable as mapper, but should not remain mixed into application query assembly. |
| `agent_run/workspace/command_policy.rs` | application command policy consumes contract status/precondition DTOs | application command precondition model; API adapter parses/generated DTO | migrate | Policy should reason over backend command state, while generated stale guard/precondition belongs at wire boundary. |
| `capability/tool_catalog.rs` | application service returns `CapabilityCatalogResponse` and tool DTOs | application capability catalog read model + API adapter DTO mapper | migrate | SPI/platform capability facts are application projection; contract DTO remains browser response owner. |
| `workspace_module/mod.rs` descriptor/presentation aggregation | application aggregation returns contract DTOs | allowed projection conversion shared by browser and agent tool contract | document | Workspace module is intentionally shared by frontend and agent tool JSON; keep DTO contract while documenting this as allowed projection conversion. |
| `workspace_module/tools.rs` operation dispatch on contract DTOs | agent tool adapter consumes shared contract DTO | contract DTO + allowed projection conversion | keep | Tool result/invocation protocol reuses the same workspace module contract; splitting now would create duplicate agent/browser JSON shapes. |
| `session/eventing.rs` context usage response | application calls contracts helper returning response DTO | application projection service + API/stream mapper to `SessionContextUsageItemResponse` | migrate | The helper analyzes SPI `ContextFrame`; analysis belongs to application projection, while response item DTO remains contract-owned. |
| `contracts/context/contract.rs` outbound context/VFS capability mapping | contracts | allowed projection conversion | keep | Narrow domain value -> DTO projection with no command semantics. |
| `contracts/backend/contract.rs` outbound backend/access/inventory response mapping | contracts | allowed projection conversion unless assembly/policy grows; API adapter for command parsing | document | Response DTOs are contract-owned; reverse access status/mode conversion should be split when route command handling is touched. |
| `contracts/project/contract.rs` project stream/config/share projection | contracts | allowed projection conversion | keep | Project event/stream envelope and narrow domain -> DTO projection fit contract crate role. |
| `contracts/story/contract.rs` story response projection | contracts | allowed projection conversion | keep | Straight outward story fact projection. |
| `contracts/workspace/contract.rs` workspace response projection | contracts | allowed projection conversion | keep | Straight outward workspace fact projection. |
| `contracts/integration/skill_asset.rs` skill asset response projection | contracts | allowed projection conversion | keep | Response projection can stay; no audited reverse command conversion. |
| `contracts/integration/llm_provider.rs` outbound provider/credential DTO mapping | contracts | allowed projection conversion | document | Outbound mapping is fine; reverse DTO -> domain credential/protocol conversion should migrate with provider command routes. |
| `contracts/integration/mcp_preset.rs` outbound preset response projection | contracts | allowed projection conversion | keep | Saved preset response DTO belongs in contracts. |
| `contracts/integration/mcp_preset.rs` DTO -> domain transport/runtime binding/route policy conversions | contracts | API adapter/application command builder | split task | Highest-risk incoming command conversion cluster; create focused CB04 implementation task. |
| `contracts/runtime/routine.rs` outbound routine response projection | contracts | allowed projection conversion | document | Routine response DTO projection can stay. |
| `contracts/runtime/routine.rs` `RoutineDispatchStrategyDto -> DispatchStrategy` | contracts | API adapter/application command builder | migrate | Reverse command/value conversion should move near routine request handling. |
| `contracts/runtime/session.rs` `SessionNdjsonEnvelope` / `SessionEventResponse` / protocol envelope | contracts | contract DTO | keep | NDJSON stream envelope and embedded runtime event fact are explicitly contract-owned. |
| `contracts/runtime/session.rs` session lineage/message ref outbound mapping | contracts | allowed projection conversion | keep | Narrow projection from persisted/session facts to DTO. |
| `contracts/runtime/session.rs` `SessionMessageRefDto -> MessageRef` | contracts | API adapter/session command mapper | migrate | Reverse request value parser should live near fork/rollback/command handling if request-facing. |
| `contracts/runtime/session.rs` `context_usage_items_from_context_frame` | contracts | application projection service | split task | The function inspects SPI `ContextFrame` and is called from application eventing, so it is application projection logic. |
| `contracts/system/auth.rs` auth/current-user/directory mapping | contracts | allowed projection conversion for narrow identity mapping; application/API for directory assembly | document | Contract owns auth DTOs, but broader directory/read model assembly should remain outside contracts. |
| `contracts/task/contract.rs` task plan projection | contracts | allowed projection conversion | keep | Task plan DTO is run-scoped projection; no audited incoming conversion. |
| `contracts/system/settings.rs` `SettingScopeKind` outbound/reverse conversion | contracts | allowed projection outbound; API adapter for reverse conversion | migrate | Reverse `SettingsScopeKind -> domain SettingScopeKind` is request parsing and should move. |
| `contracts/agent/run_mailbox.rs` `Vec<codex::UserInput>` | contracts | contract DTO | keep | Spec explicitly makes Codex user input the canonical session turn control request shape. |
| `contracts/agent/project_agent.rs` `Vec<codex::UserInput>` | contracts | contract DTO | keep | Spec explicitly reuses the same canonical user input for ProjectAgent draft start. |
| `contracts/generate_ts.rs` protocol export plumbing | contracts build/generation | contract DTO generation plumbing | keep | TypeScript export generation is not a conversion owner. |

## CB04 High-Risk Migration Task Candidates

### CB04-A: Move MCP Preset Incoming Command Conversion Out Of Contracts

- Scope: `crates/agentdash-contracts/src/integration/mcp_preset.rs` reverse conversions from `McpTransportConfigDto`, runtime binding DTOs, binding source/target DTOs, and `McpRoutePolicy` into domain values.
- Proposed owner: API adapter/application command builder.
- Risk: high, because create/update/probe carry patch/static/runtime-binding semantics and probe behavior depends on required/optional runtime bindings.
- Acceptance direction: contracts keeps request/response DTO and outbound response projection; route/application command path owns request DTO -> domain command/value mapping.

### CB04-B: Split AgentRun Workspace Snapshot Into Application Read Model + Adapter Mapper

- Scope: `agent_run/conversation_snapshot.rs`, `agent_run/project_agent_start.rs`, `agent_run/workspace/types.rs`, `agent_run/workspace/query.rs`, `agent_run/workspace/command_policy.rs`.
- Proposed owner: application read model first, API adapter mapper second.
- Risk: high, because command availability, stale guard, execution status, VFS surface, subject association, and frontend workspace snapshot are currently interleaved.
- Acceptance direction: application query/policy code no longer imports `agentdash_contracts`; API adapter maps final read model to `AgentConversationSnapshot`, workspace DTO, VFS DTO, and subject DTO.

### CB04-C: Move Session Context Usage Projection Helper To Application

- Scope: `crates/agentdash-contracts/src/runtime/session.rs::context_usage_items_from_context_frame` and `crates/agentdash-application/src/session/eventing.rs`.
- Proposed owner: application session projection service; contract crate keeps `SessionContextUsageItemResponse`.
- Risk: medium-high, because NDJSON/session projection code is user-visible and context usage facts depend on SPI frame structure.
- Acceptance direction: contracts crate no longer analyzes `ContextFrame`; application produces usage read facts and stream/API boundary maps them to response item DTOs.

### CB04-D: Capability Catalog Read Model Split

- Scope: `crates/agentdash-application/src/capability/tool_catalog.rs`.
- Proposed owner: application read model for tool/capability facts; API adapter maps to `CapabilityCatalogResponse`.
- Risk: medium-high, because capability catalog crosses SPI/platform tool facts and frontend generated workflow contracts.
- Acceptance direction: catalog query returns application model; route adapter owns `ToolDescriptorDto`, `ToolSourceDto`, `CapabilityScopeDto`, and `PlatformMcpScopeDto` construction.

### CB04-E: Routine / LLM Provider / Settings Reverse Conversion Cleanup

- Scope: `crates/agentdash-contracts/src/runtime/routine.rs::From<RoutineDispatchStrategyDto>`, `crates/agentdash-contracts/src/integration/llm_provider.rs` reverse conversions, `crates/agentdash-contracts/src/system/settings.rs` reverse conversion.
- Proposed owner: corresponding API route/application command mappers.
- Risk: medium, because each item is smaller than MCP preset but shares the same anti-pattern: generated DTO parses internal command/domain values in the contract crate.
- Acceptance direction: contracts crate retains DTO definitions and outbound projection only; reverse mapping lives where requests are handled.

### CB04-F: Backend Access Command Conversion Owner Review

- Scope: `crates/agentdash-contracts/src/backend/contract.rs` reverse status/mode conversions and any route use of backend access command DTOs.
- Proposed owner: API adapter/application command builder.
- Risk: medium, because backend access/status may blend browser response DTO, permission/access state, and command parsing.
- Acceptance direction: response projection remains allowed; request/status parsing moves to adapter/application command boundary.

## Proposed Work Items Index Update

Researcher write scope prevents editing `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/index.md` directly in this turn. Recommended update:

| ID | Title | Kind | Status | Related Design | Scope |
| --- | --- | --- | --- | --- | --- |
| CB03 | owner map 文档 | design | completed | D01 | Owner rules and module action map produced in `research/cb03-owner-map.md`; requested root-level `owner-map.md` still needs main-session/doc-author write if required. |
| CB04 | 高风险 DTO construction 迁移任务拆分 | planning | ready | D01 | Use CB04-A through CB04-F candidates above; prioritize MCP preset incoming conversion and AgentRun workspace snapshot split. |

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/prd.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/design.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/work-items/index.md`
- `.trellis/tasks/06-21-contract-boundary-ownership-audit/research/01-ownership-audit.md`

## Caveats / Not Found

- 未修改业务代码，未运行编译、测试或 contract generation check；本轮是设计整理。
- 当前 Trellis researcher 写入边界只允许写 `{TASK_DIR}/research/`，因此没有直接写用户指定的 root-level `owner-map.md`，也没有直接更新 `work-items/index.md`。
- `task.py current --source` 未解析到 active task；本文件使用用户显式提供的 task 路径。
- CB04 候选是 implementation task 拆分建议，不包含具体代码改动。
