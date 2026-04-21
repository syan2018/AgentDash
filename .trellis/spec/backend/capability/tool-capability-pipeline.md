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
    /// session 归属上下文（owner_type + 关联 ID 合一的 sum type）
    owner_ctx: SessionOwnerCtx,
    mcp_base_url: Option<String>,
    agent_declared_capabilities: Option<Vec<String>>,
    workflow_ctx: SessionWorkflowContext,  // has_active_workflow + workflow_capabilities
    agent_mcp_servers: Vec<AgentMcpServerEntry>,
    companion_slice_mode: Option<CompanionSliceMode>,
}
```

> **owner_ctx 收口**（2026-04-21，`04-20-session-owner-sum-type` PR1）
>
> 历史上 resolver 用四字段并列 `(owner_type, project_id, story_id: Option, task_id: Option)`
> 表达归属,合法组合 3 种却允许 $2^3$ 种表示。PR1 用
> [`SessionOwnerCtx`](../../../../crates/agentdash-domain/src/session_binding/value_objects.rs) sum
> type 收口,非法状态在类型层被排除:
>
> ```rust
> pub enum SessionOwnerCtx {
>     Project { project_id: Uuid },
>     Story   { project_id: Uuid, story_id: Uuid },
>     Task    { project_id: Uuid, story_id: Uuid, task_id: Uuid },
> }
> ```
>
> `build_platform_mcp_config` 通过 `ctx.story_id()` / `ctx.task_id()` getter 复用旧取值语义,
> `is_capability_visible` 通过 `ctx.owner_type()` 继续使用 `SessionOwnerType` 硬边界。
>
> **后续收口进度**(见 `04-20-session-owner-sum-type` task):
>
> - PR2:`SessionPlanInput.owner_type` 升级为 `owner_ctx: SessionOwnerCtx`;
>   同步删除从未被调用的 `RuntimeMcpBinding` 死代码(113 行)
> - PR3:`HookOwnerSummary.owner_type: String` → `SessionOwnerType` 强类型化,
>   JSON 对外契约通过 `#[serde(rename_all = "snake_case")]` 保持 `"project"/"story"/"task"`
>
> 仍未收口的:`SessionBinding` 持久化层的 `(owner_type, owner_id)` ↔ `SessionOwnerCtx`
> 显式转换(Task 变体需异步查 task 表补 story_id),
> 以及 `HookOwnerSummary` 剩余 4 个 `Option<String>` id 字段。

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

**`has_active_workflow` 与 `workflow_capabilities` 禁止硬编码 `false / None`**——
必须调用 `resolve_session_workflow_context(...)` 从 owner → agent_link/lifecycle_run 装配，
或（Task owner）直接复用已持有的 `ActiveWorkflowProjection.active_step` 经
`capabilities_from_active_step(&step)` 计算。
具体交付契约见 [装配时机](#装配时机)。

### 添加新平台能力时

1. 在 `agentdash-spi/src/tool_capability.rs` 中添加 well-known key 常量
2. 更新 `WELL_KNOWN_KEYS` 数组
3. 在 `capability_to_tool_clusters()` 和/或 `capability_to_platform_mcp_scope()` 中添加映射
4. 在 `default_visibility_rules()` 中添加可见性规则
5. 添加对应的单元测试

### 支持新 MCP 前缀时

在 `CapabilityResolver::resolve()` 中添加新前缀的解析分支（当前仅支持 `mcp:`）。

## 装配时机

`CapabilityResolver` 只决定"给定输入 → 给定输出"，**不负责发现 workflow**。
输入中的 `has_active_workflow` 与 `workflow_capabilities` 由谁在哪里装配，关系到
`workflow_can_grant` 等授予语义是否真正生效。两个装配时机必须保持契约一致：

### 时机一：Session 创建（bootstrap）

**入口**：`agentdash-application/src/capability/session_workflow_context.rs`

```rust
resolve_session_workflow_context(
    SessionWorkflowRepos { agent_link, lifecycle_def, workflow_def },
    SessionWorkflowOwner::{Project | Story | Routine | ...},
) -> SessionWorkflowContext { has_active_workflow, workflow_capabilities }
```

Owner 解析规则：

| Owner 变体 | 查询方式 | 无绑定时 |
|---|---|---|
| `Project { project_id, agent_id }` | `agent_link.find_by_project_and_agent(...)` → `default_lifecycle_key` | 返回 `NONE` |
| `Story { project_id }` | `agent_link.list_by_project(...)` + filter `is_default_for_story=true` | 返回 `NONE` |
| `Routine { project_id, agent_id }` | 等价于 `Project` 路径 | 返回 `NONE` |
| `Task` | 不经此 helper；见下一小节 | — |

拿到 `lifecycle_key` 后：
`lifecycle_def.get_by_project_and_key(...)` → 定位 `entry_step_key` 对应的 step →
`compute_effective_capabilities(&[], &step.capabilities)` →
返回 `(has_active_workflow=true, workflow_capabilities=Some(effective))`。

**Baseline 语义**：session 创建时尚无 hook runtime baseline，统一用**空集** + step
`Add / Remove` 指令。Workflow contract 级 baseline（如有）留待未来扩展接入。

**错误容忍**：repo 查询失败、未找到、key 不存在 → 全部 fallback 到
`SessionWorkflowContext::NONE` + `tracing::warn!`，不阻断 session 创建。

**Task owner 特殊路径**：Task session 创建时已通过
`resolve_workflow_via_task_sessions` / `resolve_active_workflow_projection_for_session`
拿到 `ActiveWorkflowProjection`。`task/session_runtime_inputs.rs` 与
`task/gateway/turn_context.rs` 两处 callsite 直接：

```rust
let workflow_capabilities = projection
    .as_ref()
    .map(|p| capabilities_from_active_step(&p.active_step));
```

避免重复走 agent_link 查询。

### 时机二：Workflow Run 推进（runtime）

**入口**：`workflow/orchestrator.rs::create_agent_node_session` 与
`workflow/tools/advance_node.rs`。两者已正确装配：

```rust
CapabilityResolverInput {
    has_active_workflow: true,
    workflow_capabilities: Some(step_effective_caps), // 含 runtime hook baseline
    ...
}
```

此处的 `step_effective_caps` 由 `compute_effective_capabilities(&runtime_baseline, &step.capabilities)`
产出，其中 `runtime_baseline` = `hook_session_runtime.current_capabilities()`
（即 session 当前已激活的能力集合）。这是 session 创建后**切步骤**的授予时机。

### 交付契约（时机一 vs 时机二）

| 维度 | 时机一：Session 创建 | 时机二：Workflow 推进 |
|---|---|---|
| 触发 | 创建/revive session | `complete_lifecycle_node` 等 lifecycle 命令 |
| baseline 来源 | 空集（无 runtime） | `hook_session_runtime.current_capabilities()` |
| owner 支持 | Project / Story / Routine / Task | 任意（已存在的 session） |
| 失败语义 | 容忍 + warn，回退 NONE | 容忍 + warn，保留现有 capability |
| 覆盖责任 | session 第一个 turn 的能力 | 后续每次 step 切换的能力 diff |

两个时机共同保证 `workflow_can_grant` 等 visibility 语义在真实业务链路里触达
`CapabilityResolver`。缺任何一端都会出现"PR 写了规则，实际运行不生效"的设计债。

### Grep 守卫

非测试代码中**禁止**出现下列硬编码（CI 或 review 时应阻断）：

```
has_active_workflow: false
workflow_capabilities: None
```

唯一允许的例外是 `SessionWorkflowContext::NONE` 常量定义本身与 resolver 的单元测试。

## 消费者一览

| 消费者 | 文件 | 使用方式 |
|--------|------|----------|
| Project session prompt | `agentdash-api/src/routes/acp_sessions.rs` | `resolve_session_workflow_context(Project)` → `SessionPlanInput` |
| Project session 预览 | `agentdash-api/src/routes/project_sessions.rs` | `resolve_session_workflow_context(Project)` → `CapabilityResolverInput` |
| Story session prompt | `agentdash-api/src/routes/acp_sessions.rs` | `resolve_session_workflow_context(Story)` → `SessionPlanInput` |
| Story session 预览 | `agentdash-api/src/routes/story_sessions.rs` | `resolve_session_workflow_context(Story)` → `CapabilityResolverInput` |
| Project Agent (Routine) | `routine/executor.rs` | `resolve_session_workflow_context(Routine)` → `CapabilityResolver::resolve()` |
| Task session runtime | `task/session_runtime_inputs.rs` | `capabilities_from_active_step(projection.active_step)` → `CapabilityResolver::resolve()` |
| Task turn context | `task/gateway/turn_context.rs` | `capabilities_from_active_step(projection.active_step)` → `CapabilityResolver::resolve()` |
| Workflow run 推进 | `workflow/orchestrator.rs` / `workflow/tools/advance_node.rs` | `compute_effective_capabilities(hook_runtime_baseline, &step.capabilities)` |
| Context contributor | `context/builtins.rs` | `McpContextContributor` 接受 `McpInjectionConfig` |

---

*创建：2026-04-19 — Phase 1 工具能力管线收口*
*更新：2026-04-20 — 新增「装配时机」章节 + 消费者一览对齐到 `resolve_session_workflow_context`*
