# 工具能力管线（Tool Capability Pipeline）

> Session 工具集的声明式治理规范。

---

## 概述

所有 session（Project / Story / Task）的工具集由 **CapabilityResolver** 统一计算产出，
不在各 session 创建路径中硬编码 `CapabilityState` 或 `McpInjectionConfig`。

## ToolCapability 协议

`ToolCapability` 是 **开放 string key**（SPI 层 newtype wrapper），非封闭枚举。

两类 key：
- **平台 well-known key**：固定字符串，映射到 ToolCluster 和/或平台 MCP scope
- **用户自定义 MCP key**：`mcp:<preset_key>` 格式，引用 Project MCP Preset

### 平台 well-known 能力映射

| Key | ToolCluster(s) | 平台 MCP Scope | 说明 |
|-----|---------------|---------------|------|
| `file_read` | Read | — | 文件读取 |
| `file_write` | Write | — | 文件写入 |
| `shell_execute` | Execute | — | Shell 命令执行 |
| `workspace_module` | WorkspaceModule | — | Workspace Module 创建、发现、描述、调用与展示；Canvas Agent 入口收束在这里 |
| `workflow` | Workflow | — | Lifecycle node 推进 |
| `collaboration` | Collaboration | — | Companion 协作 |
| `task` | Task | — | `task_read` / `task_write` 读取与维护 run-scoped Task |
| `story_management` | — | Story | Story 上下文编排 |
| `relay_management` | — | Relay | 全局看板/Project 管理 |
| `workflow_management` | — | Workflow | Workflow/Lifecycle CRUD |

### 用户自定义 MCP 能力

格式 `mcp:<preset_key>`，Resolver 在 `McpCandidates.project_presets` 中按 preset key 查找并注入。

`mcp:<preset_key>` 命中 Project MCP Preset 时，Resolver 必须通过 `CapabilityResolverInput.mcp_runtime_context` 调用 `resolve_preset_mcp_server()`，产出带 runtime-resolved transport 的 `RuntimeMcpServer`。这个 context 来自 frame construction final VFS，原因是 custom MCP directive 是运行时 capability projection，不是静态 preset 展示字段。未命中的 `mcp:<preset_key>` 作为不可解析 capability 处理，不生成 runtime MCP server。

## Visibility Rule

仅适用于平台 well-known 能力。`mcp:*` 不受 visibility rule 限制。

语义：**屏蔽走 AND，授予走 OR**。

- **屏蔽**：`allowed_owner_types` 是硬边界，不在列表的 owner 一定不可见
- **授予**：`auto_granted` / `agent_can_grant` / `workflow_can_grant` 三个布尔源，至少一个命中即授予

### 默认矩阵

| Key | Project | Story | Task | auto | agent | workflow |
|-----|---------|-------|------|------|-------|----------|
| file_read | ✓ | ✓ | ✓* | ✓ | — | — |
| file_write | ✓ | ✓ | ✓* | ✓ | — | — |
| shell_execute | ✓ | ✓ | ✓* | ✓ | — | — |
| workspace_module | ✓ | ✓ | ✓ | ✓ | — | — |
| workflow | ✓ | ✓ | ✓ | — | — | ✓ |
| collaboration | ✓ | — | — | ✓ | — | — |
| task | ✓ | ✓ | ✓ | ✓ | — | — |
| story_management | — | ✓ | — | ✓ | — | — |
| relay_management | ✓ | — | — | ✓ | — | — |
| workflow_management | ✓ | — | — | — | ✓ | ✓ |

> *Task session 的文件访问由外部执行器 native 提供，不通过 ToolCluster

## ToolCapabilityPath 语法

`ToolCapabilityDirective` 的 payload 使用 `ToolCapabilityPath`，统一表达能力级与工具级寻址。
分隔符 `::`（与 `mcp:<server>` 的单冒号不冲突）。

| 样例 | 含义 |
| --- | --- |
| `file_read` | 短 path — 平台能力级 |
| `file_read::fs_grep` | 长 path — 平台 cluster 工具级 |
| `mcp:code_analyzer::scan` | 长 path — 用户自定义 MCP 工具级 |

序列化：directive 包装为 `{"add": "<path>"}` / `{"remove": "<path>"}`。

## Slot 归约规则

`reduce_tool_capability_directives(directives)` 按顺序消费指令，对每个 capability key 维护一个 slot 状态机。

状态：`NotDeclared` / `FullCapability` / `ToolWhitelist(Set)` / `Blocked`

转移表（后来者胜）：

| 指令 | NotDeclared | FullCapability | ToolWhitelist{S} | Blocked |
| --- | --- | --- | --- | --- |
| `Add(cap, None)` | FullCapability | - | FullCapability | FullCapability |
| `Add(cap, Some(t))` | ToolWhitelist{t} | - | add t to S | ToolWhitelist{t} |
| `Remove(cap, None)` | Blocked | Blocked | Blocked | - |
| `Remove(cap, Some(t))` | excluded+=t | excluded+=t | S.remove(t) + excluded+=t | excluded+=t |

Resolver 在 agent baseline（auto_granted）上应用 reduction：
- `Blocked` → 即便 auto_granted=true 也被移除
- `FullCapability` / `ToolWhitelist` → 加入 effective_caps
- `ToolWhitelist` 与工具级 Remove 编译到 `CapabilityState.tool_policy`

`CapabilityState.tool.mcp_servers` 保留为 MCP 维度的 capability/draft projection。它承接
`mcp:<preset>` directive 解析、runtime command replay 和工具装配快照，并必须与
`FrameSurfaceDraft.mcp_servers` 同源；AgentRun 当前可执行 MCP surface 的事实源是
AgentFrame revision 的 MCP surface。

## 运行态工具策略

运行态唯一工具级策略字段是 `CapabilityState.tool_policy`。

边界定义：
- `ToolCapabilityDirective`：配置层输入 DSL（workflow/step 的 add/remove 意图）
- `ToolCapabilityReduction`：Resolver 内部归约中间态
- `CapabilityState.tool_policy`：运行态唯一 policy，所有工具暴露层必须消费它

所有工具发现入口必须调用 `capability_state.is_capability_tool_enabled()` 进行 capability-aware 判定。

## Companion Agent Roster Surface

Project Agent 使用 `collaboration` capability 调用 `companion_request(payload.agent_key)` 派发协作 Agent。
Companion roster 是运行态能力状态，不是工具 schema 的一部分；工具 schema 只声明静态参数结构，
可用 `agent_key` 列表必须由 frame context surface 承载。

### Signatures

```rust
pub struct CompanionAgentEntry {
    pub name: String,         // canonical agent_key
    pub executor: String,
    pub display_name: String,
}

pub struct CompanionDimension {
    pub agents: Vec<CompanionAgentEntry>,
}
```

```text
companion_request(target="sub", payload.agent_key="<CompanionAgentEntry.name>", ...)
```

### Contracts

- `CapabilityState.companion.agents` 是当前 frame 可调用 companion roster 的运行态事实源。
- `CompanionAgentEntry.name` 是 `payload.agent_key` 的 canonical 值；`display_name` 只用于展示和模型可读辅助说明。
- ProjectAgent 配置使用目标侧 `default_companion_enabled` 声明是否默认进入同项目 sibling roster；调用侧 `extra_companions` 只额外加入未默认开放的 sibling agents。
- owner bootstrap 生成 roster 的规则是 default-enabled sibling agents ∪ caller extra companions - caller self，并按 canonical `ProjectAgent.name` 去重。这样做的原因是 companion 可用性由目标 Agent 自身声明，caller 配置只表达例外加法。
- Owner bootstrap 必须把非空 roster 渲染为 slot=`companion_agents` 的 `ContextFragment`，并进入 assignment context frame，原因是初始 roster 需要随 frame bootstrapping 进入模型上下文。
- runtime capability transition 必须把 `SetCompanionAgentRosterEffect` 产生的变化写入 `CapabilityStateDelta.companion_agents`，并渲染为 `companion_agent_roster_delta` ContextFrame section，原因是后续动态变更需要通过上下文 delta 通知 Agent。
- `CompanionRequestTool` 解析 `agent_key` 时必须先在当前 frame roster 中匹配，再按 `project_id + name` 读取 selected ProjectAgent identity。
- companion child launch source 必须携带 selected ProjectAgent id 和 canonical agent_key；child `LifecycleAgent.project_agent_id` 绑定 selected ProjectAgent。这样 frame construction、AgentRun 展示与实际 child executor/capability/VFS/skill facts 使用同一个身份来源。
- companion child frame construction 在 parent slice 上叠加 selected ProjectAgent preset facts：executor config、capability directives、MCP presets、VFS grants、skill assets 与 companion return-channel baseline。
- `project_id` 来自当前 delivery runtime session 的 `RuntimeSessionExecutionAnchor`，原因是 hook snapshot 的 `run_context` 只表达 active workflow 投影，不是通用 owner 事实源。
- `AuthorityState.companion.dispatch` 是 roster 投影与 `companion_request(target="sub")` 执行 guard 的上游事实；`AuthorityState.companion.respond` 由 child lineage / gate runtime channel 提供，不依赖 parent dispatch authority。这样禁止发起新 sub companion 不会切断已启动 child 的 `companion_respond` 回流通道。
- `AuthorityState.workspace_module.present`、`AuthorityState.dynamic_workflow.author` 是当前已接入 capability projection 的静态边界：workspace module 展示和 dynamic workflow authoring 默认只面向 main/root ProjectAgent。
- background companion child 默认隐藏 human route；该 guard 当前由 companion tool 根据 child source fail-closed 执行，后续需要在 execution context 携带用户主动进入 companion run 的 provenance 后再统一纳入 Authority projection。

### Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| `collaboration` 未启用 | 不暴露 `companion_request` / `companion_respond` 工具 |
| `companion.dispatch` 未开放 | roster 为空，不注入 `companion_agents` fragment，`target=sub` 执行拒绝 |
| roster 为空 | 不注入 `companion_agents` fragment；带 `agent_key` 的调用返回可用列表为空 |
| `payload.agent_key` 为空 | invalid arguments |
| `payload.agent_key` 不在当前 frame roster | invalid arguments，并列出当前可用 `agent_key` |
| frame roster 指向的 ProjectAgent 不存在 | tool execution failed，说明 frame surface 与 ProjectAgent 存储不一致 |
| delivery runtime session 缺少 execution anchor | tool execution failed，拒绝派发 companion |

### Tests Required

- Capability/frame test asserts `CapabilityState.companion.agents` roundtrip through AgentFrame surface.
- Owner bootstrap test asserts non-empty roster renders slot=`companion_agents` with `agent_key` lines.
- Runtime context transition test asserts companion roster delta renders `companion_agent_roster_delta` section and model-visible `agent_key` lines.
- Companion tool test asserts `run_context=None` + valid execution anchor still resolves `agent_key` from frame roster.
- Roster test asserts default-enabled agents, caller `extra_companions`, self exclusion and duplicate de-duplication.
- Companion launch test asserts selected `agent_key` binds child ProjectAgent identity and selected preset facts.
- Frontend hint test or typecheck asserts UI examples use `preset_name`/canonical key, not display-only label.

## Workspace Module Agent Surface

Canvas、Extension 和平台内嵌 workspace 能力面向 Agent 统一通过 `workspace_module` capability 暴露。Canvas 仍保留自己的 domain、repository、VFS provider 与 panel runtime；`workspace_module` 只承担 Agent-facing lifecycle、operation schema、invoke routing 和 presentation facade。

### 1. Scope / Trigger

- Trigger: Canvas 的创建、绑定和展示入口收束到 workspace module，避免 Agent 同时学习 `canvas` 与 `workspace_module` 两套工具面。
- Scope: capability catalog、ToolCluster 映射、默认 session plan、tool provider 注入、ProjectAgent capability directive roundtrip。

### 2. Signatures

- `workspace_module_create(kind="canvas", input={ canvas_id?: string, title?: string, description?: string })`
- `workspace_module_list()`
- `workspace_module_describe(module_id: string)`
- `workspace_module_invoke(module_id: string, operation_key: string, input: object)`
- `workspace_module_present(module_id: string, view_key: string)`

### 3. Contracts

- `workspace_module` 是 Canvas Agent 操作的 well-known capability key。
- 默认 Agent 工具面包含 `workspace_module_create/list/describe/invoke/present`。
- 已创建 Canvas 表达为 `canvas:{mount_id}` module。
- Canvas binding 表达为实例 operation：`operation_key="canvas.bind_data"`。
- Canvas presentation 表达为 UI entry：`presentation_uri="canvas://{mount_id}"`。
- Canvas 编辑 mount 表达为 VFS URI：`cvs-<mount_id>://...`。
- ProjectAgent preset 中保存的 `canvas` capability directive 只作为 forward migration 输入；运行态普通 Agent capability 不再以 `canvas` 作为主入口。

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| `kind` 不支持 | tool validation error |
| `module_id` 不在当前 session 可见 module projection | NotFound / Forbidden |
| `operation_key` 不在 describe 返回的 operations 中 | BadRequest |
| Canvas bind input 不满足 operation schema | BadRequest |
| `view_key` 不在 describe 返回的 UI entries 中 | NotFound |
| `presentation_uri` 不是 renderer 可打开 URI | backend contract/test failure |

### 5. Good/Base/Bad Cases

- Good: `workspace_module_create(kind="canvas")` 返回 `canvas:{mount_id}`，随后 `workspace_module_describe` 能看到 `canvas.bind_data` 与 `preview` UI entry。
- Base: 已存在 Canvas 通过 `workspace_module_list -> describe -> present` 打开，不需要重新创建。
- Bad: Agent-facing catalog 同时暴露独立 Canvas capability 与 workspace module capability，导致同一 Canvas 实例有两套入口。

### 6. Tests Required

- Capability catalog test asserts `workspace_module` contains create/list/describe/invoke/present.
- Provider/tool-plan test asserts default session tool surface uses workspace module tools for Canvas workflows.
- Migration guard asserts persisted ProjectAgent `canvas` directives become `workspace_module`.
- Policy test asserts tool-level filtering still gates each `workspace_module_*` tool via `CapabilityState.tool_policy`.

### 7. Wrong vs Correct

#### Wrong

```text
top-level Canvas capability + separate workspace module capability for the same asset
```

#### Correct

```text
workspace_module_create(kind="canvas")
workspace_module_describe(module_id="canvas:{mount_id}")
workspace_module_invoke(module_id="canvas:{mount_id}", operation_key="canvas.bind_data", input={...})
workspace_module_present(module_id="canvas:{mount_id}", view_key="preview")
```

## Task Runtime Tool Surface

### 1. Scope / Trigger

- Trigger: AgentRun 会话需要直接维护 `LifecycleRun.tasks`，并让 Story projection 消费同一事实源。
- Scope: SPI capability key、ToolCluster、runtime provider、tool schema、CapabilityCatalog、generated TS。

### 2. Signatures

```rust
pub const CAP_TASK: &str = "task";
pub enum ToolCluster { Task, /* ... */ }
pub const CLUSTER_TASK_TOOLS: &[&str] = &["task_read", "task_write"];
```

```text
task_read(mode = overview | list | detail | context | execution | projection, ...)
task_write(mode = patch | snapshot, operations[], snapshot[], return_mode = ...)
```

### 3. Contracts

- `task` 是 cluster-based runtime capability，不映射平台 MCP scope。
- `task_read` 是唯一读取入口，mode 覆盖 overview/list/detail/context/execution/projection。
- `task_write` 是唯一写入口，patch operations 覆盖 create/patch/status/reorder/drop/context refs；snapshot 写入同一组 Task facts。
- 写入事实源固定为 `LifecycleRun.tasks`；Story 只读取 projection。
- Companion 派发如携带 `payload.task_id`，由 companion 工具读取 Task context 并写回 `assigned_agent_id`。

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| 当前 runtime session 缺少 execution anchor | `task_read/task_write` 构建失败 |
| `run_id` 不属于当前 project | tool execution failed |
| `task_id` 不属于目标 run | invalid arguments / not found |
| context ref enum 值未知 | invalid arguments |
| snapshot `drop_missing=true` | 未出现在 snapshot 的旧 Task 软归档为 dropped |

### 5. Good/Base/Bad Cases

- Good: Story-bound AgentRun 调用 `task_write` 创建 Task，AgentRun workspace 读回，Story projection 解释来源。
- Base: 普通 Project AgentRun 调用 `task_read overview` 只看到当前 run scope 的 Task。
- Bad: Task runtime tools 通过平台 MCP Task scope 注入，导致 Task 读写入口分散。

### 6. Tests Required

- Capability mapping test asserts `task -> ToolCluster::Task -> task_read/task_write`。
- Runtime provider test or focused check asserts capability filter gates both tools。
- Task write tests cover create/status/reorder/drop/context refs。
- Companion sub dispatch test covers `payload.task_id` adds Task context and writes `assigned_agent_id`。

### 7. Wrong vs Correct

```text
Wrong: platform MCP Task scope exposes separate Task CRUD/status tools.
Correct: task runtime capability exposes task_read/task_write and execution artifacts stay in SubjectExecutionView.
```

## 工具 schema 与模型可见说明

运行时工具更新必须同时维护两条链路：

- Provider `tools[]` 携带完整机器 schema，用于 OpenAI/Codex Responses 等服务解析工具调用。
- `tool_schema_delta` 的模型可见文本携带可调用说明，用工具名、用途、来源、参数名、必填性、类型和关键嵌套字段摘要指导模型调用。

模型可见文本禁止直接 dump 完整 pretty JSON Schema。复杂工具应输出结构化参数摘要，并依赖 provider
`tools[]` 保留完整机器契约。

进入 Responses API 的工具 schema 必须先经过 sanitizer：递归内联本地 `$ref`，移除 `$defs` /
`definitions` 与装饰性关键字，确保 object/array 结构、nullable 与组合器表达在目标 provider
可解析的 JSON Schema 子集内。

sanitizer 必须保留来源 schema 的 `required` 语义，原因是模型可见参数摘要和运行时参数校验都依赖同一份机器契约；可省略参数应在 schema 中保持 optional，让 Agent 在短文件读取、默认搜索等场景只提供真正必要的输入。

## CapabilityResolver

- 协议类型：`agentdash-spi/src/tool_capability.rs`
- Resolver 实现：`agentdash-application/src/capability/resolver.rs`
- 纯函数式设计，所有依赖通过 input 传入

## 调用规范

### 添加新 session 类型时

必须通过 `CapabilityResolver::resolve()` 获取工具集，禁止直接构造 `CapabilityState`。

### 添加新平台能力时

1. 在 `tool_capability.rs` 中添加 well-known key 常量 + 更新 `WELL_KNOWN_KEYS`
2. 在 `capability_to_tool_clusters()` / `capability_to_platform_mcp_scope()` 添加映射
3. 在 `default_visibility_rules()` 添加可见性规则
4. 添加单元测试

## 前端/API Roundtrip 契约

Workflow 与 Lifecycle 编辑链路必须把能力配置当成结构化字段透传：

- Workflow 级权威字段：`WorkflowDefinition.contract.capability_config.tool_directives`
- Lifecycle step 级权威字段：`LifecycleStepDefinition.capability_config.tool_directives`
- 前端 mapper / store / editor 必须在读取、保存和模板 bootstrap 后 roundtrip 不丢字段

### Capability Catalog Projection

前端 capability editor 使用后端 catalog projection，而不是本地镜像 well-known key、label 或
visibility baseline。这样做的原因是 capability visibility、auto-grant 和 tool mapping 都由 SPI
规则决定，前端只负责展示、排序和编辑用户选择。

#### 1. Scope / Trigger

- Trigger: workflow capability editor 需要展示 key、label、description、allowed scopes、auto grant
  和 tools。
- Scope: `agentdash-spi::platform::tool_capability` → application catalog projection →
  `agentdash-contracts::workflow::CapabilityCatalogResponse` → frontend editor。

#### 2. Signatures

```rust
pub fn query_capability_catalog(capability_keys: Option<&[String]>) -> CapabilityCatalogResponse;
pub fn query_tool_catalog(capability_keys: &[String]) -> Vec<ToolDescriptorDto>;
```

#### 3. Contracts

- `CapabilityCatalogResponse.capabilities[]` fields:
  - `key`
  - `label`
  - `description`
  - `allowed_scopes: CapabilityScopeDto[]`
  - `auto_granted`
  - `agent_can_grant`
  - `workflow_can_grant`
  - `tools: ToolDescriptorDto[]`
- `mcp:<preset_key>` entries are projected as non-auto-granted, grantable custom MCP capabilities
  with Project/Story/Task scopes.

#### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| unknown non-MCP key | omitted from catalog projection |
| duplicate requested keys | response contains one entry per key |
| well-known key | metadata and visibility derive from SPI rule |
| `mcp:*` key | response contains placeholder tool descriptor for runtime discovery |

#### 5. Good/Base/Bad Cases

- Good: `workspace_module` entry shows Project/Story/Task scopes and create/list/describe/invoke/present tools.
- Base: editor requests no explicit keys and receives all well-known platform capabilities.
- Bad: editor stores its own auto-granted baseline and diverges from SPI visibility rules.

#### 6. Tests Required

- Backend catalog test asserts `workspace_module` tools and scopes.
- Frontend panel tests assert baseline and labels come from fetched catalog.
- Contract check asserts generated `workflow-contracts.ts` includes capability catalog DTOs.

#### 7. Wrong vs Correct

```text
Wrong: frontend CAP_EDITOR_WELL_KNOWN_KEYS + AUTO_GRANTED_BASELINE decide capability visibility.
Correct: frontend consumes CapabilityCatalogResponse and only stores UI selection state.
```
