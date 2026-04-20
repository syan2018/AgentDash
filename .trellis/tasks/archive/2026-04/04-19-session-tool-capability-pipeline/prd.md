# Session 工具能力管线收口

## Goal

建立显式的 capability 声明模型，消除工具注入的硬编码路径，统一 visibility 治理规则。
让每个 session 的工具集完全由声明式 capability 驱动，而非散落在各处的条件分支。

## Background

### 当前问题

1. **MCP 注入缺乏声明式条件**
   - `StoryMcpServer` 有完整实现和 HTTP 端点（`/mcp/story/{id}`），但没有被任何 session 创建流程显式注入
   - `WorkflowMcpServer` 同理 — `McpInjectionConfig::for_workflow()` 已存在但无调用方
   - Task session 仅硬编码注入 `for_task()`，Project Agent 仅注入 `for_relay()` + preset

2. **FlowCapabilities 硬编码**
   - Project Agent 在 `routine/executor.rs` 中硬编码 `[Read, Write, Execute, Collaboration, Canvas]`
   - 没有 capability 声明从 Workflow/Lifecycle 层向下传递的机制

3. **两套执行路径差异**
   - Pi Agent (内部)：`RuntimeToolProvider.build_tools()` + MCP discover
   - 外部执行器 (Claude Code 等)：仅 MCP server (ACP 协议注入)
   - 统一管线需要兼顾两种路径

### 设计原则

- 内置工具集（ToolCluster）与扩展工具集（MCP）保持两套机制，但用统一的 capability 声明治理
- Capability 声明位于 Workflow/Lifecycle step 级，与 Agent config 交集裁剪
- 能力说明通过 hook 管线注入（初始化/变化时），而非硬编码传递描述

### 治理边界

`ToolCapability` 是**开放的 string key**，不是封闭枚举。它分为两类：

1. **平台 well-known 能力**（以固定 key 标识，映射到 ToolCluster 和/或平台 MCP Server）
   - `file_system`, `canvas`, `workflow`, `collaboration`（→ ToolCluster）
   - `story_management`, `task_management`, `relay_management`, `workflow_management`（→ 平台 MCP）
   - 这些能力的可见性由 CapabilityResolver 根据 visibility 规则决定

2. **用户自定义 MCP 能力**（以 `mcp:<name>` 前缀标识）
   - 引用已在 agent config 或 project 级别注册的外部 MCP server
   - Resolver 按 name 查找已注册配置并注入
   - 可在 workflow step 中声明，实现 step 级别的自定义工具动态注入

设计意图：**不穷举 Agent 可用工具**。平台 well-known key 治理平台侧内置能力；
`mcp:*` key 让用户能在 step 级别声明需要哪些自定义工具，resolver 从已注册配置中查找并注入。
未来如需扩展（如 `plugin:*`），只需增加新前缀和对应的解析逻辑。

## ToolCapability 协议设计

### 类型定义

`ToolCapability` = `String`（开放 key，非封闭枚举）

约定两类 key：
- **平台 well-known key**：固定字符串，如 `file_system`、`workflow_management`
- **用户自定义 MCP key**：`mcp:<server_name>` 格式，引用已注册的 MCP server 配置

### 平台 well-known 能力映射

| Key | ToolCluster(s) | 平台 MCP Scope | 典型消费者 |
|---|---|---|---|
| `file_system` | Read, Write, Execute | — | 所有需要访问文件的 session |
| `canvas` | Canvas | — | Project Agent |
| `workflow` | Workflow | — | 绑定 lifecycle 的 session |
| `collaboration` | Collaboration | — | 需要 companion 协作的 session |
| `story_management` | — | Story | 编排 Agent (PlanAgent) |
| `task_management` | — | Task | Task 执行 Agent |
| `relay_management` | — | Relay | Project Agent |
| `workflow_management` | — | Workflow | 拥有工作流管理能力的 Agent |

### 用户自定义 MCP 能力

格式：`mcp:<server_name>`

Resolver 行为：
1. 提取 `<server_name>` 部分
2. 在 agent config 的 `mcp_servers` 中查找同名 MCP server 配置
3. 找到 → 注入该 MCP server；未找到 → 警告日志，跳过

示例：step 声明 `["file_system", "workflow_management", "mcp:code_analyzer"]`
- `file_system` → 启用 ToolCluster::Read + Write + Execute
- `workflow_management` → 注入平台 WorkflowMcpServer
- `mcp:code_analyzer` → 从 agent config 查找名为 `code_analyzer` 的 MCP server 并注入

### Visibility Rule（仅适用于平台 well-known 能力）

```
CapabilityVisibilityRule:
  key: String                                           # well-known capability key
  conditions:
    - session_owner_types: [project | story | task]     # 哪些 owner 类型可见
    - requires_agent_declaration: bool                   # 是否需要 agent config 显式声明
    - requires_workflow_declaration: bool                 # 是否需要 workflow contract 声明
```

默认 Visibility 矩阵：

| Key | project session | story session | task session | 需 agent 声明 | 需 workflow 声明 |
|---|---|---|---|---|---|
| file_system | Y | Y | Y* | N | N |
| canvas | Y | N | N | N | N |
| workflow | Y | Y | Y | N | Y (有 active lifecycle) |
| collaboration | Y | N | N | N | N |
| story_management | N | Y | N | N | N |
| task_management | N | N | Y | N | N |
| relay_management | Y | N | N | N | N |
| workflow_management | Y | N | N | Y | N |
| `mcp:*` | — | — | — | — | — |

> *task session 的 file_system 由外部执行器 native 提供，不通过 ToolCluster
> `mcp:*` 不受 visibility rule 限制，由 step/workflow 声明即生效（config 须在 agent 级别注册）

## Requirements

### 1a: 定义 Capability 协议（SPI 层）

- [ ] 在 `agentdash-spi` 中新增 `tool_capability` 模块（ToolCapability = String newtype）
- [ ] 定义平台 well-known key 常量集
- [ ] 定义 well-known key → ToolCluster[] 映射函数
- [ ] 定义 well-known key → 平台 MCP scope 映射函数
- [ ] 定义 `mcp:*` key 解析函数（提取 server_name）
- [ ] 新增 `CapabilityVisibilityRule` 结构与默认规则集
- [ ] 新增 `CapabilityResolver`：输入 (session owner type, agent config, active workflow) -> 输出 (FlowCapabilities, Vec<McpInjectionConfig>)
- [ ] Resolver 对 `mcp:*` key 的处理：从 agent config 的 mcp_servers 中按 name 查找

### 1b: 收口 Project Agent 的工具注入

- [ ] `routine/executor.rs` 中的硬编码 FlowCapabilities 改为由 CapabilityResolver 计算
- [ ] Project Agent 的 MCP 注入从硬编码 `for_relay()` 改为基于 effective capabilities
- [ ] WorkflowMcpServer 基于 `workflow_management` capability 条件注入

### 1c: 收口 Task session 的工具注入

- [ ] `task/gateway/turn_context.rs` 中的 MCP 注入改为基于 effective capabilities
- [ ] `task/session_runtime_inputs.rs` 中的 MCP server 列表改为由 CapabilityResolver 产出

### 1d: 文档化

- [ ] 在 `.trellis/spec/backend/` 中新建 `tool-capability-pipeline.md`

## Acceptance Criteria

- [ ] 所有 session 类型的工具集由 CapabilityResolver 统一产出，无硬编码分支
- [ ] WorkflowMcpServer 仅对声明了 `workflow_management` capability 的 Agent 可见
- [ ] StoryMcpServer 仅对 story-level session 可见
- [ ] 现有 Task 执行流程不受影响（回归测试通过）
- [ ] Project Agent 行为不变（工具集等价）
- [ ] CapabilityResolver 有单元测试覆盖所有 session 类型 x capability 组合

## Technical Notes

### 关键文件

| 文件 | 变更类型 | 说明 |
|---|---|---|
| `agentdash-spi/src/connector.rs` | 扩展 | 新增 ToolCapability, CapabilityResolver |
| `agentdash-application/src/routine/executor.rs` | 重构 | 收口 Project Agent FlowCapabilities + MCP |
| `agentdash-application/src/task/gateway/turn_context.rs` | 重构 | 收口 Task MCP 注入 |
| `agentdash-application/src/task/session_runtime_inputs.rs` | 重构 | MCP server 列表改为 resolver 产出 |
| `agentdash-application/src/vfs/tools/provider.rs` | 不变 | 消费 FlowCapabilities，无需修改 |
| `agentdash-mcp/src/injection.rs` | 微调 | 配合 CapabilityResolver 输出 |

### 后续依赖

本任务完成后为 `04-19-dynamic-agent-capability-provisioning` (P2) 铺路，
后者在此基础上增加 step 级声明和 hook 管线的动态注入。
