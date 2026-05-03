# Connector 纯桥接层收尾——MCP/Tool 构建彻底上提 + 残留清理

## 背景

上一轮重构（S1-S5）将 System Prompt 组装和文件发现逻辑从 Connector 上提到了 Application 层。但 MCP 工具发现、RuntimeTool 构建、prompt 渲染辅助函数等仍残留在 Connector 内部，导致：

1. **MCP/Tool 构建仍在 Connector**——pipeline 通过 `connector.build_session_tools()` 间接调用，Connector 持有 `runtime_tool_provider` + `mcp_relay_provider`，知道 MCP 分流/relay 等概念
2. **Tools Section 双重渲染 (bug)**——assembler 已完整渲染 `## Available Tools`，但 connector 的 `augment_assembled_prompt` 又追加一份 `build_tools_section`
3. **大量辅助函数重复**——`describe_mount`、`is_platform_mcp_server`、`extract_mcp_server_name` 在 connector 和 assembler 各一份
4. **回退分支不该存在**——`prompt()` 内 "assembled_tools 为空则自行 build" / "assembled_system_prompt 为空则自行渲染" 的回退，破坏了单一入口原则

## 目标

**Connector 彻底变成纯桥接层**：收到 `assembled_system_prompt` + `assembled_tools` → 设给 Agent → 桥接事件流。不再持有任何 MCP/Tool/Prompt 相关逻辑。

## 任务拆分

### T1: 工具构建从 Connector 上提到 SessionHub / pipeline

- pipeline 直接持有 `RuntimeToolProvider` + `McpRelayProvider`，自行完成 MCP 分流/发现/工具构建
- 删除 `AgentConnector::build_session_tools` trait 方法及所有实现（PiAgentConnector、CompositeConnector）
- `PiAgentConnector` 删除 `runtime_tool_provider` / `mcp_relay_provider` 字段及 setter
- `PiAgentConnector` 删除 `partition_mcp_servers` 方法
- `app_state.rs` 把 `runtime_tool_provider` / `mcp_relay_provider` 注入改到 SessionHub

### T2: MCP 热更新上提到 hub 层

- `AgentConnector::update_session_mcp_servers` 签名改为接收已构建的 `Vec<DynAgentTool>`（只做纯 set）
- Hub 的 `update_session_mcp_servers` 自行做 MCP 发现后传入已构建工具
- Connector 内的 MCP 发现逻辑全部删除

### T3: 删除 Connector 内 prompt 渲染残留

- 删 `augment_assembled_prompt` / `build_tools_section` / `build_runtime_system_prompt`
- 删 `describe_mount` / `extract_mcp_server_name` / `is_platform_mcp_server`（assembler 已有）
- `prompt()` 直接使用 `context.assembled_system_prompt` + `context.assembled_tools`，无回退
- `PiAgentSessionRuntime` 统一为单一 `tools` 字段

### T4: ExecutionContext 继续瘦身

- 删 `mcp_servers`（assembler 消费完不穿透）
- 删 `relay_mcp_server_names`（同上）
- 评估 `vfs` 是否仍需穿透（如果只有已删的渲染逻辑在用则一并删除）

### T5: 测试适配

- 测试改为直接提供 `assembled_system_prompt` + `assembled_tools`
- 删除 `EmptyRuntimeToolProvider` 等不再需要的 mock

## 执行顺序

T1 → T2 → T3 → T4 → T5（T3-T5 高度并行）

## 完成状态

- [x] T1: 工具构建从 Connector 上提到 SessionHub / pipeline
- [x] T2: MCP 热更新上提到 hub 层
- [x] T3: 删除 Connector 内 prompt 渲染残留
- [x] T4: ExecutionContext 瘦身评估 — `mcp_servers`/`relay_mcp_server_names` 仍被 companion/relay 消费，保留；但 connector 不再触碰
- [x] T5: 测试适配

## 预期结果

- `connector.rs` 不再有任何 MCP/Tool/Prompt 渲染代码 ✓
- `PiAgentConnector` struct 只剩 bridge、providers、agents、system_prompt、user_preferences ✓
- ExecutionContext 字段：`mcp_servers`/`relay_mcp_server_names` 保留（companion/relay 需要），其余清理完毕 ✓
- 工具描述不再双重渲染 ✓