# 收敛 ToolSchema 更新提示与 Responses 工具 schema 兼容性

## Goal

追踪并修复运行时 `tool_schema` 更新提示过度 verbose，以及 OpenAI/Codex Responses 工具 schema 无法解析的问题。

最终目标是让 Agent 在能力热更新后仍能获得足够完整、可行动的工具说明，同时避免把大段原始 JSON Schema 重复塞进模型文本，并确保传给 Responses API 的 `tools[]` schema 是有效且稳定可解析的。

## Confirmed Facts

- 运行时能力更新帧会在模型可见文本里渲染完整工具 schema：`crates/agentdash-application/src/session/dimension/tool_schema.rs`。
- 同一轮 LLM 请求也会通过 `BridgeRequest.tools` 把工具 schema 传给 provider，因此当前存在“API tools 字段 + 模型文本”双重 schema 注入。
- `upsert_lifecycle_tool` 的参数包含 `steps`、`edges`、`capability_config` 等嵌套结构；当前 schema 很容易膨胀。
- 当前 OpenAI/Codex 报错：
  `Invalid schema for function 'mcp_agentdash_workflow_tools_upsert_lifecycle_tool'. Please ensure it is a valid JSON Schema.`
- 当前 sanitizer 主要移除装饰字段、处理 nullable、转换 `oneOf`，但仍保留 `$defs` / `definitions`，且只内联部分组合器内的本地 `$ref`。
- `references/pi-mono` 的 Responses 工具转换保留 `strict` 配置入口；Codex Responses 路径使用 `{ strict: null }`。
- `references/codex` 的 Rust 工具 schema 使用受限 `JsonSchema` 类型，Responses tool 的 `strict` 是布尔值 `false`。

## Requirements

- 模型可见的工具更新提示必须继续包含完整工具说明所需的信息，不能简化到只剩工具名。
- 模型可见提示应从“原始完整 JSON Schema dump”收敛为“可调用说明”：
  - 工具名、用途、来源/能力路径。
  - 参数名、必填性、类型、简短说明。
  - 关键嵌套字段的结构化摘要。
  - 对复杂对象给出紧凑示例或字段清单，而不是整段 pretty JSON。
- API 请求里的 `tools[]` 必须继续携带机器可解析的完整参数 schema。
- `upsert_lifecycle_tool` 等平台 MCP 工具的 schema 必须通过 OpenAI/Codex Responses 兼容性约束。
- schema sanitizer 需要覆盖本地 `$ref`、`$defs` / `definitions`、组合器、nullable、`additionalProperties` 等实际失败点。
- `OpenAiCodexResponsesBridge` 的工具 `strict` 字段需要明确兼容策略，优先对齐 Codex Rust 的布尔 `false`，除非验证表明目标服务必须使用其它形态。
- 不做兼容性回退方案；项目未上线，直接把工具说明与 schema 管线调整到当前最正确状态。

## Acceptance Criteria

- [ ] 能力热更新后的模型可见 `tool_schema_delta` 文本仍足以指导模型正确调用新增/恢复工具。
- [ ] `tool_schema_delta` 不再输出完整 pretty JSON Schema dump；复杂工具提示文本明显短于当前实现。
- [ ] `BridgeRequest.tools` / Responses API 请求仍包含完整机器 schema。
- [ ] `mcp_agentdash_workflow_tools_upsert_lifecycle_tool` 的 schema 不再触发 OpenAI/Codex Responses `Invalid schema` 400。
- [ ] 新增或更新测试覆盖平台 MCP workflow 工具 schema 的 Responses 兼容性，尤其是 `upsert_lifecycle_tool`。
- [ ] 测试覆盖模型文本渲染，确认工具说明未丢失关键参数与必填信息。
- [ ] 相关代码与测试通过项目约定的 Rust 检查命令。

## Notes

- 该任务偏复杂：涉及 prompt 渲染、MCP schema sanitizer、Responses bridge 请求结构和测试。
- 参考路径：
  - `crates/agentdash-application/src/session/dimension/tool_schema.rs`
  - `crates/agentdash-spi/src/context/tool_schema_sanitizer.rs`
  - `crates/agentdash-agent/src/tools/schema.rs`
  - `crates/agentdash-mcp/src/servers/workflow.rs`
  - `crates/agentdash-executor/src/connectors/pi_agent/bridges/openai_codex_responses_bridge.rs`
  - `references/pi-mono/packages/ai/src/providers/openai-responses-shared.ts`
  - `references/pi-mono/packages/ai/src/providers/openai-codex-responses.ts`
  - `references/codex/codex-rs/tools/src/json_schema.rs`
  - `references/codex/codex-rs/tools/src/responses_api.rs`

## Open Questions

- 工具说明文本的目标长度是否需要硬性预算，例如每个新增工具最多 N 行，还是只做结构化摘要并用测试快照约束？
