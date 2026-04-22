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
| `file_read` | Read | — | 文件读取（mounts_list / fs_read / fs_glob / fs_grep） |
| `file_write` | Write | — | 文件写入（fs_apply_patch） |
| `shell_execute` | Execute | — | Shell 命令执行 |
| `canvas` | Canvas | — | Canvas 资产管理 |
| `workflow` | Workflow | — | Lifecycle node 推进 |
| `collaboration` | Collaboration | — | Companion 协作 |
| `story_management` | — | Story | Story 上下文编排 |
| `task_management` | — | Task | Task 状态与产物管理 |
| `relay_management` | — | Relay | 全局看板/Project 管理 |
| `workflow_management` | — | Workflow | Workflow/Lifecycle CRUD |

> 2026-04-22 capability_directives 重构起，`file_system` 别名已彻底下线。
> Workflow contract 必须直接声明 `file_read / file_write / shell_execute` 细粒度 key；
> 老数据通过 migration 0018 一次性拆解为三条 Add 指令。

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
| file_read | ✓ | ✓ | ✓* | ✓ | — | — |
| file_write | ✓ | ✓ | ✓* | ✓ | — | — |
| shell_execute | ✓ | ✓ | ✓* | ✓ | — | — |
| canvas | ✓ | — | — | ✓ | — | — |
| workflow | ✓ | ✓ | ✓ | — | — | ✓ |
| collaboration | ✓ | — | — | ✓ | — | — |
| story_management | — | ✓ | — | ✓ | — | — |
| task_management | — | — | ✓ | ✓ | — | — |
| relay_management | ✓ | — | — | ✓ | — | — |
| workflow_management | ✓ | — | — | — | ✓ | ✓ |

> *Task session 的文件访问由外部执行器 native 提供，不通过 ToolCluster
>
> `workflow_management` 同时开启 agent 与 workflow 两条授予源：前端未提供 agent 能力配置入口时，通过绑定 `builtin_workflow_admin` 等内建工作流即可赋能；agent config 显式声明的旧路径也继续可用。

## CapabilityPath 语法（2026-04-22 引入）

`CapabilityDirective` 的 payload 是 `CapabilityPath`，统一表达「能力级」与「工具级」寻址：

```rust
pub struct CapabilityPath {
    pub capability: String,      // "file_read" / "mcp:code_analyzer"
    pub tool: Option<String>,    // None = 短 path；Some = 长 path（工具级）
}
```

分隔符统一 `::`（与 Rust 模块路径同构），与 `mcp:<server>` 的单冒号前缀不冲突。
MCP server name 禁止含 `::`，由 `McpPreset` 校验层强制。

| 样例                                     | 含义                        |
| ---------------------------------------- | --------------------------- |
| `file_read`                              | 短 path — 平台能力级         |
| `file_read::fs_grep`                     | 长 path — 平台 cluster 工具级 |
| `mcp:workflow_management`                | 短 path — 平台 MCP 能力级     |
| `mcp:workflow_management::upsert`        | 长 path — 平台 MCP 工具级     |
| `mcp:code_analyzer::scan`                | 长 path — 用户自定义 MCP 工具级 |

序列化形式：path 整体以 qualified string 表示，directive 包装为 `{"add": "<path>"}` / `{"remove": "<path>"}`。

示例 JSON：

```json
{
  "capability_directives": [
    {"add": "workflow_management"},
    {"remove": "shell_execute"},
    {"add": "file_read::fs_read"},
    {"remove": "file_read::fs_grep"},
    {"add": "mcp:code_analyzer"}
  ]
}
```

## Slot 归约规则

`reduce_capability_directives(directives)` 按顺序消费指令，对每个 capability key 维护一个 slot：

```rust
enum CapabilitySlotState {
    NotDeclared,                        // 初始
    FullCapability,                      // 命中过 Add(cap, None)
    ToolWhitelist(BTreeSet<String>),     // 仅命中过 Add(cap, Some(tool))
    Blocked,                             // 最后一次命中 Remove(cap, None)
}
```

工具级屏蔽独立维护 `excluded_tools: BTreeMap<capability, BTreeSet<tool>>`。

转移表（后来者胜）：

| 指令                              | NotDeclared        | FullCapability | ToolWhitelist{S} | Blocked        |
| --------------------------------- | ------------------ | -------------- | ---------------- | -------------- |
| `Add(cap, None)`                  | FullCapability     | -              | FullCapability   | FullCapability |
| `Add(cap, Some(t))`               | ToolWhitelist{t}   | -              | add t to S       | ToolWhitelist{t} |
| `Remove(cap, None)`               | Blocked            | Blocked        | Blocked          | -              |
| `Remove(cap, Some(t))`            | excluded_tools+=t  | excluded+=t    | S.remove(t) 且 excluded+=t | excluded+=t |

`CapabilityResolver` 在 agent baseline（auto_granted）上应用以上 reduction：
- `Blocked` → baseline 中即便 auto_granted=true 也被移除
- `FullCapability` / `ToolWhitelist` → 加入 effective_caps；`ToolWhitelist` 下未命中的工具进入 `flow_capabilities.excluded_tools`
- `excluded_tools` 直接传递到 `flow_capabilities.excluded_tools`（叠加在能力仍可见的情况下）

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
    workflow_ctx: SessionWorkflowContext,  // has_active_workflow + workflow_capability_directives
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

**`has_active_workflow` 与 `workflow_capability_directives` 禁止硬编码 `false / None`**——
必须调用 `resolve_session_workflow_context(...)` 从 owner → agent_link/lifecycle_run 装配，
或（Task owner）直接复用已持有的 `ActiveWorkflowProjection.active_step` 经
`capability_directives_from_active_workflow(workflow)` 计算。
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
输入中的 `has_active_workflow` 与 `workflow_capability_directives` 由谁在哪里装配，关系到
`workflow_can_grant` 等授予语义是否真正生效。两个装配时机必须保持契约一致：

### 时机一：Session 创建（bootstrap）

**入口**：`agentdash-application/src/capability/session_workflow_context.rs`

```rust
resolve_session_workflow_context(
    SessionWorkflowRepos { agent_link, lifecycle_def, workflow_def },
    SessionWorkflowOwner::{Project | Story | Routine | ...},
) -> SessionWorkflowContext { has_active_workflow, workflow_capability_directives }
```

Owner 解析规则：

| Owner 变体 | 查询方式 | 无绑定时 |
|---|---|---|
| `Project { project_id, agent_id }` | `agent_link.find_by_project_and_agent(...)` → `default_lifecycle_key` | 返回 `NONE` |
| `Story { project_id }` | `agent_link.list_by_project(...)` + filter `is_default_for_story=true` | 返回 `NONE` |
| `Routine { project_id, agent_id }` | 等价于 `Project` 路径 | 返回 `NONE` |
| `Task` | 不经此 helper；见下一小节 | — |

拿到 `lifecycle_key` 后：
`lifecycle_def.get_by_project_and_key(...)` → 定位 `entry_step_key` 对应 workflow →
`workflow.contract.capability_directives` 直接克隆为 session bootstrap 的 directive 序列 →
返回 `(has_active_workflow=true, workflow_capability_directives=Some(directives))`。

**Baseline 语义**：session 创建时尚无 hook runtime baseline，workflow contract
的 directive 序列直接作为第一轮 baseline；运行时增删由 hook runtime 的 delta 指令叠加。

**错误容忍**：repo 查询失败、未找到、key 不存在 → 全部 fallback 到
`SessionWorkflowContext::NONE` + `tracing::warn!`，不阻断 session 创建。

**Task owner 特殊路径**：Task session 创建时已通过
`resolve_workflow_via_task_sessions` / `resolve_active_workflow_projection_for_session`
拿到 `ActiveWorkflowProjection`。`task/session_runtime_inputs.rs` 与
`task/gateway/turn_context.rs` 两处 callsite 直接：

```rust
let workflow_capability_directives = projection
    .as_ref()
    .and_then(|p| p.primary_workflow.as_ref())
    .map(capability_directives_from_active_workflow);
```

避免重复走 agent_link 查询。

### 时机二：Workflow Run 推进（runtime）

**入口**：`workflow/orchestrator.rs::create_agent_node_session` 与
`workflow/tools/advance_node.rs`。两者已正确装配：

```rust
CapabilityResolverInput {
    has_active_workflow: true,
    workflow_capability_directives: Some(step_delta_directives),
    ...
}
```

此处的 `step_delta_directives` 来自 hook runtime 能力集切换的 delta（added/removed）；
Resolver 在 agent 默认能力基线上应用这些标准指令。这是 session 创建后**切步骤**的授予时机。

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
workflow_capability_directives: None
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
| Task session runtime | `task/session_runtime_inputs.rs` | `capability_directives_from_active_workflow(primary_workflow)` → `CapabilityResolver::resolve()` |
| Task turn context | `task/gateway/turn_context.rs` | `capability_directives_from_active_workflow(primary_workflow)` → `CapabilityResolver::resolve()` |
| Workflow run 推进 | `workflow/orchestrator.rs` / `workflow/tools/advance_node.rs` | `reduce_capability_directives(hook_runtime_baseline + step.capability_directives)` |
| Context contributor | `context/builtins.rs` | `McpContextContributor` 接受 `McpInjectionConfig` |

---

*创建：2026-04-19 — Phase 1 工具能力管线收口*
*更新：2026-04-20 — 新增「装配时机」章节 + 消费者一览对齐到 `resolve_session_workflow_context`*
*更新：2026-04-22 — Directive 模型重构：引入 `CapabilityPath` + slot 归约；`CapabilityEntry` / `file_system` 别名彻底下线；`WorkflowContract.capabilities` → `capability_directives`（migration 0018）*
