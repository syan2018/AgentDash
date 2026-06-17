# 模型上下文构成统计真实化

## Goal

让 Session 的模型上下文面板展示真实可解释的上下文构成，而不是把非消息类目长期显示为 `not_loaded · deferred`。用户需要能从同一张 `CONTEXT` 卡片看出当前模型输入主要由哪些内容组成：系统/开发者上下文、工具 schema、MCP、Agent/身份帧、Memory/项目指引、Skills、消息、附件与压缩摘要。

## Confirmed Facts

- `/sessions/{id}/context/projection` 当前由 `ContextProjector` 构造 `AgentContextEnvelope`，再在 `agentdash-contracts::runtime::session` 转为 `SessionProjectionViewResponse`。
- `AgentContextEnvelope` 目前只携带 `messages`，不携带 system prompt、context frames、tool schema、MCP servers、skills 或 memory/project-guidelines 等非消息输入。
- `context_usage_analysis()` 只根据 projection segment 统计 `messages`、`attachments`、`compaction_summary`；`system_developer`、`system_tools`、`mcp_tools`、`agents`、`memory`、`skills` 被固定写成 `0 / not_loaded / deferred=true`。
- 前端 `SessionProjectionView` 只是展示 contract 返回的 `context_usage.categories`，不是造成 `not_loaded` 的根因。
- 既有 spec 要求 projection view 返回 `context_usage` 分析数据，用于解释模型当前可见内容构成；provider usage 仍是总量和窗口压力的权威来源。
- `references/claude-code` 的 `/context` 实现先把历史转换为真实 API 请求视图，再分析 context usage；它明确拆分 system prompt、system tools、MCP tools、custom agents、memory files、skills、messages、free space 和 compact buffer，并把 deferred tools 显示但不计入实际占用。

## Requirements

- `context_usage` 必须从后端 contract 层返回真实分项数据；前端不得手写或猜测这些跨层 DTO 字段。
- `AgentContextEnvelope` 或等价后端 projection DTO 必须能表达非消息上下文构成，至少覆盖当前 UI 已展示的主分类：
  - `system_developer`
  - `system_tools`
  - `mcp_tools`
  - `agents`
  - `memory`
  - `skills`
  - `messages`
  - `attachments`
  - `compaction_summary`
- 每个分类必须有清晰来源语义。真实估算使用 `local_estimate` 或更精确的来源标记；只有确实没有数据源的分类才允许为空，但不能继续把已知可投影内容标记为 `not_loaded`。
- token 估算必须复用后端统一 token estimation helper 或同等后端集中逻辑，避免前端重复估算。
- 统计口径必须接近“模型实际看到的请求”，不能只读 raw transcript；compaction projection、runtime context frame、工具可见性和 deferred/loaded 状态都要在后端统一收敛后再进入 `context_usage`。
- provider usage / runtime token usage 保持负责总量、窗口压力、pending estimate、reserve/free space；projection usage 负责解释构成，两者在 UI 上可以并列但语义必须清楚。
- 修改跨层 contract 后必须更新 generated TypeScript，并通过 drift check。
- 项目处于预研期，不做旧字段兼容或 UI 兼容分支；以正确模型为准。

## Acceptance Criteria

- [x] `/sessions/{id}/context/projection` 返回的 `context_usage.categories` 中，当前可获得的 system/context frame/tool/MCP/skill/memory 类目不再固定为 `not_loaded · deferred`。
- [x] `SessionProjectionView` 能展示真实分项 token 来源，并保留 `messages`、`attachments`、`compaction_summary` 明细。
- [x] 后端 contract 和 generated TypeScript 保持一致，`pnpm run contracts:check` 通过。
- [x] 后端 focused tests 覆盖非消息类目进入 `context_usage` 的分桶逻辑。
- [x] 前端 focused tests 覆盖真实分类 source 展示，不再把 sample projection 的 system 类目固定为 `not_loaded`。
- [x] 代码不引入兼容旧字段、双字段 mapper 或前端重复 token 估算。

## Out Of Scope

- 不要求 provider 逐项返回精确 token usage；本任务做后端统一估算与分类解释。
- 不重做压缩策略、fork/rollback 语义或 session timeline 展示。
- 不引入数据库 migration，除非实现过程中确认必须持久化新的 projection fact；优先使用当前 launch/runtime projection 可重建的事实。

## Open Questions

- 是否需要把 Memory 与项目指引严格拆分为两个 UI 类目？当前 UI 只有 `Memory`，设计先把用户偏好、项目指引、恢复/continuation 类上下文归入 memory/guidelines 类桶，并在技术设计中定义映射。
