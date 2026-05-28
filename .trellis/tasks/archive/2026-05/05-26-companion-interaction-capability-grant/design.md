# Companion 通用交互信道与能力扩展治理设计

## 目标边界

本设计把 Companion 定义为 Agent 主动跨主体交互的统一入口。它可以承载问用户、请求平台 broker、派发其它 session 协作和能力扩展申请，但最终授权事实仍由 permission / grant record 与 capability runtime 管理。

核心分层：

```text
Agent 意图
  -> companion_request(payload)
  -> Companion interaction bus
  -> Platform / Human / Session handler
  -> Permission / Grant decision
  -> RuntimeCapabilityTransition
  -> CapabilityState replay
  -> Tool hot update + ContextFrame notice
```

## 架构职责

### Companion Layer

Companion layer 负责交互编排：

- 接收 Agent 发起的结构化请求。
- 根据 target 路由到 human、platform broker、parent 或 sub/session。
- 保存 request / response / status 的交互事件。
- 向 Agent 回传回执、结果摘要或 pending 状态。

Companion event 适合记录“谁问了什么、目标是谁、对话状态如何”。它不是权限事实源。

### Payload Registry

Payload registry 负责 payload type 的机器契约：

- type 名称与 request / response 角色。
- 必填字段与 JSON Schema / typed validator。
- expected response type。
- UI renderer hint。
- 文本提示生成，用于 skill 或 context frame 里的简短约束。

当前 `PayloadTypeRegistry` 已有轻量定义，本期建议扩展为更强的 typed payload contract。

### Platform Broker

Platform broker 是 companion target 的一种，负责把平台类请求翻译为系统命令或权限请求。

对 `capability_grant_request`，broker 执行：

- 解析 `requested_paths` 为 `ToolCapabilityPath`。
- 校验 owner type 与 capability visibility hard boundary。
- 查询可申请工具目录。
- 生成 permission / grant request。
- 将审批结果转成 capability transition apply request。

### Permission / Grant Layer

Permission / Grant layer 负责权威裁决：

- 状态机：`created`、`pending_policy`、`pending_user_approval`、`approved`、`rejected`、`applied`、`failed`、`expired`、`revoked`。
- 记录 source session、request tool call、requested paths、reason、scope、TTL、审批人、policy decision。
- 提供撤销和过期判断。

读取侧只把 `applied` 且未过期 / 未撤销的 grant 视为运行时可消费事实。

### Capability Runtime

Capability runtime 负责实际生效：

- 从 approved grant 编译 `RuntimeCapabilityTransition`。
- 通过 dimension registry replay 到新的 `CapabilityState`。
- 更新 tool capabilities、tool policy 和 MCP server set。
- 触发 `replace_current_capability_state`。
- 对 connector 做 live tool replacement 或记录 next-turn apply。
- 发出 `capability_state_changed`、`tool_schema_delta`、必要的 companion result。

## 数据流：能力扩展请求

### 1. Agent 发起请求

Agent 调用 `companion_request`：

```json
{
  "target": "platform",
  "payload": {
    "type": "capability_grant_request",
    "requested_paths": ["workflow_management::upsert_lifecycle_tool"],
    "reason": "需要更新当前 Project 的 lifecycle 定义",
    "scope": "session",
    "ttl_seconds": 3600
  }
}
```

### 2. Companion 校验并路由

Companion tool 使用 payload registry 校验 request type。校验通过后生成 interaction record，并路由给 platform broker。

### 3. Broker 生成 grant request

Broker 读取当前 session owner、CapabilityState、tool catalog、visibility rules，生成 grant request。

建议最小字段：

```text
id
source_session_id
source_turn_id
source_tool_call_id
requested_paths
reason
scope
expires_at
status
policy_decision
approved_by_user_id
created_at
updated_at
```

### 4. 用户或策略裁决

用户批准后，grant request 进入 `approved`。策略可自动拒绝或自动批准部分低风险请求。裁决结果写入 grant record，不写回 companion payload 作为权威事实。

### 5. 编译 capability transition

Approved grant 编译成 declaration / effect records：

- tool declaration：`dimension=tool`、`declaration_type=capability_directive`
- MCP server set effect：需要新增平台 MCP 或外部 MCP 时更新 server set
- tool policy：工具级 include / exclude 进入 `CapabilityState.tool_policy`

### 6. Runtime apply

Live apply 路径：

```text
apply_runtime_capability_transition
  -> replace_current_capability_state
  -> build_tools_for_execution_context
  -> connector.update_session_tools
  -> emit capability_state_changed
  -> emit tool_schema_delta
```

Next-turn apply 路径：

```text
persist pending grant transition
  -> next LaunchPlan replay
  -> PreparedTurn includes updated CapabilityState
  -> initial capability context frame includes added tools
```

### 7. 回执

Companion 可回传：

```json
{
  "type": "capability_grant_result",
  "status": "applied",
  "grant_id": "...",
  "granted_paths": ["workflow_management::upsert_lifecycle_tool"]
}
```

该回执服务对话连续性；权限事实以 grant record 和 applied capability transition 为准。

## Companion 基础契约收束

现有 companion 代码已经具备大部分基础：

- `CompanionRequestTarget` 已有 `sub`、`parent`、`human`。
- `PayloadTypeRegistry` 已集中校验 request / response type、required fields、expected response type 与 `ui_hint`。
- `execute_human_request` 已保存 `payload_type`，等待回应时也把 request type 写入 wait registry。
- `companion_respond` 已在 pending action / parent completion 路径上复用 registry 校验 response type。
- 前端已有 `SessionCompanionRequestCard`，但目前主要消费 `prompt` / `options` / `wait`。

因此基础重构可以保持轻量：

1. 在 target enum 增加 `platform`，路由到 platform broker。
2. 为 payload registry 增加 `capability_grant_request` / `capability_grant_result`，并明确所有内置 type 的 ui hint。
3. 将工具参数从 `payload: string` 调整为结构化 `payload: object`，后端不再要求 Agent 提供二次 JSON 编码字符串。
4. 把工具描述里的长示例迁入 `companion-system` skill，工具描述只保留入口说明。
5. 前端把 companion request card 拆出 renderer 分发，根据 `payload_type` 或 `ui_hint` 渲染 approval / notification / capability grant。

范围会明显变大的只有两件事：

- 为所有 companion request 建独立持久 interaction table，而不是继续依赖 session event + wait registry。
- 为跨 session / workflow approval 建统一 interaction query API。

持久化方向单独建任务追踪，不阻塞本期能力申请链路。

### Payload Object Contract

建议调整：

```rust
pub struct CompanionRequestParams {
    pub target: CompanionRequestTarget,
    #[serde(default)]
    pub wait: bool,
    pub payload: serde_json::Value,
}

pub struct CompanionRespondParams {
    pub request_id: String,
    pub payload: serde_json::Value,
}
```

解析入口改为直接校验 object：

- `payload` 必须是 JSON object。
- `payload.type` 缺失时仍允许通用自由 payload，但 registry 不做 type-specific 校验。
- 已知 type 继续做 request / response role 与 required fields 校验。
- 前端 `respondCompanionRequest` 已经传 object，无需二次编码。

预研期可以直接把 string payload 下线，避免形成双形态。

## `companion-system` Embedded Skill

建议 bundle：

```text
skills/companion-system/
  SKILL.md
  references/
    payload-envelope.md
    capability-grant-request.md
    human-interaction.md
    cross-session-dispatch.md
    response-adoption.md
```

`SKILL.md` 保持短文档，说明：

- 什么场景使用 companion。
- target 选择规则。
- payload.type 与 request / response 角色。
- 授权类请求的事实源边界。
- 收到回执后的处理方式。

reference 文件承载具体 payload 示例和字段说明。

注入策略建议：

- session 具备 `collaboration` capability 时注入。
- session 暴露 capability request 入口时注入。
- companion sub-session 继承必要的 companion skill baseline，避免回流协议丢失。

## 前端交互

最小 UI 模型：

- Companion request card：展示 payload type、目标、reason、状态。
- Capability grant approval card：展示 requested paths、风险说明、TTL、批准 / 拒绝。
- Result card：展示 applied / rejected / failed / expired / revoked。
- ContextFrame stream：继续展示 capability delta 与 tool schema delta。

前端 renderer 应消费 payload registry 的 `ui_hint`，让新增 payload type 有明确扩展点。

## 主要取舍

### Companion 只做交互，不做授权事实源

这样可以复用现有 companion 信道的灵活性，同时让审计、撤销、重放仍落在 permission / grant 与 capability runtime 上。

### Agent 可见目录与可调用工具分离

未授权工具可以通过 catalog / skill 被 Agent 知道，但不进入 provider tools。批准后通过 tool hot update 暴露具体 schema。

### 内嵌 skill 承担操作手册

工具 schema 保持短小，复杂决策规则进入 `companion-system` skill。这样能降低工具提示膨胀，也让协议说明随平台版本受管同步。

### Interaction 持久化另行设计

当前 wait registry 与 session event 已能支撑本期人类回应和结果可视化。独立 interaction table 会牵涉更广的事实源统一，应单独设计，避免把本期 capability grant 链路拖成持久化平台重构。

## 关联既有设计

- `.trellis/spec/backend/capability/architecture.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/capability/capability-dimension-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/embedded-skill-bundles.md`
- `.trellis/tasks/05-17-backend-capability-expansion-governance`
- `.trellis/tasks/04-12-plugin-extension-api`
