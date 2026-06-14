# Research: frontend-contracts-permission

- Query: Frontend / Contracts / Permission / Companion 是否存在过度设计、过厚抽象、重复事实源、跨层耦合、generated contracts 绕过、permission / companion capability 分散问题。
- Scope: internal
- Date: 2026-06-14

## Findings

### 摘要判断

当前 Frontend / Contracts / Permission 面不是整体过度设计，AgentRun runtime command 主路径已经基本收敛到 `AgentConversationSnapshot.commands` 与 generated DTO；真正的架构风险集中在 capability / permission 的事实源分裂：

- 正式 `PermissionGrant` 聚合、REST API、generated contract 与前端 `PermissionGrantCard` 已存在，但 companion 还保留一条 `capability_grant_request` / `capability_grant_result` JSON 协议和前端审批卡，且后端明确缺少 platform broker 闭环。
- capability catalog / visibility rule 一半在后端 SPI，一半被前端手写镜像，工具目录接口也绕过 `agentdash-contracts`。
- permission contract 已生成，但关键嵌套事实仍降级为 `JsonValue`，让 UI 继续做字段级猜测。

这些问题会让后续清理方向非常明确：permission grant 事实应以 `PermissionGrantService` + generated Permission contract 为唯一授权事实源；companion 只能作为交互/通知通道或 broker 入口，不应承载授权结果事实；capability catalog 应由后端投影成窄 contract，前端不再镜像 visibility rule。

### 文件发现

| Path | Description |
| --- | --- |
| `.trellis/spec/frontend/architecture.md` | 前端不创建第二套业务事实源，feature module 遵循 model / ui 分离。 |
| `.trellis/spec/frontend/type-safety.md` | generated wire 单源、内部 API response 通过 generated contract 消费。 |
| `.trellis/spec/frontend/state-management.md` | store 不为 generated DTO 再做字段级归一化，AgentRun commands 以后端 snapshot 为准。 |
| `.trellis/spec/frontend/workflow-activity-lifecycle.md` | Workflow definition / lifecycle runtime / AgentRun command API 的前端边界。 |
| `.trellis/spec/cross-layer/frontend-backend-contracts.md` | `agentdash-contracts -> generated/* -> frontend service/reducer` 的标准链路。 |
| `.trellis/spec/cross-layer/backbone-protocol.md` | Session event stream 允许前端主路径直接消费 `BackboneEvent`。 |
| `.trellis/spec/backend/permission/architecture.md` | Permission System 的职责是统一管理 runtime capability grant 事实。 |
| `.trellis/spec/backend/permission/grant-lifecycle.md` | PermissionGrant 生命周期契约；部分字段名与当前代码已有漂移，见 Caveats。 |
| `packages/app-web/src/features/permission/PermissionGrantCard.tsx` | 正式 PermissionGrant 审批卡。 |
| `packages/app-web/src/features/session/model/companionRequestViewModel.ts` | companion JSON payload 到 capability grant UI view model 的转换。 |
| `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx` | session feed 内嵌 companion / capability grant 交互卡。 |
| `packages/app-web/src/features/workflow/ui/panels/shared.ts` | 前端镜像 well-known capability key、label、baseline visibility。 |
| `packages/app-web/src/features/workflow/ui/panels/CapabilityPanel.tsx` | workflow capability editor 使用前端 baseline 常量和后端 tool catalog。 |
| `packages/app-web/src/features/executor-selector/model/types.ts` | executor discovery / discovered-options DTO 仍为前端手写类型。 |
| `crates/agentdash-contracts/src/permission.rs` | PermissionGrant generated contract source。 |
| `crates/agentdash-contracts/src/companion.rs` | Companion gate response contract 仅包含 generic JSON payload request。 |
| `crates/agentdash-application/src/permission/service.rs` | PermissionGrantService 状态机、policy、frame effect 编排。 |
| `crates/agentdash-application/src/companion/tools.rs` | companion platform capability grant broker 明确未闭环。 |
| `crates/agentdash-application/src/companion/payload_types.rs` | companion payload registry 定义 capability grant JSON 协议。 |
| `crates/agentdash-spi/src/platform/tool_capability.rs` | 后端 well-known capability、tool catalog、visibility rule 权威实现。 |

### 问题 1：PermissionGrant 与 companion capability grant 形成两条审批事实链

- 优先级: P0
- 问题类型: 重复事实源 / 职责漂移 / 跨层耦合
- 证据路径:
  - `crates/agentdash-application/src/permission/service.rs:71`
  - `crates/agentdash-application/src/permission/service.rs:120`
  - `crates/agentdash-application/src/permission/service.rs:141`
  - `packages/app-web/src/features/permission/PermissionGrantCard.tsx:31`
  - `crates/agentdash-application/src/companion/payload_types.rs:87`
  - `packages/app-web/src/features/session/model/companionRequestViewModel.ts:42`
  - `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx:52`
  - `crates/agentdash-application/src/companion/tools.rs:916`
  - `crates/agentdash-application/src/companion/tools.rs:1373`
- 具体代码证据:
  - `PermissionGrantService::request` 创建 grant、执行 policy、持久化，并在自动批准时应用 frame effect（`service.rs:71-139`）；`approve` 再走 domain 状态机并应用 capability frame effect（`service.rs:141-171`）。
  - 前端已有正式 `PermissionGrantCard`，直接调用 `/permission-grants/{id}/approve|reject|revoke`（`PermissionGrantCard.tsx:31-48`）。
  - companion payload registry 又注册 `capability_grant_request -> capability_grant_result`（`payload_types.rs:87-93`、`payload_types.rs:125-131`）。
  - session companion UI 根据 `payload_type === "capability_grant_request"` 渲染批准/拒绝按钮，并构造 `capability_grant_result` JSON（`companionRequestViewModel.ts:42-49`、`companionRequestViewModel.ts:105-119`、`SessionCompanionRequestCard.tsx:52-55`）。
  - 后端 platform target 明确拒绝该请求：`target=platform payload.type=capability_grant_request` 暂不支持，原因是缺少 platform permission grant broker、policy inputs 和 live runtime capability update handoff（`companion/tools.rs:916-928`、`companion/tools.rs:1373-1377`）。
- 影响面:
  - 用户可能看到 session 内 capability grant 审批卡，但这条审批只产生 companion response，不一定产生 `PermissionGrant` 聚合、capability delta 或 tool schema delta。
  - 后续实现 broker 时容易出现三处更新：companion payload registry、session UI、PermissionGrant REST/UI/runtime apply。
  - 审计、撤销、TTL、scope escalation 与 runtime transition 的事实源会分裂。
- 建议清理方向:
  - 预研期直接收敛到正确形态：`PermissionGrantService` / `PermissionGrant` 是唯一授权事实源。
  - `companion_request target=platform capability_grant_request` 只作为 broker input，broker 必须创建/返回 `PermissionGrantResponse` 或 grant id；用户审批 UI 只消费 generated permission contract。
  - 在 broker 完成前，删除或隐藏 session 内 capability grant 审批按钮，避免产生“已批准但未授权”的假事实。
  - companion response 只保留会话连续性，不作为 capability access 的 authority。

### 问题 2：PermissionGrant 列表 contract 允许 status 查询，但实现只能返回 active grant

- 优先级: P1
- 问题类型: 重复事实源 / contract 语义漂移
- 证据路径:
  - `crates/agentdash-contracts/src/permission.rs:28`
  - `crates/agentdash-api/src/routes/permission_grants.rs:102`
  - `crates/agentdash-domain/src/permission/repository.rs:15`
  - `crates/agentdash-infrastructure/src/persistence/postgres/permission_grant_repository.rs:116`
  - `crates/agentdash-infrastructure/migrations/0001_init.sql:1112`
- 具体代码证据:
  - generated query DTO 暴露 `status?: PermissionGrantStatusDto`（`permission.rs:28-39`）。
  - API `list_grants` 先按 `effect_frame_id` 或 `run_id` 调用 `list_active_by_frame` / `list_active_by_run`，再用 `query.status` 二次过滤（`permission_grants.rs:102-134`）。
  - repository trait 明确 `list_active_by_frame` 只查询 `status = applied | scope_escalated`（`repository.rs:15-22`）。
  - Postgres 实现实际 SQL 也是 `status IN ('applied', 'scope_escalated')`（`permission_grant_repository.rs:116-138`）。
  - migration 却已经有包含 `pending_user_approval` 的 status index（`0001_init.sql:1112`），说明 pending 查询曾被纳入读模型考虑。
- 影响面:
  - `GET /permission-grants?status=pending_user_approval` 在当前实现下无法返回 pending grant。
  - 正式 `PermissionGrantCard` 很难形成 pending approval inbox，session companion 卡会自然填补空缺，进一步放大问题 1 的双事实源。
  - 审批入口、撤销入口、active grant 展示入口的列表语义不一致。
- 建议清理方向:
  - 将 repository/API 改成按 `effect_frame_id | run_id` + optional `status` 查询 grant read model；`active` 可以是显式 query alias，而不是唯一 repository 方法。
  - 前端 pending/active/revoked 都走同一 generated `PermissionGrantResponse[]`，session feed 只链接到 grant id 或嵌入同一个 DTO。
  - 不保留 active-only 列表作为通用 `/permission-grants` 语义；如需要 active-only，命名成专用 selector/service。

### 问题 3：Permission generated contract 仍把 typed permission fact 降成 JsonValue

- 优先级: P1
- 问题类型: 过宽 DTO / generated contract 事实源不完整 / UI 字段猜测
- 证据路径:
  - `crates/agentdash-domain/src/permission/value_objects.rs:98`
  - `crates/agentdash-domain/src/permission/value_objects.rs:107`
  - `crates/agentdash-contracts/src/permission.rs:55`
  - `crates/agentdash-api/src/routes/permission_grants.rs:65`
  - `packages/app-web/src/features/permission/PermissionGrantCard.tsx:119`
- 具体代码证据:
  - domain 已有 typed `ScopeEscalationIntent { target_subject_kind, unlocked_paths }` 与 `PolicyDecision { outcome, matched_rules, reason }`（`value_objects.rs:98-121`）。
  - contract 却把 `scope_escalation_intent`、`policy_decision` 定义为 `Option<Value>`（`permission.rs:55-61`）。
  - API 通过 `serde_json::to_value` 转成 JSON（`permission_grants.rs:65-73`）。
  - 前端 `PermissionGrantCard` 再把 `JsonValue` 当 record 取 `target_subject_kind`（`PermissionGrantCard.tsx:119-124`）。
- 影响面:
  - generated contract 名义上存在，但关键嵌套字段不能给前端提供结构约束。
  - UI 会继续手写字段读取，policy outcome、matched rules、scope escalation target 的文案和行为容易漂移。
  - contract check 无法发现 `ScopeEscalationIntent` / `PolicyDecision` 字段改名或枚举变更造成的前端坏展示。
- 建议清理方向:
  - 在 `agentdash-contracts::permission` 增加 `ScopeEscalationIntentDto`、`PolicyDecisionDto`、`PolicyOutcomeDto`，`PermissionGrantResponse` 使用 typed DTO。
  - API 层做 domain -> contract 显式映射，不再用 `serde_json::to_value` 作为跨层 contract。
  - 前端 `PermissionGrantCard` 只消费 typed nested DTO；需要未知扩展时单独放 `diagnostics?: JsonValue`，不要把核心字段做成 `JsonValue`。

### 问题 4：Workflow capability editor 镜像后端 capability catalog / visibility rule

- 优先级: P1
- 问题类型: 重复事实源 / generated contracts 绕过 / 跨层耦合
- 证据路径:
  - `packages/app-web/src/features/workflow/ui/panels/shared.ts:99`
  - `packages/app-web/src/features/workflow/ui/panels/shared.ts:141`
  - `crates/agentdash-spi/src/platform/tool_capability.rs:73`
  - `crates/agentdash-spi/src/platform/tool_capability.rs:714`
  - `crates/agentdash-application/src/capability/tool_catalog.rs:1`
  - `crates/agentdash-api/src/routes/workflows.rs:1077`
  - `packages/app-web/src/types/workflow.ts:197`
- 具体代码证据:
  - 前端 `CAP_EDITOR_WELL_KNOWN_KEYS`、label、description 和 `AUTO_GRANTED_BASELINE` 手写维护（`shared.ts:99-148`），注释写明镜像后端 visibility rule。
  - 后端 SPI 拥有 well-known keys、cluster tools、`ToolDescriptor`、`default_visibility_rules` 权威实现（`tool_capability.rs:73-149`、`tool_capability.rs:165-177`、`tool_capability.rs:714-789`）。
  - 后端已有 tool catalog service，但 API 直接返回 `Vec<agentdash_spi::ToolDescriptor>`，不是 contract DTO（`tool_catalog.rs:1-15`、`workflows.rs:1077-1087`）。
  - 前端为该接口手写 `ToolDescriptor` / `ToolSource` union（`types/workflow.ts:197-211`）。
- 影响面:
  - 新增/调整 capability key、allowed scope、auto-grant 策略时，后端 runtime 和前端 editor 需要同步改多处。
  - `WorkflowTargetKind` 现在只有 `project | story`，但后端 `CapabilityScope` 已有 `Task`；前端 baseline 无法表达 task scope，一旦开放就会漂移。
  - tool catalog 虽来自后端，但 capability 可见性、标签、baseline 仍是前端本地事实源。
- 建议清理方向:
  - 将 capability catalog projection 纳入 `agentdash-contracts`，生成 `capability-catalog-contracts.ts`，至少包含 key、label、description、allowed_scopes、auto_granted、agent_can_grant、workflow_can_grant、tools。
  - `/tool-catalog` 不直接暴露 SPI 类型；由 API 显式映射为 contract DTO。
  - Workflow capability editor 只消费后端 catalog projection；前端可保留纯展示排序/折叠状态，但不再镜像 visibility rule。

### 问题 5：PermissionGrantCard scope label 已落后于 generated scope

- 优先级: P2
- 问题类型: UI 事实漂移 / 小型重复事实源
- 证据路径:
  - `crates/agentdash-contracts/src/permission.rs:5`
  - `crates/agentdash-domain/src/permission/value_objects.rs:8`
  - `packages/app-web/src/features/permission/PermissionGrantCard.tsx:143`
- 具体代码证据:
  - generated `PermissionGrantScopeDto` 是 `turn | agent_frame | activity`（`permission.rs:5-11`），domain `GrantScope` 同样是 `Turn | AgentFrame | Activity`（`value_objects.rs:8-14`）。
  - `PermissionGrantCard.scopeLabel` 却映射 `turn | session | workflow_step`，缺少 `agent_frame` 和 `activity`（`PermissionGrantCard.tsx:143-149`）。
- 影响面:
  - 当前 UI 会把 `agent_frame`、`activity` 原样展示，用户文案不一致。
  - 这是问题 1/3 的症状：scope 概念在 companion payload、spec、domain、contract、UI 多处手写。
- 建议清理方向:
  - 直接按 current generated enum 修正 UI label；同时把 companion `capability_grant_request.scope` 停止作为授权事实字段，或改为 broker 输入后映射到 `PermissionGrantScopeDto`。
  - 如果 scope 文案要长期稳定，可由 permission contract 增加 display metadata 或前端用 exhaustiveness check 管住 enum label。

### 问题 6：executor-selector discovery 仍是 API-local DTO + 前端手写模型

- 优先级: P2
- 问题类型: generated contracts 绕过 / 跨层 contract 漂移
- 证据路径:
  - `crates/agentdash-api/src/dto/discovery.rs:5`
  - `crates/agentdash-api/src/routes/discovery.rs:9`
  - `crates/agentdash-api/src/routes/discovered_options.rs:15`
  - `packages/app-web/src/features/executor-selector/model/types.ts:35`
  - `packages/app-web/src/features/executor-selector/model/useExecutorDiscovery.ts:42`
  - `packages/app-web/src/features/executor-selector/model/useExecutorDiscoveredOptions.ts:32`
- 具体代码证据:
  - `/agents/discovery` response 定义在 `agentdash-api/src/dto/discovery.rs`，不是 `agentdash-contracts`（`discovery.rs:5-26`）。
  - 前端 `DiscoveryResponse`、`ConnectorInfo`、`ExecutorInfo` 全部手写（`types.ts:5-39`），`useExecutorDiscovery` 直接 `await res.json() as DiscoveryResponse`（`useExecutorDiscovery.ts:34-44`）。
  - `/agents/discovered-options/stream` NDJSON envelope 为 `{ Ready } | { JsonPatch } | { finished } | { Error }`，由 API route 手写 JSON（`discovered_options.rs:15-21`、`discovered_options.rs:29-60`）。
  - 前端也手写 `ServerMessage` union，并以 `JsonPatch as Operation[]` 应用到本地状态（`useExecutorDiscoveredOptions.ts:32-36`、`useExecutorDiscoveredOptions.ts:118-145`）。
- 影响面:
  - model selector、provider/model capability、permission policy 选项继续扩展时，contract drift 只能靠运行时发现。
  - `PermissionPolicy = "AUTO" | "SUPERVISED" | "PLAN"` 是执行器启动策略，不等同于 PermissionGrant，但命名接近，且没有 generated contract 约束，容易被误并入权限系统。
- 建议清理方向:
  - 把 discovery response 和 discovered-options stream envelope 纳入 `agentdash-contracts` 或明确缩小为 connector-private 内部协议，不让 feature 长期手写跨层 DTO。
  - 保留 executor `permission_policy` 作为执行器配置字段；命名上避免和 PermissionGrant 混用，例如 UI 文案使用“执行器审批策略 / approval policy”。

### 问题 7：Session system event UI 直接持有过宽 BackboneEvent，platform/companion 解析下沉不足

- 优先级: P2
- 问题类型: 过厚 UI / 过宽 DTO 消费 / model-ui 边界偏移
- 证据路径:
  - `packages/app-web/src/features/session/ui/SessionEntry.tsx:158`
  - `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:22`
  - `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:190`
  - `packages/app-web/src/features/session/ui/SessionSystemEventCard.tsx:524`
  - `packages/app-web/src/features/session/ui/SessionCompanionRequestCard.tsx:21`
  - `packages/app-web/src/features/session/model/platformEvent.ts:31`
- 具体代码证据:
  - `SessionEntry` 按 `BackboneEvent.type` 分发，platform 事件直接把完整 event 传给 task/system card（`SessionEntry.tsx:158-170`）。
  - `SessionSystemEventCardProps` 接受完整 `BackboneEvent`，组件内部再提取 platform type/data/message（`SessionSystemEventCard.tsx:22-26`、`SessionSystemEventCard.tsx:190-199`）。
  - 同一组件维护大量 event label/default message 和 detail field 解析（`SessionSystemEventCard.tsx:76-122`、`SessionSystemEventCard.tsx:524-587`）。
  - `SessionCompanionRequestCard` 也接受完整 `BackboneEvent`，再调用 `parseCompanionRequest` 解析（`SessionCompanionRequestCard.tsx:21-27`）。
  - `platformEvent.ts` 已有 extraction helper，但输出仍是 `Record<string, unknown>`（`platformEvent.ts:31-62`）。
- 影响面:
  - UI 层承担协议分发、字段解析、文案和交互意图，尤其 companion capability grant 这种特殊 payload 会继续向 UI 扩散。
  - 新增 platform event 时容易直接在 UI 大组件里追加分支，而不是先收敛成 model view。
- 建议清理方向:
  - 不重写 session feed 主路径；`BackboneEvent` 作为 stream contract 保持。
  - 只在 model 层增加 `SessionSystemEventViewModel` / typed companion view model，把 platform event data 解析集中到 `features/session/model`。
  - UI props 改为窄 view model，`SessionCompanionRequestCard` 不再接收完整 `BackboneEvent`；permission grant 相关 payload 按问题 1 收敛到 generated permission DTO。

### 代码模式

- 标准 generated contract 链路已经存在于 permission：`agentdash-contracts/src/permission.rs:5-67` -> `packages/app-web/src/generated/permission-contracts.ts` -> `packages/app-web/src/services/permission.ts:1-30` -> `PermissionGrantCard.tsx:10-16`。
- 过宽 generated 字段模式：domain typed struct (`value_objects.rs:98-121`) -> API `serde_json::to_value` (`permission_grants.rs:65-73`) -> frontend `JsonValue` 字段读取 (`PermissionGrantCard.tsx:119-124`)。
- 双审批模式：formal grant 状态机 (`permission/service.rs:71-171`) 与 companion JSON payload registry/UI (`payload_types.rs:87-131`, `companionRequestViewModel.ts:42-119`) 并存，但 platform broker 明确未实现 (`companion/tools.rs:916-928`)。
- capability mirror 模式：后端 SPI `default_visibility_rules` (`tool_capability.rs:714-789`) 被前端 `AUTO_GRANTED_BASELINE` 手写镜像 (`shared.ts:141-148`)。
- route-local DTO 外溢模式：tool catalog route 返回 `agentdash_spi::ToolDescriptor` (`workflows.rs:1077-1087`)，前端在 `types/workflow.ts:197-211` 手写对应 union。

### External References

无。本次只读 review 基于本地代码、Trellis specs 和任务上下文，未使用外部网络资料。

### Related Specs

- `.trellis/spec/frontend/architecture.md`: 前端不创建第二套业务事实源；feature module model / ui 分离。
- `.trellis/spec/frontend/type-safety.md`: generated wire 单源；mapper 不重新声明后端 enum/string union。
- `.trellis/spec/frontend/state-management.md`: store 不保存后端事实的第二副本；AgentRun command 以后端 snapshot 为准。
- `.trellis/spec/frontend/workflow-activity-lifecycle.md`: AgentRun composer / mailbox / lifecycle projection 的 command owner 是 AgentRun workspace。
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`: 跨 feature 复用、前端消费或流式传输的 DTO 必须进入 contract crate。
- `.trellis/spec/cross-layer/backbone-protocol.md`: Session feed 主路径直接消费 `BackboneEvent` 是当前约定。
- `.trellis/spec/backend/permission/architecture.md`: Permission System 统一管理 Agent runtime capability grant 事实。
- `.trellis/spec/backend/permission/grant-lifecycle.md`: 描述了 grant lifecycle，但 scope/query 字段与当前实现存在漂移，见 Caveats。

### 明确不是问题或暂不建议动的边界

- AgentRun runtime composer command 主路径暂不建议重构：`AgentRunWorkspaceView.conversation.commands`、`ConversationCommandView.stale_guard`、`ConversationKeyboardMapView` 已由 generated `workflow-contracts` 提供，`useAgentRunWorkspaceCommands` 按 command id / stale guard 回传，符合 spec。
- `buildDraftSessionCommandState` 在 ProjectAgent draft 阶段合成 `start_draft` command 暂不算问题：draft 尚未有 `run_id + agent_id` workspace snapshot，前端需要一个本地启动意图；真正 runtime workspace 仍消费后端 snapshot。
- `useAgentRunWorkspaceState` 保存当前 `AgentRunWorkspaceView` 并用 key guard 清空旧 projection，暂不视为重复后端事实源；它是当前页面 projection cache。写入 `lifecycleStore.setAgent/setFrame` 可后续评估，但不是本轮优先风险。
- `BackboneEvent` 在 session model/reducer 主路径直接消费不是问题；Backbone spec 已规定这是统一事件流。需要收窄的是 platform/companion UI props，而不是另造整条 session feed DTO。
- executor-selector 的 `permissionPolicy` 是执行器/Codex 启动审批策略，不等同于 PermissionGrant；不建议把它合并进 permission grant system，只需改善命名和 contract 边界。

### Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 no active task；本文件按用户显式给出的任务路径和唯一允许写入路径生成，不依赖 session active pointer。
- 未发现 `packages/app-web/src/features/companion` 目录；companion 前端逻辑目前位于 `features/session`，后端协议位于 `agentdash-contracts/src/companion.rs`、`agentdash-application/src/companion/*` 和 `agentdash-domain/src/companion/skills/*`。
- `.trellis/spec/backend/permission/grant-lifecycle.md` 仍写 `session_id`、`turn/session/workflow_step` 等旧形态；当前代码、contract、migration 使用 `source_runtime_session_id`、`effect_frame_id`、`turn/agent_frame/activity`。本研究按当前代码事实判断架构问题，spec 漂移应由后续 spec update 单独处理。
- 未运行测试或 contract generation；本任务为只读架构 review，结论基于静态代码证据。

### 后续适合拆成任务的候选

1. 收敛 capability grant broker：删除临时 companion grant UI 分支，或实现 `target=platform capability_grant_request -> PermissionGrantService::request -> PermissionGrantResponse -> runtime capability delta` 的完整闭环。
2. 重做 permission grant list read model：支持 `effect_frame_id | run_id` + `status` 查询 pending/active/terminal grants，并让前端审批入口只消费 generated `PermissionGrantResponse`。
3. Typed permission nested DTO：为 `ScopeEscalationIntent`、`PolicyDecision`、`PolicyOutcome` 生成 contract，并清理前端 `JsonValue` 字段读取。
4. Capability catalog contract 化：将 well-known key、visibility rule、labels、tool descriptors 放入 `agentdash-contracts` projection，移除前端 `AUTO_GRANTED_BASELINE` 镜像。
5. executor discovery contract 化：把 `/agents/discovery` 和 `/agents/discovered-options/stream` 的 response/envelope 纳入 generated contracts，或明确降级为 connector-private internal API。
6. Session platform event view model 瘦身：把 `SessionSystemEventCard` 的 event type/data 解析迁移到 `features/session/model`，UI 只接收窄 view model。
