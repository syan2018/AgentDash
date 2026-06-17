# 模型上下文构成统计真实化设计

## Architecture Boundary

`GET /sessions/{id}/context/projection` 继续作为 Context panel 的模型可见上下文查询入口。该 route 返回 `SessionProjectionViewResponse`，仍由 `agentdash-contracts` 定义 wire DTO，并由 generated TypeScript 进入前端。

当前问题是 `AgentContextEnvelope` 只表达 message projection。修复方向是在后端 projection 层补充一个可序列化的 usage breakdown 输入面，而不是让前端从 runtime store、event stream 或 UI state 拼装构成。

## Data Model

参考 Claude Code 的 `/context` 设计，本任务应生成一个与真实 API 请求同口径的 `context_usage` snapshot，而不是只在 message projection 上补几个占位字段。`AgentContextEnvelope.messages` 继续表达 durable model-context transcript；非消息构成由 API assembly 层从 launch/runtime projection 汇总成 usage items，再与 message segments 一起生成 `SessionContextUsageAnalysisResponse`。

新增内部结构建议放在 contract 转换或 application session query 层，而不是强行让 compaction projection store 持久化所有非消息输入：

- `kind`: 主分类 key，对齐 `SessionContextUsageCategoryResponse.kind`
- `label`: 展示标签
- `token_estimate`: 后端估算结果
- `source`: 来源语义，例如 `context_frame`、`tool_schema`、`mcp_server`、`skill`、`local_estimate`
- `deferred`: 该片段是否为可用但未注入模型的延迟内容
- `detail`: 可选细项名称，例如 tool name、MCP server、skill name、memory file path、system prompt section

消息本体继续来自 `messages`，避免破坏 compaction projection、fork/rollback 和 segment provenance。

Claude Code 中 deferred 工具会展示为 `MCP tools (deferred)` / `System tools (deferred)`，但 `actualUsage` 计算时排除 `isDeferred`。AgentDashboard 的 category 可以继续保留同一 `kind` 并用 `deferred=true` 表达，前端计算总量和 free space 时不得把 deferred token 当作已占用上下文。

## Data Sources

`ContextProjector` 负责消息投影；非消息构成需要从当前 session 对应的 launch/runtime projection 读取。实现上应新增一个 session context usage builder，输入为：

- `AgentContextEnvelope`: durable compact-aware messages。
- latest launch/runtime context snapshot: system/context frames、MCP、assembled tools、skills/capabilities。
- runtime token usage: provider total、effective window、reserve/free space 只用于压力展示，不参与非消息分类估算。

优先使用当前可获得的稳定事实：

- `context_frames`: 按 `ContextFrameSection` 分类。
  - `Identity`、`AssignmentContext`、`SystemNotice` 中身份/任务类进入 `agents` 或 `system_developer`。
  - `UserPreferences`、`ProjectGuidelines`、`ContinuationContext` 进入 `memory`。
  - `ToolSchema`、`ToolSchemaDelta` 进入 `system_tools`。
  - `SkillDelta` 进入 `skills`。
  - 其它 hook/runtime 注入根据 section kind 归入 `system_developer` 或 `agents`。
- `ExecutionContext.session.mcp_servers`: 进入 `mcp_tools`，估算 server identity 与已知工具声明文本。
- `ExecutionContext.turn.assembled_tools`: 进入 `system_tools`，通过工具 name/description/schema 估算。

如果当前查询时没有 live prepared turn 或 frame runtime projection 可读，API 仍返回消息/附件/摘要构成，不为缺失数据继续伪造 `not_loaded`。分类存在且 token 为 0 应表示“当前没有该类模型输入”，不是“加载失败”。

Claude Code 的重要口径：

- `/context` 先执行与 query/API 调用一致的投影：compact boundary、context collapse、microcompact，再做统计。
- system prompt 与 system context 合并成 named entries 单独计数。
- built-in tools 与 MCP tools 先做 bulk 计数，再用本地比例估计分摊到细项。
- skill frontmatter 单独展示；不要把承载 skill/slash command 的工具 schema 和 skill 明细重复计入分类。
- 最终总量优先使用 provider API usage；分项估算用于解释构成。

## Contract Shape

`SessionContextUsageCategoryResponse` 保持当前字段：

- `kind`
- `label`
- `token_estimate`
- `source`
- `deferred`

新增 `SessionContextUsageItemResponse` 并挂到 `SessionContextUsageAnalysisResponse.items`：

- `kind`
- `label`
- `name`
- `token_estimate`
- `source`
- `deferred`

前端第一步仍可只展示聚合 category；items 用于测试、后续 hover/detail panel、Top MCP/Skills/Memory 明细，不需要另起非 contract 的前端模型。

Rust contract 是唯一事实源；更新后运行 contract generator，前端直接消费 generated type。

## Token Estimation

所有估算在后端完成：

- 文本片段使用 `text_tokens`。
- 工具 schema 使用 `estimate_tool_tokens` 或对 runtime tool schema 转成同等 `ToolDefinition` 后估算。若 bulk 估算与单项估算都可得，优先使用 bulk 总量，再按本地 schema 大小比例分摊，避免重复计算固定工具前缀开销。
- message / attachment / compaction summary 继续使用现有 `estimate_message_tokens` 和 content part 估算。

`projection.token_estimate` 应考虑新增非消息片段，使 header token estimate 与分类总量更接近真实 request estimate。runtime provider usage 仍可显示在 header 的“当前 x / window”。

若 provider usage 可用，header 的当前上下文压力继续来自 provider usage；`context_usage.categories` 的合计可以是后端估算，二者不必强行相等，但 source 必须说明 `provider` / `local_estimate`。

## Frontend

`SessionProjectionView` 保持从 `projection.context_usage.categories` 渲染构成。需要调整测试样例和少量展示文案：

- 不再把 system 类 sample 固定为 `not_loaded`。
- 对 `source` 展示真实来源，例如 `context_frame`、`tool_schema`、`mcp_server`、`skill`、`local_estimate`。
- `pending_estimate`、`reserve`、`free_space` 继续来自 `tokenUsage`，因为它们是 runtime pressure 语义，不是 projection segment 语义。
- deferred categories 可以显示 `deferred`，但前端视觉上应表达“可用但未加载”，不是错误态 `not_loaded`。

## Compatibility And Migration

项目未上线，不做旧字段兼容。若只扩展非持久化 envelope/contract，无数据库 migration。若实现发现需要持久化 latest prepared turn 的 usage snapshot，必须补对应 migration，并以当前正确 schema 为准。

## Risks

- 当前 `/context/projection` 是 repository-backed 查询，可能没有 live `ExecutionContext`。实现时需要找到 session 到 latest frame/runtime projection 的稳定读取路径，避免把内存态当事实源。
- Tool schema 可能同时来自 assembled tools 和 ContextFrame section，需去重，推荐按 tool name/source 去重。
- ContextFrame 的 section kind 与 UI 分类不是一一对应，需要集中映射并测试，避免后续新增 section 时静默掉入错误桶。
- 参考 Claude Code 的 API token counting 在本项目里不能直接依赖 provider count endpoint；本任务先做统一后端本地估算，保留 source 标记，后续再接 provider count API。

## Rollback

该任务主要改 DTO、projection assembly 与 UI 展示。回滚点是恢复 `AgentContextEnvelope` 与 `SessionProjectionViewResponse` 原 shape，并重新生成 TS contract。由于不做旧兼容，回滚必须同时撤回 Rust DTO 与 generated TS。
