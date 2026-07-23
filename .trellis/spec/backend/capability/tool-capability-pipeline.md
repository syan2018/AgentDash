# 工具能力管线（Tool Capability Pipeline）

> Session 工具集的声明式治理规范。

---

## 概述

所有 session（Project / Story / Task）的声明式工具基线由 **CapabilityResolver** 统一计算产出，
不在各 session 创建路径中硬编码 `CapabilityState` 或 `McpInjectionConfig`。运行期最终可见工具面由
AgentRun effective capability/admission 服务在该基线上叠加当前 AgentFrame surface、Grant projection
与 runtime policy 后输出。

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

声明式与 frame surface 内的工具级策略字段是 `CapabilityState.tool_policy`。运行期执行准入由
AgentRun effective capability/admission 服务基于 `CapabilityState.tool_policy` 输出；执行权限由
独立的 AgentRun permission facade 判定。

边界定义：
- `ToolCapabilityDirective`：配置层输入 DSL（workflow/step 的 add/remove 意图）
- `ToolCapabilityReduction`：Resolver 内部归约中间态
- `CapabilityState.tool_policy`：运行态唯一 policy，所有工具暴露层必须消费它

工具 schema 构建使用 AgentRun 输出的 final visible capability view；单个工具执行使用 AgentRun
admission decision。

Bound Agent Surface 的 schema-visible capability projection只服务工具 catalog/materialization，
不是权限事实。工具执行在 `tool.execute` 前分别通过 AgentRun capability admission 与 permission
facade，原因是工具存在性、VFS 可达性和执行授权是不同 owner 的约束，不能合并成同一 projection。

Application tool assembly 必须保留 MCP discovery provenance 并生成 `RuntimeToolSchemaEntry`。
Project MCP 的 `server_name/tool_name/runtime_name/description/parameters_schema` 来自 discovery 结果，
不能依赖平台静态 catalog 事后猜测。`DynAgentTool` 只服务执行，`RuntimeToolSchemaEntry` 服务
`tool_schema_delta`、`ContextFrame.rendered_text` 与 Agent 可见 PromptText；两者在 assembly
边界同源产出，原因是平台需要全局掌握 Agent 实际可见的能力说明。

AgentRun admission bridge 必须从 `RuntimeToolSchemaEntry.capability_key` 和 `tool_path` 构造
`AgentRunAdmissionRequest`。缺少 provenance 的工具不能绕过 AgentRun admission，原因是该工具没有
可审计的 capability ownership，不能被视作已授权的 runtime surface。

## MCP Source Readiness Contract

### 1. Scope / Trigger

- Trigger: MCP 工具发现跨越 AgentFrame surface、relay/backend health、direct HTTP 连接和 model-visible context。source 可用性必须和 MCP source 同步投影，避免工具列表、能力状态与模型提示读取不同事实。
- Scope: `RuntimeMcpServer`、`McpToolDiscovery`、`McpRelayProvider`、tool assembly、capability delta、launch accepted event。

### 2. Signatures

```rust
pub enum RuntimeMcpSourceReadiness {
    Pending,
    Ready { tool_count: usize },
    Unavailable { reason_code: String, message: String },
}

pub struct RuntimeMcpServer {
    pub name: String,
    pub transport: McpTransportConfig,
    pub uses_relay: bool,
    pub readiness: RuntimeMcpSourceReadiness,
}

pub struct McpToolDiscoveryOutcome {
    pub tools: Vec<DiscoveredMcpTool>,
    pub sources: Vec<McpToolSourceOutcome>,
}
```

```rust
pub struct RelayMcpListOutcome {
    pub tools: Vec<RelayMcpToolInfo>,
    pub sources: Vec<RelayMcpSourceOutcome>,
}
```

### 3. Contracts

- `CapabilityState.tool.mcp_servers[]` 是 MCP source set 与 source readiness 的运行态事实源；readiness 属于具体 `RuntimeMcpServer`。
- `RuntimeMcpSourceReadiness::Pending` 表示尚未执行本轮 discovery；序列化时可省略，原因是 pending 是 capability/frame surface 的声明态默认值。
- `Ready.tool_count` 记录该 source discovery 返回的原始工具数；工具级 capability policy 过滤后的 callable/schema 数量由 tool assembly 另行投影。
- `Unavailable.reason_code` 必须是稳定 snake_case code；`message` 面向诊断与模型提示，可包含底层连接/relay 错误摘要。
- `McpToolDiscovery::discover_tool_entries()` 返回 `McpToolDiscoveryOutcome`；每个输入 source 都应产生 `Ready` 或 `Unavailable` outcome，单个 source 失败不终止其它 source discovery。
- `McpRelayProvider::list_relay_tools()` 返回 `RelayMcpListOutcome`；relay backend anchor 缺失、relay 响应错误、意外响应和通信失败都必须形成 source outcome。
- `assemble_tool_surface_for_execution_context()` 只组装 tools/schema/source outcomes；`launch::preparation` 在工具装配后把 source readiness 合并回 `context.session.mcp_servers` 与 `context.turn.capability_state.tool.mcp_servers`，再派生 supervisor state、bootstrap frame 与 accepted capability state。
- model-visible MCP unavailable 提示在 accepted launch events 中 `turn_started` 后提交；payload 使用 `system_message` 且包含 `kind="mcp_source_readiness"` 与结构化 `sources[]`。
- `CapabilityStateDelta.mcp_server_readiness` 是从 `after.tool.mcp_servers[].readiness` 派生的 event/context projection，不是独立状态源。
- 本机 runtime 的 MCP health 变化必须通过 `EventCapabilitiesChanged` 主动上报，原因是云端 relay discovery 依赖 backend registry 中的最新 MCP server/health projection。

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| Direct source transport 不是 HTTP | source outcome = `Unavailable { reason_code: "unsupported_transport" }` |
| Direct HTTP 连接或 list_tools 失败 | source outcome = `Unavailable { reason_code: "connection_failed" }`；继续发现其它 source |
| Relay provider 缺失 | relay source outcome = `Unavailable { reason_code: "relay_provider_missing" }` |
| Relay backend anchor 不可解析 | relay source outcome = `Unavailable { reason_code: "backend_anchor_unavailable" }` |
| Relay list_tools 响应 error | relay source outcome = `Unavailable { reason_code: "list_tools_failed" }` |
| Relay 返回非 list_tools 响应 | relay source outcome = `Unavailable { reason_code: "unexpected_response" }` |
| Relay 通信失败 | relay source outcome = `Unavailable { reason_code: "relay_unreachable" }` |
| Discovery 顶层错误 | 本轮请求的 source outcome = `Unavailable { reason_code: "discovery_failed" }` |

### 5. Good/Base/Bad Cases

- Good: 一个 relay MCP 离线、另一个 direct MCP 在线；tool assembly 暴露 direct tools，capability/context frame 标明离线 relay source。
- Base: 所有 source ready；`RuntimeMcpServer.readiness` 进入 ready，model-visible unavailable 段落为空。
- Bad: 所有 source 都 unavailable；accepted turn 仍提交结构化 readiness notice，模型看到相关工具本轮不可用。

### 6. Tests Required

- Direct discovery test asserts unsupported/failed source yields `McpToolDiscoveryOutcome.sources[].server.readiness=Unavailable` and does not abort the outcome.
- Relay discovery test asserts provider outcome maps to `McpToolDiscoveryOutcome.sources` while filtering tools by capability policy.
- Runtime context transition test asserts initial capability frame renders unavailable source name、reason code 与 message。
- Launch preparation/commit test or focused integration check asserts accepted capability state and committed `system_message` payload use post-assembly readiness.
- Local runtime test or check asserts capability health changes produce `EventCapabilitiesChanged` payload.

### 7. Wrong vs Correct

#### Wrong

```text
capability_state.tool.mcp_servers = [server]
capability_state.tool.<separate readiness list> = ["server failed"]
```

#### Correct

```text
capability_state.tool.mcp_servers = [
  RuntimeMcpServer {
    name: "code-analyzer",
    readiness: Unavailable {
      reason_code: "connection_failed",
      message: "connection refused"
    }
  }
]
```

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
- Owner bootstrap 的 companion roster 进入 `CapabilityState.companion.agents`，并由 initial CAP delta frame 渲染为 `companion_agent_roster_delta` section；这样模型上下文、前端 timeline 和工具执行使用同一份能力状态闭包。
- runtime capability transition 必须把 `SetCompanionAgentRosterEffect` 产生的变化写入 `CapabilityStateDelta.companion_agents`，并由 CAP delta frame 渲染为 `companion_agent_roster_delta` section，原因是动态 roster 变化属于能力事实变化。
- `CompanionRequestTool` 解析 `agent_key` 时必须先在当前 frame roster 中匹配，再按 `project_id + name` 读取 selected ProjectAgent identity。
- companion child launch source 必须携带 selected ProjectAgent id 和 canonical agent_key；child `LifecycleAgent.project_agent_id` 绑定 selected ProjectAgent。这样 frame construction、AgentRun 展示与实际 child executor/capability/VFS/skill facts 使用同一个身份来源。
- companion child frame construction 在 parent slice 上叠加 selected ProjectAgent preset facts：executor config、capability directives、MCP presets、Project VFS mount exposure、skill assets 与 companion return-channel baseline。
- `project_id` 来自 `AgentRunRuntimeTarget -> LifecycleRun` 产品授权坐标；Runtime binding只提供thread/binding identity，不替代Project ownership。
- `AuthorityState.companion.dispatch` 是 roster 投影与 `companion_request(target="sub")` 执行 guard 的上游事实；`AuthorityState.companion.respond` 由 child lineage / gate runtime channel 提供，不依赖 parent dispatch authority。这样禁止发起新 sub companion 不会切断已启动 child 的 `companion_respond` 回流通道。
- `AuthorityState.workspace_module.present`、`AuthorityState.dynamic_workflow.author` 是当前已接入 capability projection 的静态边界：workspace module 展示和 dynamic workflow authoring 默认只面向 main/root ProjectAgent。
- background companion child 默认隐藏 human route；该 guard 当前由 companion tool 根据 child source fail-closed 执行，后续需要在 execution context 携带用户主动进入 companion run 的 provenance 后再统一纳入 Authority projection。

### Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| `collaboration` 未启用 | 不暴露 `companion_request` / `companion_respond` 工具 |
| `companion.dispatch` 未开放 | roster 为空，CAP delta 表达当前无可派发对象，`target=sub` 执行拒绝 |
| roster 为空 | CAP delta 表达当前无可派发对象；带 `agent_key` 的调用返回可用列表为空 |
| `payload.agent_key` 为空 | invalid arguments |
| `payload.agent_key` 不在当前 frame roster | invalid arguments，并列出当前可用 `agent_key` |
| frame roster 指向的 ProjectAgent 不存在 | tool execution failed，说明 frame surface 与 ProjectAgent 存储不一致 |
| delivery runtime session 缺少 execution anchor | tool execution failed，拒绝派发 companion |

### Tests Required

- Capability/frame test asserts `CapabilityState.companion.agents` roundtrip through AgentFrame surface.
- Owner bootstrap test asserts non-empty roster renders initial CAP delta `companion_agent_roster_delta` section with `agent_key` lines.
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

- `workspace_module_operate(operation="canvas.create" | "canvas.attach" | "canvas.copy", input={...})`
- `workspace_module_list()`
- `workspace_module_describe(module_id: string)`
- `workspace_module_invoke(module_id: string, operation_key: string, input: object)`
- `workspace_module_present(module_id: string, view_key: string)`

### 3. Contracts

- `workspace_module` 是 Canvas Agent 操作的 well-known capability key。
- 默认 Agent 工具面包含 `workspace_module_operate/list/describe/invoke/present`。
- 已创建 Canvas 表达为 `canvas:{canvas_mount_id}` module。
- Canvas binding 表达为实例 operation：`operation_key="canvas.bind_data"`；绑定落在当前 AgentRun 的 Canvas mount metadata overlay，不写回 Canvas 源对象；结果在 Canvas runtime 与 `{canvas_mount_id}` mount 中投影为 `bindings/<alias>.<ext>` 只读生成文件，扩展名来自显式 `content_type` 或 `source_uri` 推断。
- Canvas render diagnostics 表达为实例 operation：`operation_key="canvas.inspect"`；调用只读取 AgentRun→Canvas 引用上的 latest runtime observation，不写入模型历史。
- Canvas interaction diagnostics 表达为实例 operation：`operation_key="canvas.get_interaction_state"`；调用只读取 Canvas source 显式上报的 latest interaction snapshot，不写入模型历史。
- Canvas presentation 表达为 UI entry：`presentation_uri="canvas://{canvas_mount_id}"`。
- Canvas 编辑 mount 表达为 VFS URI：`{canvas_mount_id}://...`。
- Extension runtime action operation 由 `RuntimeGateway::surface_for_actor(actor, context)` 的 concrete action descriptor 投影，原因是 action schema、permission policy 与 actor/context support 必须和 Gateway invoke 使用同一事实源。
- Extension runtime projection 在 WorkspaceModule 聚合中只提供 installation/module ownership、UI tab、protocol channel 和权限摘要事实，原因是 Project 安装投影描述资产，不描述当前 session actor 的可执行 action surface。
- `WorkspaceModuleOperation.readiness` 表达 operation 调用就绪诊断，独立于 module visibility 与 renderer loadability，原因是缺少 Gateway、channel transport、runtime backend anchor 或 action catalog entry 时，`list` / `describe` / `present` 仍需要保留可见 module 与 UI entry。
- ProjectAgent preset 中保存的 `canvas` capability directive 只作为 forward migration 输入；运行态普通 Agent capability 不再以 `canvas` 作为主入口。

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| `kind` 不支持 | tool validation error |
| `module_id` 不在当前 session 可见 module projection | NotFound / Forbidden |
| `operation_key` 不在 describe 返回的 operations 中 | BadRequest |
| Canvas bind input 不满足 operation schema | BadRequest |
| Canvas runtime observation 尚未上报 | `canvas.inspect` 返回 `observation=null` |
| Canvas interaction snapshot 尚未上报 | `canvas.get_interaction_state` 返回 `snapshot=null` |
| Extension runtime action 不在当前 Gateway catalog | operation readiness 为 `runtime_action_unavailable` |
| RuntimeGateway / channel transport / runtime backend anchor 缺失 | operation readiness 携带对应结构化诊断，module 可见性不因此改变 |
| `view_key` 不在 describe 返回的 UI entries 中 | NotFound |
| `presentation_uri` 不是 renderer 可打开 URI | backend contract/test failure |

### 5. Reference Cases

- Operate flow: `workspace_module_operate(operation="canvas.create")` 返回 `canvas:{canvas_mount_id}`，随后 `workspace_module_describe` 能看到 `canvas.bind_data` 与 `preview` UI entry。
- Copy flow: `workspace_module_operate(operation="canvas.copy")` 从只读 shared Canvas materialize 新 personal Canvas module，返回的新 descriptor 恢复 source edit operations。
- Diagnostic flow: `workspace_module_describe` 返回 `canvas.inspect` 与 `canvas.get_interaction_state`，Agent 通过 `workspace_module_invoke` 读取 latest Canvas runtime facts。
- Existing Canvas flow: 已存在 Canvas 通过 `workspace_module_list -> describe -> present` 打开。
- Capability catalog: Canvas authoring 归入 workspace module capability，原因是同一 Canvas 实例的 lifecycle、operation 与 presentation 需要共享一条 discoverable module path。

### 6. Tests Required

- Capability catalog test asserts `workspace_module` contains operate/list/describe/invoke/present.
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
workspace_module_operate(operation="canvas.create")
workspace_module_describe(module_id="canvas:{canvas_mount_id}")
workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.bind_data", input={...})
workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.inspect", input={})
workspace_module_invoke(module_id="canvas:{canvas_mount_id}", operation_key="canvas.get_interaction_state", input={})
workspace_module_present(module_id="canvas:{canvas_mount_id}", view_key="preview")
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
task_read(mode = overview | list | detail | context | projection, ...)
task_write(mode = patch | snapshot, operations[], snapshot[], return_mode = ...)
```

### 3. Contracts

- `task` 是 cluster-based runtime capability，不映射平台 MCP scope。
- `task_read` 是 Task plan/context/projection 的读取入口，mode 覆盖 overview/list/detail/context/projection；Task runtime execution evidence 由 `SubjectExecutionView` 统一投影。
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
| snapshot `drop_missing=true` | 未出现在 snapshot 的既有 Task 软归档为 dropped |

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

运行时工具更新从Agent实际接纳的tool definition派生两个消费者不同、identity相同的投影：

- Provider `tools[]` 携带完整机器 schema，用于 OpenAI/Codex Responses 等服务解析工具调用。
- 一个`CapabilityStateDelta` ContextFrame同时携带capability各维度变化与
  `ToolSchemaDelta` added/removed/changed结构化证据，以及完整可读`rendered_text`；Complete Agent
  以`context/system_append`投递该文本，并把精确同一frame发布给平台展示。首次投递是
  empty→current，运行中revision只包含真正变化的section，语义无变化时不生成frame。

可读renderer必须覆盖名称、description、capability/source/path、required/optional、object/array
嵌套、enum/const与schema constraints，并附带无损完整JSON Schema；section中的
`parameters_schema`保持原样，前端可按需展开。
provider bridge只做vendor structured field映射，不追加工具PromptText，原因是ContextFrame owner
需要独立管理context更新、缓存与compaction。Dash从native history恢复当前active surface的append
序列，而不是要求最新surface重放完整schema；这样增量语义、热更新顺序与revoke都由平台控制。

`RuntimeToolDefinition.provenance`是capability/source/tool path/context usage的typed来源：
静态平台工具从统一`ToolDescriptor`注册表取得，动态MCP在discovery时按真实server/tool identity
构造，并随`AgentSurfaceContributionPayload::Tool`无损传入concrete Agent。这样delta identity、
ContextFrame展示和工具执行表都不依赖runtime name前缀或adapter route猜测。

进入 Responses API 的工具 schema 必须先经过 sanitizer：递归内联本地 `$ref`，移除 `$defs` /
`definitions` 与装饰性关键字，确保 object/array 结构、nullable 与组合器表达在目标 provider
可解析的 JSON Schema 子集内。

sanitizer 必须保留来源 schema 的 `required` 语义，原因是模型可见参数摘要和运行时参数校验都依赖同一份机器契约；可省略参数应在 schema 中保持 optional，让 Agent 在短文件读取、默认搜索等场景只提供真正必要的输入。

## CapabilityResolver

- 协议类型：`agentdash-platform-spi/src/tool_capability.rs`
- Resolver 实现：`agentdash-application/src/capability/resolver.rs`
- 纯函数式设计，所有依赖通过 input 传入。
- 输出声明式 / frame construction 基线 `CapabilityState`；AgentRun runtime admission 由对应 application facade 独立完成。

## 调用规范

### 添加新 session 类型时

必须通过 `CapabilityResolver::resolve()` 获取声明式 `CapabilityState` 基线。运行期工具面再通过 AgentRun
effective capability/admission 服务输出。

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
- Scope: `agentdash-platform-spi::platform::tool_capability` → application catalog projection →
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

- Good: `workspace_module` entry shows Project/Story/Task scopes and operate/list/describe/invoke/present tools.
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
