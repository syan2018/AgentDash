# 实施计划：收敛 ToolSchema 更新提示与 Responses 工具 schema 兼容性

## Checklist

- [x] 读取相关规范与现有测试：tool capability pipeline、session runtime context、MCP workflow server。
- [x] 增加失败保护测试：
  - [x] `upsert_lifecycle_tool` schema 递归检查无 Responses 不兼容关键字。
  - [x] Codex Responses request body 中 tool `strict` 为布尔 `false`。
  - [x] `tool_schema_delta` rendered text 保留调用说明但不 dump 完整 schema。
- [x] 修正 schema sanitizer：
  - [x] 递归内联所有本地 `$ref`。
  - [x] 移除 `$defs` / `definitions`。
  - [x] 补齐 object / array 默认结构。
  - [x] 处理 nullable 与组合器，保持 OpenAI/Codex 可解析。
- [x] 收敛或同步 `agentdash-spi` 与 `agentdash-agent` 两处 sanitizer。
- [x] 调整 `tool_schema_delta` 模型文本渲染为紧凑工具说明。
- [x] 调整 `OpenAiCodexResponsesBridge` 的 `strict` 序列化策略。
- [x] 运行聚焦测试：
  - [x] `cargo test -p agentdash-mcp workflow`
  - [x] `cargo test -p agentdash-application tool_schema`
  - [x] `cargo test -p agentdash-executor openai_codex_responses_bridge`
  - [x] 必要时运行更大范围 Rust 测试。
- [x] 根据实现结果决定是否更新 `.trellis/spec/`，记录工具 schema / prompt 渲染约定。

## Review Gates

- 模型文本里不能丢失工具调用所需信息。
- API tools schema 不能依赖自然语言说明补洞。
- 不引入兼容性回退；直接修到当前目标 provider 的正确形态。
- 不改数据库 schema。

## Rollback Points

- `tool_schema_delta` 渲染调整可以独立回滚。
- `strict` 字段调整可以独立回滚。
- sanitizer 变更风险最高，必须由测试覆盖后再合并。
