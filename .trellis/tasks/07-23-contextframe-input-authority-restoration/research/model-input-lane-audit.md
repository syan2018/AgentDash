# Model Input Lane Audit

## Audit Question

当前分支中，哪些平台拥有的内容会进入内嵌 Dash Agent 的 provider request，却绕过 ContextFrame 单一渲染与热更新链路？

## Baseline Decision From Prior Work

历史任务 `06-20-context-frame-fact-domain-convergence` 与
`06-22-piagent-mcp-toolschema-contextframe-alignment` 已确认：

- ContextFrame `rendered_text` 是 Agent-visible PromptText；
- tool execution table 与 ToolSchema PromptText 同源但分责；
- provider `tools` 只承载机器 schema；
- ContextFrame 经 turn-start notice / transform-context进入 Agent消息；
- MCP 与 built-in tool 共用 Application-owned ToolSchema renderer；
- 初始工具面是 empty→current 的普通 delta；
- Agent loop 在每个 provider request 前刷新 tools/context。

这些合同曾有 focused backend/frontend tests通过，不是仅存在于设计文档中的设想。

## Commit Timeline

| Commit | Effect |
| --- | --- |
| `af21f9d7c` | 删除原 `agentdash-application-runtime-session`，包含 ContextFrame注入、transform-context和ToolSchema formatter |
| `1f1fdf0aa`…`8889bb3e3` | 一度恢复 Context Projection与main等价链路 |
| `af4b0d840` | 再次删除 context projection、tool schema dimension与golden tests |
| `4118d8770` | 把 ContextFrame定义为下游UI projection，工具文本缩成name+description并移除前端schema展开 |
| `fd733107e` | 在 Dash system prompt新增独立可读ToolSchema摘要 |

## Current Production Path

```text
Product RuntimeToolDefinition
  -> AgentSurfaceContributionPayload::Tool
  -> DashToolDefinition
  -> DashSurface::render_system_prompt()
       -> independent "Runtime Tool Schema"
  -> DashCoreContext (once per user turn)
  -> Core provider rounds clone static prompt/tools
  -> BridgeRequest
  -> vendor structured tools

SurfaceApplied history
  -> Native canonical_projection
  -> rebuild ToolSchemaDelta ContextFrame
  -> shallow rendered_text
  -> frontend
```

## Findings

| Lane | Evidence | Classification | Task target |
| --- | --- | --- | --- |
| readable ToolSchema | `dash/history.rs` owns `render_tool_schema_notice`; canonical projection owns a second shallow renderer | Confirmed bypass | one ContextFrame renderer |
| structured provider tools | bridge maps exact schema to `ToolDefinition`; provider adapters use native tool fields | Required machine contract | retain, add no-text guard |
| surface instructions | Dash concatenates instruction text; adapter later creates frames | Confirmed bypass | accepted frames drive prompt |
| intrinsic identity | Native adapter inserts intrinsic instruction before Product surface | Confirmed platform context | accepted Identity frame |
| workspace/context requirement | Native adapter formats workspace/constraint strings | Confirmed adapter renderer | move to context materializer |
| initial context | Dash adds title wrapper; frame publishes raw payload | Confirmed text mismatch | one accepted frame text |
| runtime compaction | Dash appends `<compacted_context>` directly | Confirmed bypass | CompactionSummary frame |
| hot surface update | `materialize_context()` runs once; Core clones context each round | Confirmed stale snapshot | per-round refresh |
| ContextFrame delivery metadata | protocol declares phase/order/cache/model channel/consumption; Dash ignores them | Confirmed declaration-only contract | executable materializer |
| frontend ToolSchema | structured schema exists but renderer only shows count/description | Confirmed inspectability regression | restore schema expansion |
| user input/steer | native conversation input | Not a bypass | exclude |
| assistant/tool history | native conversation and execution evidence | Not a bypass | exclude |
| naming/compaction summarizer job | internal provider operation, not main Agent context | Different lane | exclude |
| external Complete Agents | agent-owned native prompt/history may be opaque | Needs contract audit | guard platform-owned delivery only |

## Provider Audit

Repository search found no provider-layer readable ToolSchema formatter. Anthropic/OpenAI adapters consume
`BridgeRequest.system_prompt/messages/tools` and serialize native structured tools. The only current
`Runtime Tool Schema` PromptText renderer is in Dash history.

Therefore the phrase “provider内嵌工具说明”对应的实际故障点位于 Dash prompt materialization，而不是
vendor bridge；provider bridges仍需回归守卫，防止以后增加隐式PromptText。

## Main-Reference Comparison

Main-reference has the missing architecture pieces:

- `session/tool_assembly.rs` 同源生成 callable tools 与 `RuntimeToolSchemaEntry`；
- `session/dimension/tool_schema.rs` 渲染 capability/source/path/description/required/type/enum/nested fields；
- `session/context_frame.rs` 把 `rendered_text` 入队为 turn-start notice；
- `session/hook_delegate.rs` 把 notice变成steering message；
- `agent_loop/streaming.rs` 在每次 assistant/provider response 前刷新 tools并调用 transform-context；
- frontend ToolSchema item可展开 structured JSON。

当前代码库仍保留旧 Agent loop中的 per-request refresh/transform-context实现，可作为行为参考；Native
Dash Core的新架构需要按 concrete Agent authority重新接线，而不是复制旧session owner。

## Relationship To Active Persistence Task

`07-20-agent-runtime-persistence-authority-convergence` 正确规定 concrete Complete Agent拥有native
history/context，但其后续新增的“Dash system prompt可读tool摘要、ContextFrame只做展示”合同与更早的
ContextFrame input authority相冲突。

本任务不改变持久化owner，只把同一Dash owner document中的accepted context改为：

- exact ContextFrame负责可读输入；
- exact tool definitions负责机器调用；
- history/read/live/frontend从已接纳的同一对象派生。

## Test Evidence

以下 focused tests在诊断阶段均通过，证明当前分叉行为是被测试固化的现状：

- `accepted_tool_description_and_nested_schema_reach_the_provider_without_reconstruction`
- `surface_projection_reports_tool_changes_instead_of_replaying_full_schema_snapshots`
- `accepted_tools_render_one_model_readable_schema_notice_from_their_exact_definitions`

目标实现需要重写这些断言，使其证明 ContextFrame单一渲染、provider structured tools原样传递与下一
provider round热更新。
