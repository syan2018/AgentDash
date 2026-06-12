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
- **用户自定义 MCP key**：`mcp:<server_name>` 格式，引用 agent config 中注册的外部 MCP server

### 平台 well-known 能力映射

| Key | ToolCluster(s) | 平台 MCP Scope | 说明 |
|-----|---------------|---------------|------|
| `file_read` | Read | — | 文件读取 |
| `file_write` | Write | — | 文件写入 |
| `shell_execute` | Execute | — | Shell 命令执行 |
| `workspace_module` | WorkspaceModule | — | Workspace Module 创建、发现、描述、调用与展示；Canvas Agent 入口收束在这里 |
| `workflow` | Workflow | — | Lifecycle node 推进 |
| `collaboration` | Collaboration | — | Companion 协作 |
| `story_management` | — | Story | Story 上下文编排 |
| `task_management` | — | Task | Task 状态与产物管理 |
| `relay_management` | — | Relay | 全局看板/Project 管理 |
| `workflow_management` | — | Workflow | Workflow/Lifecycle CRUD |

### 用户自定义 MCP 能力

格式 `mcp:<server_name>`，Resolver 在 agent config 的 `mcp_servers` 中按 name 查找并注入。

当 `mcp:<server_name>` 命中 Project MCP Preset 时，Resolver 必须通过 `CapabilityResolverInput.mcp_runtime_context` 调用 `resolve_preset_mcp_server()`，产出带 runtime-resolved transport 的 `SessionMcpServer`。这个 context 来自 frame construction final VFS，原因是 custom MCP directive 是运行时 capability projection，不是静态 preset 展示字段。命中 agent 内联 `mcp_servers` 时直接消费已解析的 `SessionMcpServer`，不再按 preset runtime binding 重写。

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
| story_management | — | ✓ | — | ✓ | — | — |
| task_management | — | — | ✓ | ✓ | — | — |
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

## 运行态工具策略

运行态唯一工具级策略字段是 `CapabilityState.tool_policy`。

边界定义：
- `ToolCapabilityDirective`：配置层输入 DSL（workflow/step 的 add/remove 意图）
- `ToolCapabilityReduction`：Resolver 内部归约中间态
- `CapabilityState.tool_policy`：运行态唯一 policy，所有工具暴露层必须消费它

所有工具发现入口必须调用 `capability_state.is_capability_tool_enabled()` 进行 capability-aware 判定。

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
