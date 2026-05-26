# Companion 通用交互信道与能力扩展治理设计

## Goal

设计一期 Companion 扩展方案，让 Agent 可以通过统一的 companion 信道主动发起跨主体交互请求，包括问用户、请求平台 broker 裁决、派发其它 session / companion agent 协作，以及申请临时能力扩展。所有授权类请求最终汇入标准权限系统与 Capability runtime，避免权限事实藏在自然语言消息或工具提示里。

本任务产出规划与设计，不直接进入实现阶段。实现前需要明确 MVP 范围、数据模型与前端交互边界。

## Background

当前项目已有几块基础能力：

- `companion_request` / `companion_respond` 已存在，支持向 human、parent、sub 等目标发起协作并回流结果。
- `PayloadTypeRegistry` 已有 request / response type 注册与校验，内置 `task`、`review`、`approval`、`notification`、`completion`、`resolution`、`decision` 等类型。
- Capability 系统已有 `RuntimeCapabilityTransition`、dimension replay、`replace_current_capability_state`、`update_session_tools`、`tool_schema_delta` 等运行时能力更新链路。
- 平台 MCP 工具已通过 well-known capability key 映射，例如 `relay_management`、`story_management`、`task_management`、`workflow_management`。
- Embedded Skill Bundle 已有 `canvas-system` 实践，可用于承载平台内嵌 Agent-facing 操作手册。

讨论结论：

- Companion 应成为 Agent 主动跨主体交互的统一入口。
- Companion payload 表达意图，Permission / Grant request 负责授权裁决，CapabilityState 负责运行时事实。
- 能力扩展请求应通过 companion 自定义 payload 进入平台 broker，再转成标准权限请求，批准后由 capability dimension pipeline 应用。
- Companion 体系需要内嵌 `companion-system` skill，避免把复杂协议全部塞进工具描述。

## Requirements

### R1. Companion 作为通用主动交互信道

Companion 信道需要承载 Agent 主动发起的结构化请求，目标至少覆盖：

- `human`：请求当前用户审批、选择、补充信息或确认范围。
- `platform`：请求平台 broker 做 policy 判断、生成权限申请、触发平台动作。
- `parent`：向父 session 回流结果或请求主 session 决策。
- `sub` / `session`：向其它 companion agent 或 session 派发协作。

请求 envelope 需要稳定表达 `request_id`、来源 session、目标、payload type、payload、状态、创建时间和可追踪上下文。

### R2. 自定义 payload type 必须可注册、可校验、可渲染

Companion payload type 需要从当前轻量字段校验演进到更明确的注册模型：

- 每个 type 声明 request / response 角色。
- 每个 request type 可声明期望 response type。
- 每个 type 提供机器可校验 schema 或 typed validator。
- 每个 type 提供前端 `ui_hint` 或 renderer key。
- 未识别类型的处理策略需要在设计中明确，保证审计和诊断可理解。

### R3. 能力扩展请求走 companion payload，而非直接授予

新增 `capability_grant_request` 请求类型，用于表达 Agent 申请临时工具 / MCP 能力的意图。

最小语义：

- `requested_paths`：`ToolCapabilityPath[]`，例如 `workflow_management::upsert_lifecycle_tool`。
- `reason`：模型说明为什么需要该能力。
- `scope`：建议支持 `turn` / `session` / `workflow_step` 等生效范围。
- `ttl_seconds` 或 `expires_at`：临时授权的到期约束。
- `interaction_hint`：建议用户审批文案或风险说明。

对应 response type 可为 `capability_grant_result`，只作为交互回执；授权事实以权限 / grant record 为准。

### R4. Platform broker 把 companion payload 规约到权限系统

平台 broker 负责把 `capability_grant_request` 转成标准权限 / grant request：

- 校验 owner hard boundary。
- 校验 requested path 是否在可申请目录中。
- 计算需要用户审批、平台策略自动批准或拒绝的路径。
- 记录来源 session、tool call、请求理由、审批人、TTL、policy 命中原因。

### R5. 权限裁决后汇入 Capability runtime

批准后的能力变更必须生成 `RuntimeCapabilityTransition` 或等价的 dimension records，并通过现有 capability replay / projection 入口得到新的 `CapabilityState`。

生效后需要：

- 更新 `CapabilityState.tool.capabilities` / `tool_policy` / `mcp_servers`。
- 重新构建 runtime tools 与 MCP tools。
- 对支持 live update 的 connector 调用工具热更新。
- 发出 `capability_state_changed` 与 `tool_schema_delta`，让 Agent 看到新增工具。
- 支持撤销、过期和审计查询。

### R6. `companion-system` 内嵌 skill

新增平台内嵌 skill bundle，作为 Agent 使用 companion 信道的操作手册。

内容至少包括：

- Companion 的心智模型和目标选择规则。
- 通用 payload envelope。
- `capability_grant_request` / `capability_grant_result` 规范。
- human interaction 请求规范。
- cross-session dispatch 与 response adoption 规范。
- 授权类请求如何汇入权限系统与 capability runtime。

该 skill 应在 session 具备 `collaboration` capability 或具备能力申请入口时可见。

### R7. 前端需要统一展示请求、审批、回执与能力更新

前端需要能够展示：

- Companion 请求卡片。
- capability grant 审批卡片。
- 审批结果与回执。
- grant apply / fail / expire / revoke 状态。
- tool schema delta 与新增工具说明。

### R8. 审计、诊断和重放是设计输入

设计需要明确哪些事实进入 companion event，哪些事实进入 permission / grant record，哪些事实进入 capability transition。重放 session 或排查问题时，应能从结构化记录还原请求来源、审批决策、能力应用结果和最终工具面。

### R9. Companion 基础契约应做轻量收束

本期可以同步整理现有 companion request / response 的基础契约，但不要求重写执行模型：

- 将 `target=sub/parent/human/platform` 统一为明确路由枚举。
- 将现有 payload type 的 request / response / ui_hint 规则集中在 payload registry。
- 将 `companion_request` / `companion_respond` 的工具参数从 JSON string payload 收束为结构化 JSON object，减少 Agent 双重转义和格式错误。
- 缩短 `companion_request` / `companion_respond` 工具描述，把复杂使用规则迁移到 `companion-system` skill。
- 前端按 `payload_type` / `ui_hint` 选择卡片，而不是只按 `companion_human_request` 的固定 prompt/options 结构渲染。

### R10. Companion interaction 持久化单独追踪

本期不把所有 companion request / response 抽成独立持久化交互表。该方向单独创建后续任务跟踪，因为它需要重新讨论 session event、wait registry、pending action、human approval 与审计查询之间的事实源关系。

## Acceptance Criteria

- [ ] 明确 Companion 通用交互信道的职责边界：交互入口、协作编排、回执，不承担最终权限事实。
- [ ] 明确 `capability_grant_request` 从 companion payload 到 permission / grant record 的完整链路。
- [ ] 明确权限裁决如何转成 `RuntimeCapabilityTransition`，并复用现有 capability dimension pipeline。
- [ ] 明确 live apply 与 next-turn apply 的生效语义，以及对应的 context frame / tool schema delta。
- [ ] 明确 `PayloadTypeRegistry` 的演进方向，包括 schema、response type、UI hint 与诊断策略。
- [ ] 明确 `companion-system` embedded skill 的文件结构、注入条件和维护规则。
- [ ] 明确现有 companion payload 基础契约的轻量收束范围，避免工具提示继续膨胀。
- [ ] 明确 `payload: object` 的工具 schema、后端解析和前端回应链路。
- [ ] 明确前端卡片与状态展示的最小范围。
- [ ] 明确审计、TTL、撤销、过期和失败诊断要求。
- [ ] 形成 `design.md`，包含架构边界、数据流、数据模型草案、主要 trade-off。
- [ ] 形成 `implement.md`，拆分可执行阶段、验证命令、风险文件和回滚点。

## Out Of Scope

- 本任务不实现完整代码改动。
- 本期设计不负责 backend accessible root 扩展治理；该方向由 `05-17-backend-capability-expansion-governance` 覆盖。
- 本期设计不落地 companion interaction 独立持久化表；该方向由后续 tracking task 讨论。
- 本期设计不要求重做全部 companion UI，仅定义这条能力扩展链路需要的最小 UI 模型。
- 本期设计不把 `mcp.call_tool` 暴露成 Agent 可见万能工具；Agent 可调用面仍由已批准后的具体工具 schema 决定。

## Open Questions

无阻塞问题。`payload` 在本期直接收束为结构化 JSON object，不保留 JSON string 兼容输入。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
