# Research: contract-boundary-deep-dive

- Query: deep-dive application/contracts/API/frontend generated DTO 与手写 DTO/stream 的边界问题，基于第一轮报告与必要代码抽样输出后续可拆任务候选。
- Scope: internal
- Date: 2026-06-21

## Findings

## 结论摘要

1. **最高风险边界是 Project-level NDJSON 仍未进入 contract crate。** Session stream 已有 `SessionNdjsonEnvelope` generated DTO，而 Project stream 仍由 domain `StreamEvent` 经 API 直接序列化，再由前端 `api/eventStream.ts` 手写解析；这违反 cross-layer spec 中“NDJSON envelope 属于 contract”的规则。
2. **`agentdash-application` 已直接依赖 `agentdash-contracts`，不是单点偶发。** application `Cargo.toml` 直接依赖 contracts，并在 AgentRun conversation snapshot、workspace query、session eventing、workspace module 等模块组装 browser-facing DTO；这让 use case/read model 与 wire DTO 同步演进。
3. **`agentdash-contracts` 当前是“wire DTO + 内部模型转换 adapter”混合层。** 它依赖 domain/SPI/agent protocol，并内置大量 `From<domain/spi/protocol>`，MCP preset 还存在 DTO 与 domain 的双向 `From`。这能减少 API mapper，但 contract crate 的变更敏感面已经覆盖内部模型。
4. **API route-local DTO 分两类：极小 transport wrapper 可以保留在 route/API dto，本身跨 feature 被前端消费的 response 应进入 contract crate。** 目前 BackendAccess/BackendWorkspaceInventory、Canvas CRUD、SkillAsset、SessionExecutionState、Auth/identity directory 等仍有手写 DTO 或前端 mapper，需要按消费面分级。
5. **`types/index.ts` 不是单一问题。** generated re-export 和少量 UI view model 是合理入口；`ProjectBackendAccess*`、`BackendWorkspaceInventory*`、`WorkspaceDetectionResult` 这类跨层 response 形态属于 contract 缺口；`CapabilityKey`、`AgentPresetConfig` 这类 UI convenience/view model 可继续 feature/shared-local，但不能作为 API wire source。

## 文件发现

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` | 本轮 review 父任务目标、约束、验收标准。 |
| `.trellis/tasks/06-21-module-topology-coupling-review/design.md` | review 分轮、产物 schema、耦合分类与第二轮触发规则。 |
| `.trellis/tasks/06-21-module-topology-coupling-review/research/01-backend-layer-topology.md` | 第一轮后端分层报告，指出 application 依赖 contracts 与 contracts 依赖 domain/SPI/protocol。 |
| `.trellis/tasks/06-21-module-topology-coupling-review/research/06-frontend-contracts-topology.md` | 第一轮前端 contracts 报告，指出 Project NDJSON、`types/index.ts`、route-local mapper 需深挖。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | cross-layer contract 规则：Rust contract -> generated TS；NDJSON envelope 属于 contract；route-local DTO 仅限极小 wrapper。 |
| `.trellis/spec/frontend/type-safety.md` | 前端类型安全规则：generated wire 单源、mapper 边界、view model 与 DTO 分层。 |
| `crates/agentdash-application/Cargo.toml` | application 直接依赖 `agentdash-contracts`。 |
| `crates/agentdash-contracts/Cargo.toml` | contracts 依赖 `agentdash-domain` / `agentdash-spi` / `agentdash-agent-protocol` / `agentdash-agent-types`。 |
| `crates/agentdash-contracts/src/runtime/session.rs` | Session HTTP/NDJSON DTO 与 SPI/protocol 转换集中点。 |
| `crates/agentdash-contracts/src/integration/mcp_preset.rs` | MCP preset DTO 与 domain 双向转换集中点。 |
| `crates/agentdash-domain/src/common/events.rs` / `crates/agentdash-api/src/stream.rs` | Project-level event stream 的 domain/API source。 |
| `packages/app-web/src/api/eventStream.ts` / `packages/app-web/src/types/acp.ts` / `packages/app-web/src/stores/eventStore.ts` | Project-level NDJSON 前端手写 type/parser/store 链路。 |
| `packages/app-web/src/types/index.ts` | generated re-export、手写 DTO、view model 混合入口。 |
| `packages/app-web/src/services/*` | 前端 service mapper 抽样，区分 contract 缺口与 feature-local view model。 |

## 主链路拓扑

### 1. 标准 generated contract 链路

```text
agentdash-contracts Rust DTO
  -> serde wire shape / ts-rs generation
  -> packages/app-web/src/generated/*-contracts.ts
  -> frontend service 返回 generated type
  -> store/reducer/hook 转换为 feature view model
```

这条链路在 Session stream、AgentRun workspace、VFS surface、Extension runtime、Workflow/Task plan 等主路径上已经存在。典型证据：`SessionNdjsonEnvelope` 在 contracts 中定义为 connected/event/heartbeat union (`crates/agentdash-contracts/src/runtime/session.rs:76`)，生成到前端 `packages/app-web/src/generated/session-contracts.ts:47`，Session transport 以 generated envelope 解析 (`packages/app-web/src/features/session/model/streamTransport.ts:12`, `packages/app-web/src/features/session/model/streamTransport.ts:65`, `packages/app-web/src/features/session/model/streamTransport.ts:84`)。

### 2. application 组装 browser-facing projection 链路

```text
application use case / read model
  -> imports agentdash_contracts DTO
  -> 直接构造 AgentConversationSnapshot / AgentRunWorkspaceView / SessionEventResponse / WorkspaceModulePresentation
  -> API route 透传 Json<contract DTO>
  -> frontend generated DTO
```

证据：application manifest 直接依赖 `agentdash-contracts` (`crates/agentdash-application/Cargo.toml:10`)；`conversation_snapshot.rs` import contract workflow/vfs DTO (`crates/agentdash-application/src/agent_run/conversation_snapshot.rs:3`) 并返回 `AgentConversationSnapshot` (`crates/agentdash-application/src/agent_run/conversation_snapshot.rs:270`)；workspace query import `agentdash_contracts::vfs` 和 `workflow` (`crates/agentdash-application/src/agent_run/workspace/query.rs:1`, `crates/agentdash-application/src/agent_run/workspace/query.rs:2`)；session eventing import contract session DTO (`crates/agentdash-application/src/session/eventing.rs:8`)；workspace module application module import `WorkspaceModule*` contract DTO (`crates/agentdash-application/src/workspace_module/mod.rs:17`)。

### 3. contracts 内置内部模型转换链路

```text
domain / SPI / agent-protocol model
  -> agentdash-contracts impl From / helper conversion
  -> contract DTO
  -> API route 或 application 直接使用
```

证据：contracts manifest 依赖 internal crates (`crates/agentdash-contracts/Cargo.toml:15`, `crates/agentdash-contracts/Cargo.toml:16`, `crates/agentdash-contracts/Cargo.toml:17`, `crates/agentdash-contracts/Cargo.toml:18`)；Session contract import `BackboneEnvelope`、agent types、SPI hooks/session persistence (`crates/agentdash-contracts/src/runtime/session.rs:6`, `crates/agentdash-contracts/src/runtime/session.rs:7`, `crates/agentdash-contracts/src/runtime/session.rs:11`, `crates/agentdash-contracts/src/runtime/session.rs:16`) 并实现 `From<PersistedSessionEvent>` (`crates/agentdash-contracts/src/runtime/session.rs:47`)；Workspace contract 对 domain workspace enum/aggregate 实现 `From` (`crates/agentdash-contracts/src/workspace/contract.rs:15`, `crates/agentdash-contracts/src/workspace/contract.rs:101`, `crates/agentdash-contracts/src/workspace/contract.rs:134`)；MCP preset 存在 DTO 与 domain 双向转换 (`crates/agentdash-contracts/src/integration/mcp_preset.rs:81`, `crates/agentdash-contracts/src/integration/mcp_preset.rs:107`, `crates/agentdash-contracts/src/integration/mcp_preset.rs:142`, `crates/agentdash-contracts/src/integration/mcp_preset.rs:151`)。

### 4. Project-level NDJSON 手写链路

```text
domain StreamEvent
  -> agentdash-api stream.rs 直接 serializes NDJSON
  -> packages/app-web/src/api/eventStream.ts 手写 parse
  -> packages/app-web/src/types/acp.ts StreamEvent
  -> stores/eventStore.ts project event fanout
```

证据：domain 定义 `StreamEvent` union (`crates/agentdash-domain/src/common/events.rs:11`)；API stream 直接 import domain event (`crates/agentdash-api/src/stream.rs:13`) 并提供 stream query (`crates/agentdash-api/src/stream.rs:25`)；前端 parser 以 `Record<string, unknown>` 手写 `Connected/StateChanged/BackendRuntimeChanged/Heartbeat` (`packages/app-web/src/api/eventStream.ts:19`, `packages/app-web/src/api/eventStream.ts:35`, `packages/app-web/src/api/eventStream.ts:41`, `packages/app-web/src/api/eventStream.ts:45`, `packages/app-web/src/api/eventStream.ts:64`, `packages/app-web/src/api/eventStream.ts:68`)；事件类型仍在 `types/acp.ts` 手写 (`packages/app-web/src/types/acp.ts:123`)；store 消费该手写 `StreamEvent` (`packages/app-web/src/stores/eventStore.ts:2`, `packages/app-web/src/stores/eventStore.ts:27`, `packages/app-web/src/stores/eventStore.ts:69`)。

## 耦合矩阵

| From | To | Relationship | Evidence | Risk | Suggested follow-up task |
| --- | --- | --- | --- | --- | --- |
| `agentdash-application` | `agentdash-contracts` | application 直接构造/返回 browser-facing contract DTO。 | `crates/agentdash-application/Cargo.toml:10`; `crates/agentdash-application/src/agent_run/conversation_snapshot.rs:3`; `crates/agentdash-application/src/session/eventing.rs:8`; `crates/agentdash-application/src/workspace_module/mod.rs:17` | P0/P1：use case/read model 与 wire DTO 合并，后续 DTO 调整会牵动 application 编排。 | “梳理 application contract DTO 依赖并明确 projection assembly owner”。 |
| `agentdash-contracts` | domain/SPI/protocol | contracts 同时是 wire DTO source 与内部模型 adapter。 | `crates/agentdash-contracts/Cargo.toml:15-18`; `crates/agentdash-contracts/src/runtime/session.rs:6-16`; `crates/agentdash-contracts/src/workspace/contract.rs:101`; `crates/agentdash-contracts/src/runtime/session.rs:47` | P1：内部模型变更会直接改变 contract crate；`From` 所在层规则不清会继续扩张。 | “定义并收敛 contracts 内部模型转换边界”。 |
| `agentdash-contracts::mcp_preset` | domain MCP preset | DTO/domain 双向 `From` 让 request DTO 可直接进入 domain。 | `crates/agentdash-contracts/src/integration/mcp_preset.rs:81`; `crates/agentdash-contracts/src/integration/mcp_preset.rs:107`; `crates/agentdash-contracts/src/integration/mcp_preset.rs:142`; `crates/agentdash-contracts/src/integration/mcp_preset.rs:151` | P1：incoming validation/domain command mapping 可能被放进 contract crate。 | “拆分 MCP preset wire DTO 与 domain command conversion owner”。 |
| Project stream API | Frontend `eventStream.ts` / `types/acp.ts` | Project NDJSON envelope 未进入 generated contract，前端手写解析 stream payload。 | `crates/agentdash-domain/src/common/events.rs:11`; `crates/agentdash-api/src/stream.rs:13`; `packages/app-web/src/api/eventStream.ts:35`; `packages/app-web/src/types/acp.ts:123` | P0：stream envelope、cursor、事件 payload 无 contract check，最容易 drift。 | “将 Project event NDJSON envelope 纳入 agentdash-contracts 并生成前端类型”。 |
| API `dto/backend_access.rs` | Frontend `types/index.ts` backend access/inventory types | 跨 Project/Backend/Workspace 的 response 仍是 API-local DTO + frontend hand type。 | `crates/agentdash-api/src/dto/backend_access.rs:11`; `crates/agentdash-api/src/dto/backend_access.rs:30`; `crates/agentdash-api/src/dto/backend_access.rs:65`; `packages/app-web/src/types/index.ts:93`; `packages/app-web/src/types/index.ts:322`; `packages/app-web/src/types/index.ts:337` | P1：跨 feature 复用的 project-backend binding/inventory facts 缺少 generated source。 | “将 ProjectBackendAccess 与 BackendWorkspaceInventory DTO contract 化”。 |
| API Canvas DTO | Frontend `services/canvas.ts` / generated canvas runtime DTO | Canvas CRUD response 仍手写 map；contract crate 只看到 runtime snapshot 侧覆盖。 | `crates/agentdash-api/src/dto/canvas.rs:57`; `packages/app-web/src/services/canvas.ts:33`; `packages/app-web/src/services/canvas.ts:48`; `packages/app-web/src/services/canvas.ts:84`; `packages/app-web/src/generated/canvas-contracts.ts:14` | P1：Canvas CRUD 与 Canvas runtime contract 分裂，容易出现 service mapper 默认值掩盖 drift。 | “补齐 Canvas CRUD contract 并删除前端 identity/default mapper”。 |
| API SkillAsset DTO | Frontend `services/skillAsset.ts` / `types/skill-asset.ts` | Skill asset HTTP DTO 仍手写类型与 mapper，service 还默认填充缺失字段。 | `crates/agentdash-api/src/routes/skill_assets.rs:76`; `packages/app-web/src/types/skill-asset.ts:9`; `packages/app-web/src/services/skillAsset.ts:80`; `packages/app-web/src/services/skillAsset.ts:97`; `packages/app-web/src/services/skillAsset.ts:525` | P1：Skill asset 作为 Project asset 与 UI editor 共同消费，缺少 generated DTO 会放大字段漂移。 | “将 SkillAsset HTTP DTO 纳入 contracts，区分 wire DTO 与 editor draft view model”。 |
| generated extension-management DTO | Frontend `services/extensionManagement.ts` | 已有 generated contract，但 service 仍以 `unknown` 手写校验与重建 response。 | `packages/app-web/src/generated/extension-management-contracts.ts:10`; `crates/agentdash-contracts/src/extension/management.rs:47`; `packages/app-web/src/services/extensionManagement.ts:122`; `packages/app-web/src/services/extensionManagement.ts:152`; `packages/app-web/src/services/extensionManagement.ts:159` | P1：不是 contract 缺口，而是前端绕过 generated wire 单源。 | “让 ExtensionManagement service 直接消费 generated DTO，保留必要 view model mapper”。 |
| Session runtime state API | Frontend `types/session.ts` / `services/session.ts` | Session execution state 仍由 API-local DTO 和前端 mapper 表达。 | `crates/agentdash-api/src/dto/session.rs:16`; `crates/agentdash-api/src/routes/sessions.rs:298`; `packages/app-web/src/types/session.ts:69`; `packages/app-web/src/services/session.ts:105`; `packages/app-web/src/services/session.ts:119` | P1/P2：若仍被 AgentRun/workspace control 使用，则属于 generated contract 缺口；若只做 legacy page 状态，可降级。 | “确认 SessionExecutionState 消费面并决定是否 contract 化”。 |
| Workspace module platform event | Frontend `presentation.ts` | HTTP present 已 generated，但 session platform event payload 仍按 `Record<string, unknown>` 解析。 | `packages/app-web/src/generated/workspace-module-contracts.ts:112`; `packages/app-web/src/features/workspace-module/model/presentation.ts:1`; `packages/app-web/src/features/workspace-module/model/presentation.ts:49`; `packages/app-web/src/features/workspace-module/model/presentation.ts:61` | P1：HTTP 与 stream event presentation 可能出现两套形态。 | “让 workspace_module_presented stream payload 使用 generated DTO”。 |
| `types/index.ts` generated aliases | generated contract files | 作为 ergonomics re-export，基本符合 spec。 | `packages/app-web/src/types/index.ts:2`; `packages/app-web/src/types/index.ts:39`; `packages/app-web/src/types/index.ts:41`; `packages/app-web/src/types/index.ts:270`; `packages/app-web/src/types/index.ts:393` | P2：不是直接风险，但文件混入太多手写 wire 类型降低可读性。 | “拆分 types/index.ts：generated aliases、shared view model、legacy wire gaps 分文件”。 |
| `CapabilityKey` / `CapabilityOption` | Workflow UI | 前端内置能力选项和 UI 展示 view model，不是 API DTO source。 | `packages/app-web/src/types/index.ts:128`; `packages/app-web/src/types/index.ts:139`; `packages/app-web/src/types/index.ts:151`; `.trellis/spec/frontend/type-safety.md` | Low：按 spec 可存在；风险在于被拿去收窄 API `CapabilityDirective`。 | 不单独拆任务；纳入 `types/index.ts` 分类清理验收。 |
| `AgentPresetConfig` / ProjectAgent view aliases | generated ProjectAgent DTO | UI convenience over JSON/config blob，与 generated DTO 组合生成 view model。 | `packages/app-web/src/types/index.ts:246`; `packages/app-web/src/types/index.ts:275`; `packages/app-web/src/types/index.ts:283`; `packages/app-web/src/types/index.ts:304` | P2：可保留为 view model，但 service 入参/出参不能以它替代 generated wire。 | “ProjectAgent config view model 与 generated wire DTO 边界标注/拆分”。 |
| Auth/current user / identity directory DTO | API auth/directory routes | 仍是手写 API dto + frontend mapper。 | `crates/agentdash-api/src/routes/auth_routes.rs:135`; `packages/app-web/src/types/index.ts:173`; `packages/app-web/src/types/index.ts:211`; `packages/app-web/src/services/currentUser.ts:19`; `packages/app-web/src/services/currentUser.ts:39` | P2：跨 app shell 消费，值得 contract 化，但不阻塞主业务 DTO 收敛。 | “将 auth/current-user/identity-directory DTO contract 化或明确为 auth transport wrapper”。 |

## Code Patterns

- 正向模式：Session NDJSON 使用 contract/generated。`SessionNdjsonEnvelope` 在 Rust contract 中定义 (`crates/agentdash-contracts/src/runtime/session.rs:76`)，前端 generated 文件暴露同名 union (`packages/app-web/src/generated/session-contracts.ts:47`)，Session transport 只在 unknown -> generated event envelope 边界校验 (`packages/app-web/src/features/session/model/streamTransport.ts:65`, `packages/app-web/src/features/session/model/streamTransport.ts:84`)。
- 风险模式：Project NDJSON 以 domain `StreamEvent` 为 wire source，前端手写 parse。证据见 `crates/agentdash-domain/src/common/events.rs:11`、`crates/agentdash-api/src/stream.rs:13`、`packages/app-web/src/api/eventStream.ts:35`、`packages/app-web/src/types/acp.ts:123`。
- 风险模式：contracts crate 同时承担 DTO 与内部转换。Session contract 从 SPI persistence event 转 DTO (`crates/agentdash-contracts/src/runtime/session.rs:47`)，Workspace contract 从 domain aggregate 转 response (`crates/agentdash-contracts/src/workspace/contract.rs:134`)，MCP preset 有 request/domain 双向转换 (`crates/agentdash-contracts/src/integration/mcp_preset.rs:107`, `crates/agentdash-contracts/src/integration/mcp_preset.rs:151`)。
- 风险模式：前端 service 对内部 API response 做手写重建和默认值填充。Canvas mapper 从 `Record<string, unknown>` 重建 `Canvas` (`packages/app-web/src/services/canvas.ts:48`, `packages/app-web/src/services/canvas.ts:84`)；SkillAsset mapper 从 raw response 重建 DTO 并填默认 timestamp (`packages/app-web/src/services/skillAsset.ts:80`, `packages/app-web/src/services/skillAsset.ts:97`)；ExtensionManagement 已有 generated DTO 但仍手写 mapper (`packages/app-web/src/generated/extension-management-contracts.ts:10`, `packages/app-web/src/services/extensionManagement.ts:122`)。
- 可接受模式：feature/shared view model 由 generated DTO 派生。`CapabilityKey` 是 UI option key (`packages/app-web/src/types/index.ts:128`, `packages/app-web/src/types/index.ts:139`)；`AgentPresetConfig` 是 JSON config 的 UI convenience wrapper (`packages/app-web/src/types/index.ts:246`)；这些应与 API wire DTO 文件分离，但不一定进入 contract crate。

## P0 backlog candidates

1. **Project event NDJSON contract 化**
   - 问题：Project-level event stream 仍是 domain/API/前端三处手写事实源。
   - 影响范围：`crates/agentdash-domain/src/common/events.rs`、`crates/agentdash-api/src/stream.rs`、`packages/app-web/src/api/eventStream.ts`、`packages/app-web/src/types/acp.ts`、`packages/app-web/src/stores/eventStore.ts`。
   - 验收方向：Rust contract 定义 Project event NDJSON envelope；生成 TS；前端 stream parser 消费 generated union；`pnpm run contracts:check` 能捕获 drift。

2. **application contract DTO 依赖归属审计**
   - 问题：application 直接构造 browser-facing DTO，wire DTO 与 use case/read model 边界不清。
   - 影响范围：`agent_run/conversation_snapshot.rs`、`agent_run/workspace/query.rs`、`session/eventing.rs`、`workspace_module/mod.rs`、`capability/tool_catalog.rs`。
   - 验收方向：逐项标出哪些是 application-level read model，哪些是 API/contract adapter；确定 owner 后迁移或明确边界，不设计兼容层。

## P1 backlog candidates

1. **contracts crate 内部转换边界收敛**
   - 问题：`agentdash-contracts` 依赖 domain/SPI/protocol 并内置大量 `From`，尤其 MCP preset 双向转换会让 request -> domain command mapping 留在 DTO crate。
   - 验收方向：列出允许保留的 outbound projection conversion 与需要迁移的 incoming/domain command conversion；更新代码后 contracts 仍是 generated wire source。

2. **ProjectBackendAccess / BackendWorkspaceInventory contract 化**
   - 问题：跨 Project/Backend/Workspace 的 binding/inventory DTO 仍在 API dto 与 `types/index.ts` 手写。
   - 验收方向：Rust contract + generated TS 覆盖 access、inventory、candidate、refresh/sync response；frontend service 直接消费 generated DTO。

3. **Canvas CRUD contract 化**
   - 问题：Canvas runtime snapshot 已 generated，但 Canvas CRUD response 仍在 API/front service 手写。
   - 验收方向：Canvas CRUD request/response 进入 contracts；`services/canvas.ts` 删除 raw mapper；editor draft 保留为 feature view model。

4. **SkillAsset contract 化并拆 editor draft view model**
   - 问题：SkillAsset HTTP DTO 与 editor markdown/frontmatter draft 混在同一 service/type 文件。
   - 验收方向：SkillAsset list/create/update/import response 生成；editor draft/validation/frontmatter parser 保持 feature-local。

5. **ExtensionManagement service 回到 generated DTO**
   - 问题：已有 generated `extension-management-contracts.ts`，但 service 仍用 `unknown` 手写校验重建。
   - 验收方向：service 返回 generated `ProjectExtensionManagementListResponse` / item；只保留 UI view model 转换，不重建 wire DTO。

6. **workspace_module_presented stream payload contract 化**
   - 问题：HTTP present DTO 已 generated，platform event payload 仍 Record mapper。
   - 验收方向：Backbone/session platform event 的 `workspace_module_presented` payload 消费同源 generated DTO；前端打开 tab 从 generated payload 取 `presentation_uri`。

## P2 backlog candidates

1. **拆分 `types/index.ts` 的职责**
   - 问题：generated aliases、shared view model、auth DTO、backend inventory wire gaps 混在一个入口。
   - 验收方向：保留 generated alias 入口；把 UI view model 拆到 feature/shared model 文件；把已确认的 wire gaps 移入 contracts。

2. **SessionExecutionState 消费面确认**
   - 问题：`/sessions/{id}/state` 仍是 API-local DTO + frontend mapper；是否仍是主链路事实源需要确认。
   - 验收方向：若仍被 Session/AgentRun control UI 消费，纳入 contracts；若只剩诊断/legacy endpoint，标注为 route-local wrapper。

3. **Auth/current-user/identity-directory DTO contract 化或明确 wrapper 归属**
   - 问题：app shell 跨页面消费 current user 和 identity directory，但仍是手写 DTO。
   - 验收方向：若继续作为前端 app-wide 事实源，则进 contracts；否则收缩到 auth feature-local service boundary。

4. **ProjectAgent config view model 边界标注**
   - 问题：`AgentPresetConfig`、`ProjectAgentExecutor` 等是 generated DTO + UI config wrapper 的组合。
   - 验收方向：确保 service wire 使用 generated ProjectAgent DTO；UI config wrapper 只在 editor/picker 内部使用。

## 不重复项

- 不重复 06-14 已覆盖的 AgentRun workspace command/mailbox projection 重复、RuntimeSession control 漂移、PermissionGrant/companion grant 双事实源、Capability catalog contract 化方向、大组件消费过宽 DTO 等问题；本报告只使用这些结论定位 contract 边界。
- 不重复第一轮 `01-backend-layer-topology.md` 的 crate 全局依赖图与 API route 直连 `state.repos` 问题；这里只深挖 application/contracts/API/frontend DTO 边界。
- 不重复第一轮 `06-frontend-contracts-topology.md` 的前端全模块拓扑；这里只展开 Project NDJSON、`types/index.ts`、route-local mapper 与 generated DTO 使用边界。
- 不把 `CapabilityKey`、`AgentPresetConfig`、editor draft、markdown/frontmatter parser 这类 UI/view model 直接判为 contract 缺口；它们的问题是文件归属和 service 边界，而不是一定要进 Rust contract。

## External References

- 未使用外部资料。本次为内部代码、任务报告与 Trellis 规范静态盘查。

## Related Specs

- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/research/01-backend-layer-topology.md`
- `.trellis/tasks/06-21-module-topology-coupling-review/research/06-frontend-contracts-topology.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 在当前 shell 返回 `Current task: (none)`；本报告按用户显式指定的 task path 与输出文件写入。
- 未运行测试、`contracts:check`、`frontend:check` 或服务端编译；本报告是只读架构研究。
- 未全量审查所有 route-local DTO；本轮抽样聚焦第一轮报告点名的 application/contracts/API/frontend DTO/stream 边界。
- 未发现 Project-level event NDJSON 对应的 `agentdash-contracts` generated DTO；搜索命中的是 domain `StreamEvent`、API `stream.rs` 和前端手写 `StreamEvent`。
