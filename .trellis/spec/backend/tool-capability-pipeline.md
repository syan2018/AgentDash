# 工具能力管线（Tool Capability Pipeline）

> Session 工具集的声明式治理规范。

---

## 概述

所有 session（Project / Story / Task）的工具集由 **CapabilityResolver** 统一计算产出，
不再在各 session 创建路径中硬编码 `FlowCapabilities` 或 `McpInjectionConfig`。

## ToolCapability 协议

### 类型定义

`ToolCapability` 是 **开放 string key**（SPI 层 newtype wrapper），非封闭枚举。

约定两类 key：
- **平台 well-known key**：固定字符串，映射到 ToolCluster 和/或平台 MCP scope
- **用户自定义 MCP key**：`mcp:<server_name>` 格式，引用 agent config 中注册的外部 MCP server

### 平台 well-known 能力映射

| Key | ToolCluster(s) | 平台 MCP Scope | 说明 |
|-----|---------------|---------------|------|
| `file_system` | Read, Write, Execute | — | 文件系统读写执行 |
| `canvas` | Canvas | — | Canvas 资产管理 |
| `workflow` | Workflow | — | Lifecycle node 推进 |
| `collaboration` | Collaboration | — | Companion 协作 |
| `story_management` | — | Story | Story 上下文编排 |
| `task_management` | — | Task | Task 状态与产物管理 |
| `relay_management` | — | Relay | 全局看板/Project 管理 |
| `workflow_management` | — | Workflow | Workflow/Lifecycle CRUD |

### 用户自定义 MCP 能力

格式：`mcp:<server_name>`

Resolver 行为：
1. 提取 `<server_name>` 部分
2. 在 agent config 的 `mcp_servers` 中按 name 查找
3. 找到 → 注入该 MCP server；未找到 → 警告日志，跳过

## Visibility Rule

仅适用于平台 well-known 能力。`mcp:*` 不受 visibility rule 限制。

语义分两层：**屏蔽走 AND，授予走 OR**。

- **屏蔽（AND）**：`allowed_owner_types` 是硬边界，不在列表的 owner 一定不可见。
- **授予（OR）**：`auto_granted` / `agent_can_grant` / `workflow_can_grant` 三个布尔源，至少一个命中即视为被授予。

```
CapabilityVisibilityRule {
    key: String,
    allowed_owner_types: [SessionOwnerType],   // 硬边界（AND）
    auto_granted: bool,                         // owner 匹配就默认授予（基础能力）
    agent_can_grant: bool,                      // agent config 显式声明即授予
    workflow_can_grant: bool,                   // 当前 workflow 声明即授予
}
```

判定伪代码：

```
if cap.is_custom_mcp(): return true
rule = find_rule(cap) or return false
if owner_type not in rule.allowed_owner_types: return false
return rule.auto_granted
    || (rule.agent_can_grant && agent_declares)
    || (rule.workflow_can_grant && has_active_workflow)
```

### 默认矩阵

| Key | Project | Story | Task | auto | agent | workflow |
|-----|---------|-------|------|------|-------|----------|
| file_system | ✓ | ✓ | ✓* | ✓ | — | — |
| canvas | ✓ | — | — | ✓ | — | — |
| workflow | ✓ | ✓ | ✓ | — | — | ✓ |
| collaboration | ✓ | — | — | ✓ | — | — |
| story_management | — | ✓ | — | ✓ | — | — |
| task_management | — | — | ✓ | ✓ | — | — |
| relay_management | ✓ | — | — | ✓ | — | — |
| workflow_management | ✓ | — | — | — | ✓ | ✓ |

> *Task session 的 file_system 由外部执行器 native 提供，不通过 ToolCluster
>
> `workflow_management` 同时开启 agent 与 workflow 两条授予源：前端未提供 agent 能力配置入口时，通过绑定 `builtin_workflow_admin` 等内建工作流即可赋能；agent config 显式声明的旧路径也继续可用。

## CapabilityResolver

### 位置

- 协议类型：`agentdash-spi/src/tool_capability.rs`
- Resolver 实现：`agentdash-application/src/capability/resolver.rs`

### 输入

```rust
CapabilityResolverInput {
    owner_type: SessionOwnerType,
    mcp_base_url: Option<String>,
    project_id: Uuid,
    story_id: Option<Uuid>,
    task_id: Option<Uuid>,
    agent_declared_capabilities: Option<Vec<String>>,
    has_active_workflow: bool,
    workflow_capabilities: Vec<String>,
    agent_mcp_servers: Vec<AgentMcpServerEntry>,
}
```

### 输出

```rust
CapabilityResolverOutput {
    flow_capabilities: FlowCapabilities,
    platform_mcp_configs: Vec<McpInjectionConfig>,
    effective_capabilities: BTreeSet<ToolCapability>,
}
```

### 无状态

Resolver 是纯函数式设计，所有依赖通过 input 传入，便于测试和推理。

## 调用规范

### 添加新 session 类型时

必须通过 `CapabilityResolver::resolve()` 获取工具集，禁止直接构造 `FlowCapabilities` 或 `McpInjectionConfig`。

### 添加新平台能力时

1. 在 `agentdash-spi/src/tool_capability.rs` 中添加 well-known key 常量
2. 更新 `WELL_KNOWN_KEYS` 数组
3. 在 `capability_to_tool_clusters()` 和/或 `capability_to_platform_mcp_scope()` 中添加映射
4. 在 `default_visibility_rules()` 中添加可见性规则
5. 添加对应的单元测试

### 支持新 MCP 前缀时

在 `CapabilityResolver::resolve()` 中添加新前缀的解析分支（当前仅支持 `mcp:`）。

## 消费者一览

| 消费者 | 文件 | 使用方式 |
|--------|------|----------|
| Project Agent (Routine) | `routine/executor.rs` | `CapabilityResolver::resolve()` → `FlowCapabilities` + MCP ACP servers |
| Task session runtime | `task/session_runtime_inputs.rs` | `CapabilityResolver::resolve()` → `RuntimeMcpServer` 列表 |
| Task turn context | `task/gateway/turn_context.rs` | `CapabilityResolver::resolve()` → `McpContextContributor` |
| Context contributor | `context/builtins.rs` | `McpContextContributor` 接受 `McpInjectionConfig` |

---

*创建：2026-04-19 — Phase 1 工具能力管线收口*
