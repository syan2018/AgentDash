# 工具能力管线（Tool Capability Pipeline）

> Session 工具集的声明式治理规范。

---

## 概述

所有 session（Project / Story / Task）的工具集由 **CapabilityResolver** 统一计算产出，
不再在各 session 创建路径中硬编码 `CapabilityState` 或 `McpInjectionConfig`。

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

> 2026-04-22 ToolCapabilityDirective 重构起，`file_system` 别名已彻底下线。
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
    || (rule.workflow_can_grant && workflow_declares)
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

## ToolCapabilityPath 语法（2026-04-22 引入）

`ToolCapabilityDirective` 的 payload 是 `ToolCapabilityPath`，统一表达「能力级」与「工具级」寻址：

```rust
pub struct ToolCapabilityPath {
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
| `workflow_management`                    | 短 path — 平台 MCP 能力级     |
| `workflow_management::upsert`            | 长 path — 平台 MCP 工具级     |
| `mcp:code_analyzer::scan`                | 长 path — 用户自定义 MCP 工具级 |

序列化形式：path 整体以 qualified string 表示，directive 包装为 `{"add": "<path>"}` / `{"remove": "<path>"}`。

示例 JSON：

```json
{
  "capability_config": {
    "tool_directives": [
      {"add": "workflow_management"},
      {"remove": "shell_execute"},
      {"add": "file_read::fs_read"},
      {"remove": "file_read::fs_grep"},
      {"add": "mcp:code_analyzer"}
    ]
  }
}
```

## Slot 归约规则

`reduce_tool_capability_directives(directives)` 按顺序消费指令，对每个 capability key 维护一个 slot：

```rust
enum ToolCapabilitySlotState {
    NotDeclared,                        // 初始
    FullCapability,                      // 命中过 Add(cap, None)
    ToolWhitelist(BTreeSet<String>),     // 仅命中过 Add(cap, Some(tool))
    Blocked,                             // 最后一次命中 Remove(cap, None)
}
```

工具级屏蔽只在归约中间态维护 `ToolCapabilityReduction.excluded_tools:
BTreeMap<capability, BTreeSet<tool>>`，用于让 Resolver 编译出运行态策略。
它不是运行态工具状态，禁止直接被 tool builder、MCP discovery 或前端事件消费。

转移表（后来者胜）：

| 指令                              | NotDeclared        | FullCapability | ToolWhitelist{S} | Blocked        |
| --------------------------------- | ------------------ | -------------- | ---------------- | -------------- |
| `Add(cap, None)`                  | FullCapability     | -              | FullCapability   | FullCapability |
| `Add(cap, Some(t))`               | ToolWhitelist{t}   | -              | add t to S       | ToolWhitelist{t} |
| `Remove(cap, None)`               | Blocked            | Blocked        | Blocked          | -              |
| `Remove(cap, Some(t))`            | excluded_tools+=t  | excluded+=t    | S.remove(t) 且 excluded+=t | excluded+=t |

`CapabilityResolver` 在 agent baseline（auto_granted）上应用以上 reduction：
- `Blocked` → baseline 中即便 auto_granted=true 也被移除
- `FullCapability` / `ToolWhitelist` → 加入 effective_caps
- `ToolWhitelist` 与 `Remove(cap, Some(tool))` 统一编译到
  `CapabilityState.tool_policy[capability]`
- 若 capability 最终不在 `effective_caps`，该 capability 下的工具级 filter 不进入运行态

## 运行态工具策略

运行态只有一个工具级策略字段：

```rust
pub struct CapabilityState {
    pub tool: ToolDimension,
    pub companion: CompanionDimension,
    pub vfs: VfsDimension,
}

pub struct ToolDimension {
    pub capabilities: BTreeSet<ToolCapability>,
    pub enabled_clusters: BTreeSet<ToolCluster>,
    pub tool_policy: BTreeMap<String, ToolCapabilityFilter>,
    pub mcp_servers: Vec<SessionMcpServer>,
}

pub struct ToolCapabilityFilter {
    pub include_only: BTreeSet<String>,
    pub exclude: BTreeSet<String>,
}
```

边界定义：

- `ToolCapabilityDirective`：配置层输入 DSL，仅表达 workflow / step 的 add/remove 意图
- `ToolCapabilityReduction`：Resolver 内部归约中间态，仅用于实现 slot 状态机
- `CapabilityState.tool_policy`：运行态唯一工具级 policy，所有工具暴露层必须消费它

运行态禁止新增或保留与 `tool_policy` 并行表达同一件事的状态字段，例如
`CapabilityState.excluded_tools`、`CapabilityState.included_tools` 或持久化的
`*_tool_paths`。事件、Markdown 通知、前端展示所需的 path 列表，必须从
`tool_policy` 派生。

所有工具发现入口必须调用 capability-aware 判定：

```rust
capability_state.is_capability_tool_enabled(
    capability_key,
    tool_name,
    optional_cluster,
)
```

适用入口：

- 本地 runtime tools：按平台 capability key + `ToolCluster` 双重裁剪
- 直连 MCP discovery：先把 server name 映射回 capability key，再按原始 MCP tool name 裁剪
- Relay MCP discovery：按 relay 返回的 `server_name` / `tool_name` 裁剪

`is_capability_tool_enabled` 在 `capabilities` 非空时必须先确认 capability
本身有效，再应用 `include_only` / `exclude`。这样即使某条链路意外挂上 MCP server，
只要 canonical state 没授予对应 capability，工具仍不会暴露。

MCP / 平台 MCP 工具没有 cluster 兜底，必须始终先命中
`CapabilityState.capabilities` 中的 capability key 才能暴露。`CapabilityState::default()`
不得被解释为 MCP 工具的兼容式全量放行；这条约束用于防止 session bootstrap 或
agent preset 把 MCP server 透传进来时绕过 workflow step 的工具级策略。

## 工具 schema 告知链路

工具 schema 的可读告知必须与运行态工具集同源，禁止 system prompt 另行渲染
`Available Tools` 或简化版参数摘要。PiAgent 侧有两层职责：

- `BridgeRequest.tools` 是 provider function/tool calling 协议字段，只承载当前
  `CapabilityState` 过滤后的真实 `ToolDefinition`，不得被当作上下文提示的并行
  生成源。
- 给 Agent 阅读的工具 schema 告知统一走 `ContextFrame` + `HookTurnStartNotice`
  (`RuntimeEventSource::RuntimeContextUpdate`)。初始化 owner bootstrap 时可以发送当前
  `assembled_tools` 的完整 surface；workflow/capability 变化时必须发送 delta。
- 初始化工具 schema 告知的结构化源是 `ContextFrameSection::ToolSchema`。
  runtime capability state 变化后的工具告知必须使用
  `ContextFrameSection::ToolSchemaDelta`，只发送由 `CapabilityStateDelta`
  影响到的工具 path / schema delta，不发送完整当前工具 schema。每个 tool entry
  必须至少包含 provider 实际使用的 `name`、`description`、`parameters_schema`；
  能从平台工具目录推断时还必须携带 `capability_key`、`source` 与 `tool_path`，
  让前端可以把“本次新增/恢复的可调用工具”与能力来源对应起来。
- `rendered_text` 必须由工具模块自己的 typed metadata 渲染得到，且与
  `SessionMetaUpdate { key: "context_frame" }` 持久化的 frame 同源。禁止继续把完整
  tool JSON schema 只塞进 Markdown code block，而不提供前端可绘制的 tool schema
  section；runtime delta 通知也禁止退化为全量 schema dump。

因此一次 Plan → Apply 流转必须同时满足：

- Plan 初始 `assembled_tools` / `BridgeRequest.tools` 不包含被 workflow step
  remove 的 upsert 工具。
- Plan 初始 system prompt 不包含 `## Available Tools`，也不包含工具 description /
  参数 schema 的副本。
- Apply 流转后 runtime notice 中展示本次 state delta 影响到的工具 schema delta，且
  `connector.update_session_tools(...)` 已把同一份工具集 replace 到 live PiAgent。
- 前端主视图默认展示工具清单与参数字段摘要；完整参数 schema 和 `rendered_text`
  只在展开区显示，避免再次退化为“主视图堆原始 JSON”。

## CapabilityResolver

### 位置

- 协议类型：`agentdash-spi/src/tool_capability.rs`
- Resolver 实现：`agentdash-application/src/capability/resolver.rs`

### 输入

```rust
CapabilityResolverInput {
    /// session 归属上下文（决定 visibility 基线 + platform MCP scope）
    owner_ctx: SessionOwnerCtx,
    /// 各来源按 directive 应用顺序排列的 contributions；授权语义由 source 显式决定
    contributions: Vec<ContextContributions>,
    /// MCP server 候选数据源
    mcp_candidates: McpCandidates,
}

ContextContributions {
    source: ContextContributionSource,
    tool: Option<ToolContribution>,
    companion: Option<CompanionContribution>,
}

ContextContributionSource {
    Agent,
    Workflow,
    Resource,
}

ToolContribution {
    directives: Vec<ToolCapabilityDirective>,
    has_active_workflow: bool,
}

McpCandidates {
    presets: AvailableMcpPresets,
    agent_servers: Vec<AgentMcpServerEntry>,
}
```

> **2026-05-09 输入侧 ContextContributions 化重构**
>
> 消灭了 `agent_declared_capabilities`（冗余中间字段）、`SessionWorkflowContext`（冗余 wrapper）
> 和 `companion_slice_mode`（不属于能力表达的 session 上下文管理概念）。
> 各来源的能力意图统一用 `ToolCapabilityDirective` 表达，Resolver 内部按维度合并解析。
> `ContextContributions.source` 是授权语义的一等字段，禁止再通过数组下标推断
> agent / workflow 来源。Agent 来源的 Add directive 只进入 `agent_can_grant`
> 路径；Workflow 来源的 Add directive 只进入 `workflow_can_grant` 路径；
> Resource 来源只承载 MCP / Companion 等候选数据，不授予 well-known capability。
> `has_active_workflow=true` 只隐式授予 lifecycle 运行所需的 `workflow` 能力；
> `workflow_management` 等管理能力必须来自 Agent 或 Workflow 的显式 Add directive。

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
    state: CapabilityState,
}
```

`state` 是 Resolver 唯一输出。平台 MCP、自定义 MCP 与能力 key 不得作为旧式并行
字段重新出现在输出结构；需要展示或 diff 时从 `CapabilityState` 派生。

### 无状态

Resolver 是纯函数式设计，所有依赖通过 input 传入，便于测试和推理。

## 调用规范

### 添加新 session 类型时

必须通过 `CapabilityResolver::resolve()` 获取工具集，禁止直接构造 `CapabilityState` 或 `McpInjectionConfig`。

**`has_active_workflow` 与 `workflow_tool_directives` 禁止硬编码 `false / None`**——
必须调用 `resolve_session_workflow_context(...)` 从 owner → agent_link/lifecycle_run 装配，
或（Task owner）直接复用已持有的 `ActiveWorkflowProjection.active_step` 经
`tool_directives_from_active_workflow(workflow)` 计算。
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
输入中的 `has_active_workflow` 与 `workflow_tool_directives` 由谁在哪里装配，关系到
`workflow_can_grant` 等授予语义是否真正生效。两个装配时机必须保持契约一致：

### 时机一：Session 创建（bootstrap）

**入口**：`agentdash-application/src/capability/session_workflow_context.rs`

```rust
resolve_session_workflow_context(
    SessionWorkflowRepos { agent_link, lifecycle_def, workflow_def },
    SessionWorkflowOwner::{Project | Story | Routine | ...},
) -> Option<ToolContribution>
```

Owner 解析规则：

| Owner 变体 | 查询方式 | 无绑定时 |
|---|---|---|
| `Project { project_id, agent_id }` | `agent_link.find_by_project_and_agent(...)` → `default_lifecycle_key` | 返回 `None` |
| `Story { project_id }` | `agent_link.list_by_project(...)` + filter `is_default_for_story=true` | 返回 `None` |
| `Routine { project_id, agent_id }` | 等价于 `Project` 路径 | 返回 `None` |
| `Task` | 不经此 helper；见下一小节 | — |

拿到 `lifecycle_key` 后：
`lifecycle_def.get_by_project_and_key(...)` → 定位 `entry_step_key` 对应 workflow →
`workflow.contract.capability_config.tool_directives` 直接克隆为 session bootstrap 的 directive 序列 →
返回 `Some(ToolContribution { directives, has_active_workflow: true })`。调用方必须包成
`ContextContributions { source: ContextContributionSource::Workflow, ... }` 后传给 Resolver。

**Baseline 语义**：session 创建时尚无 hook runtime baseline，workflow contract
的 directive 序列直接作为第一轮 baseline；运行时增删由 hook runtime 的 delta 指令叠加。

**错误容忍**：repo 查询失败、未找到、key 不存在 → 全部返回 `None` +
`tracing::warn!`，不阻断 session 创建。

**Task owner 特殊路径**：Task session 创建时已通过
`resolve_workflow_via_task_sessions` / `resolve_active_workflow_projection_for_session`
拿到 `ActiveWorkflowProjection`。`task/session_runtime_inputs.rs` 与
`task/gateway/turn_context.rs` 两处 callsite 直接：

```rust
let workflow_tool_directives = projection
    .as_ref()
    .and_then(|p| p.primary_workflow.as_ref())
    .map(tool_directives_from_active_workflow);
```

避免重复走 agent_link 查询。

### 时机二：Workflow Run 推进（runtime）

**入口**：`workflow/orchestrator.rs::create_agent_node_session` 与
`workflow/tools/advance_node.rs`。两者已正确装配：

```rust
contributions: vec![ContextContributions {
    source: ContextContributionSource::Workflow,
    tool: Some(ToolContribution {
        directives: step_delta_directives,
        has_active_workflow: true,
    }),
    ..
}]
```

此处的 `step_delta_directives` 来自 hook runtime 能力集切换的 delta（added/removed）；
Resolver 在 agent 默认能力基线上应用这些标准指令。这是 session 创建后**切步骤**的授予时机。

### 交付契约（时机一 vs 时机二）

| 维度 | 时机一：Session 创建 | 时机二：Workflow 推进 |
|---|---|---|
| 触发 | 创建/revive session | `complete_lifecycle_node` 等 lifecycle 命令 |
| baseline 来源 | 空集（无 runtime） | `hook_session_runtime.current_capabilities()` |
| owner 支持 | Project / Story / Routine / Task | 任意（已存在的 session） |
| 失败语义 | 容忍 + warn，返回 None | 容忍 + warn，保留现有 capability |
| 覆盖责任 | session 第一个 turn 的能力 | 后续每次 step 切换的能力 diff |

两个时机共同保证 `workflow_can_grant` 等 visibility 语义在真实业务链路里触达
`CapabilityResolver`。缺任何一端都会出现"PR 写了规则，实际运行不生效"的设计债。

### Grep 守卫

非测试代码中**禁止**出现下列硬编码（CI 或 review 时应阻断）：

```
has_active_workflow: false
workflow_tool_directives: None
```

旧 `SessionWorkflowContext` 的 `NONE` 常量已不存在；真实代码应返回 `Option<ToolContribution>`，
并在调用点显式标记 `ContextContributionSource::Workflow`。

## 消费者一览

| 消费者 | 文件 | 使用方式 |
|--------|------|----------|
| Project session prompt | `agentdash-api/src/routes/acp_sessions.rs` | `resolve_session_workflow_context(Project)` → `SessionPlanInput` |
| Project session 预览 | `agentdash-api/src/routes/project_sessions.rs` | `resolve_session_workflow_context(Project)` → `Option<ToolContribution>` → `CapabilityResolverInput` |
| Story session prompt | `agentdash-api/src/routes/acp_sessions.rs` | `resolve_session_workflow_context(Story)` → `SessionPlanInput` |
| Story session 预览 | `agentdash-api/src/routes/story_sessions.rs` | `resolve_session_workflow_context(Story)` → `Option<ToolContribution>` → `CapabilityResolverInput` |
| Project Agent (Routine) | `routine/executor.rs` | `resolve_session_workflow_context(Routine)` → `CapabilityResolver::resolve()` |
| Task session runtime | `task/session_runtime_inputs.rs` | `tool_directives_from_active_workflow(primary_workflow)` → `CapabilityResolver::resolve()` |
| Task turn context | `task/gateway/turn_context.rs` | `tool_directives_from_active_workflow(primary_workflow)` → `CapabilityResolver::resolve()` |
| Workflow run 推进 | `workflow/orchestrator.rs` / `workflow/tools/advance_node.rs` | `reduce_tool_capability_directives(hook_runtime_baseline + step.tool_directives)` |
| Context contributor | `context/builtins.rs` | `McpContextContributor` 接受 `McpInjectionConfig` |

## 前端/API Roundtrip 契约

Workflow 与 Lifecycle 编辑链路必须把能力配置当成结构化字段透传，不允许在前端 DTO
中丢弃后端字段后再保存。

- Workflow 级能力配置权威字段是
  `WorkflowDefinition.contract.capability_config.tool_directives`。
- Lifecycle step 级能力配置权威字段是
  `LifecycleStepDefinition.capability_config.tool_directives`；它应用在绑定 workflow
  的 contract 配置之后。
- 前端 mapper / store / editor 新建节点时必须保留 `capability_config` 的
  `tool_directives` 与 `mount_directives`。即使当前 UI 暂不提供 step 级能力编辑器，
  也必须在读取、保存和模板 bootstrap 后 roundtrip 不丢字段。
- 平台 well-known capability 使用平台 key，例如
  `workflow_management::upsert_workflow_tool`；`mcp:<server>` 只表示用户自定义 MCP
  server，例如 `mcp:code_analyzer::scan`。

---

*创建：2026-04-19 — Phase 1 工具能力管线收口*
*更新：2026-04-20 — 新增「装配时机」章节 + 消费者一览对齐到 `resolve_session_workflow_context`*
*更新：2026-04-22 — Directive 模型重构：引入 `ToolCapabilityPath` + slot 归约；`CapabilityEntry` / `file_system` 别名彻底下线；历史 `WorkflowContract.capabilities` 已迁入结构化能力配置。*
*更新：2026-05-06 — 工具能力指令路径硬切到 `WorkflowContract.capability_config.tool_directives`；旧根字段仅作为数据迁移来源，不再是运行时/接口定义。*
*更新：2026-05-07 — 补齐前端/API roundtrip 契约：Lifecycle step 级 `capability_config` 不得在编辑链路丢失。*
*更新：2026-05-08 — 运行态工具策略收敛到 `CapabilityState.tool_policy`；`ToolCapabilityDirective`/`ToolCapabilityReduction` 仅作为配置输入与 Resolver 中间态，所有本地/MCP 工具暴露入口必须消费 capability-aware 判定。*
*更新：2026-05-09 — 工具 schema runtime context 收敛到 `ContextFrameSection::ToolSchemaDelta`，运行时只发送 CapabilityStateDelta 影响到的工具 schema delta；Agent 可见文本与前端结构化卡片同源渲染。*
