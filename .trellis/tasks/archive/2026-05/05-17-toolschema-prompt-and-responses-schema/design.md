# 技术设计：收敛 ToolSchema 更新提示与 Responses 工具 schema 兼容性

## 边界

本任务分成两条互相配合但职责不同的链路：

- 模型可见工具说明：由 runtime context frame / `tool_schema_delta` 渲染，目标是让模型知道新增工具怎么用。
- Provider 工具 schema：由 `BridgeRequest.tools` 进入 OpenAI/Codex Responses `tools[]`，目标是给模型服务提供机器可解析的完整 JSON Schema。

两条链路不能互相替代。模型文本保留可调用说明，但不再直接 dump 完整 pretty JSON Schema；API tools 保留完整 schema，并保证合法。

## 模型可见工具说明

`crates/agentdash-application/src/session/dimension/tool_schema.rs` 当前把 `parameters_schema` pretty JSON 完整渲染进文本。建议改为 schema 摘要渲染：

- 顶层：工具名、description、capability、source、tool path。
- 参数摘要：字段名、required / optional、类型、description。
- 嵌套对象：递归展开到有限深度，超出深度显示 `{ ... }` 或 “包含嵌套字段”。
- 数组：显示 item 类型摘要，例如 `steps: array<StepInput-like object>`。
- 组合器 / nullable：显示为 `string | null`、`one of (...)` 等简洁形式。
- 保留少量关键示例字段，但不输出完整 `$defs`、`properties` 原文。

结构化 `ContextFrameSection::ToolSchemaDelta` 可以继续保存完整 `parameters_schema` 供 UI 展示；只调整模型可见 `rendered_text`。

## Responses Schema 兼容

当前 sanitizer 位于：

- `crates/agentdash-spi/src/context/tool_schema_sanitizer.rs`
- `crates/agentdash-agent/src/tools/schema.rs`

两份逻辑几乎重复，需要避免长期漂移。实现时优先收敛为单一权威 sanitizer，或至少保持测试覆盖两边一致行为。

兼容目标：

- 递归解析所有本地 `$ref`，不仅是 `anyOf/allOf/oneOf` 直接元素。
- 内联后移除 `$defs` / `definitions`，避免 Responses schema validator 不接受。
- 移除或转换不在目标子集里的关键字：`$schema`、`title`、`default`、`format`、`examples`、`readOnly`、`writeOnly`、`deprecated` 等。
- 对 object 明确 `type: object`、`properties`、`required`、`additionalProperties: false`。
- 对可选字段用 nullable union 或 `anyOf` 表达，但保证目标 provider 接受。
- 对 `serde_json::Value` 这类自由 JSON 字段，需要定义明确策略：允许宽松 object，或用更具体的输入 DTO 替代。

## Codex Responses Strict 字段

`OpenAiCodexResponsesBridge` 当前写入 `"strict": null`。参考 `references/codex`，Rust Codex 的 `ResponsesApiTool.strict` 是布尔 `false`。建议先改为 `false`，并通过请求体构建测试锁定。

如果后续发现 ChatGPT Codex backend 必须使用 `null`，需要在 provider 配置里显式区分，而不是无条件写死。

## 测试策略

- 添加 schema 兼容性断言：递归扫描 `upsert_lifecycle_tool` schema，不允许残留 `$ref`、`$defs`、`definitions`、`oneOf`、`allOf`、`default`、`format` 等。
- 添加 `tool_schema_delta` 渲染测试：确认文本包含工具用途、参数、必填信息，但不包含完整 JSON code fence 或大段 pretty schema。
- 添加 Codex Responses request body 测试：确认 function tool 的 `strict` 是布尔 `false`，且 schema 位于 `parameters`。
- 优先用现有单元测试覆盖；如有条件，再补最小集成测试模拟 `upsert_lifecycle_tool` 的实际 schema 进入 bridge。

## 风险

- 过度压缩工具说明会降低模型调用成功率，所以摘要渲染必须围绕“调用所需信息”而不是“视觉简洁”。
- `CapabilityConfig` / mount 指令等领域类型嵌套很深，schema 内联可能显著增大 API tools payload；必要时应考虑为 MCP 输入 DTO 定义更窄、更面向 agent 的输入结构。
- 两份 sanitizer 重复存在，修一份漏一份会继续引发不一致。
