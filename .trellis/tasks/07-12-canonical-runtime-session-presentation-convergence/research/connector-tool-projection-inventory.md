# Connector 与 Tool Protocol Projection 初始审计

## 已确认退化点

- `agentdash-integration-codex/src/mapping.rs::item_content`只识别user/agent/reasoning/plan/dynamic/MCP少数类型，unknown item走`AgentMessage { text: item.to_string() }`。
- `agentdash-integration-native-agent/src/tool.rs`把工具统一投影为`RuntimeItemContent::ToolCall`，terminal经generic Tool Broker result收敛。
- `agentdash-agent-runtime/src/tool_broker.rs`当前conversation journal只保存tool name、arguments和generic JSON output，无法区分command/file/MCP/fs/Companion等presentation family。
- `agentdash-integration-remote-runtime`是Runtime Wire proxy，应保持typed event原样穿透，只替换本地placement/generation坐标。
- `AgentToolResult`当前包含`content/is_error/details: JsonValue`；`details.kind`承载多个产品工具的结构信息，但没有owner-declared conversation projector。

## 旧行为基线

使用以下Git对象做行为oracle：

```powershell
git show af21f9d7c^:crates/agentdash-executor/src/connectors/pi_agent/stream_mapper.rs
git show af21f9d7c^:crates/agentdash-executor/src/connectors/codex_bridge.rs
```

旧Native mapper明确投影：Agent message/reasoning delta与final、item started/updated/completed、command output delta、shell exec、fs read/grep/glob、context compaction、usage、approval与typed error/platform facts。

## 必须完成的动态Inventory

W3执行时不得只使用静态名单。必须从最终Business Surface/Tool Catalog枚举所有`ToolContribution`，为每项记录：owner crate、runtime name、capability key、tool path、allowed channel、projector family、call/update/result fixture、frontend renderer。任何缺失projector的contribution在surface compile阶段失败。

## 初始Family清单

- Codex standard ThreadItem families
- Native Agent message/reasoning/provider events
- command/shell
- file write/edit/apply patch
- fs read/grep/glob
- MCP
- explicit dynamic tool
- Workspace Module/Canvas/VFS
- Companion/collaboration
- Task/Wait/Lifecycle product tools
- context compaction、usage、error、approval/user input
- Remote Runtime/Relay typed pass-through

该清单是起点，不替代运行时catalog inventory。
