# 收口 Workflow 工具级能力裁剪链路

## 背景

内嵌 `builtin_workflow_admin` 设计为 Plan → Apply 两阶段：

- Plan 阶段允许查看 Workflow/Lifecycle 定义，但不得调用 `upsert_workflow_tool` / `upsert_lifecycle_tool`。
- Apply 阶段在同一会话内切换工具表面，开放 upsert 工具执行实际写入。

当前内置 workflow JSON 已正确声明：

- Plan：`add workflow_management`，并 `remove workflow_management::upsert_workflow_tool` / `remove workflow_management::upsert_lifecycle_tool`
- Apply：`add workflow_management`

但实际会话中 Plan 阶段仍会向 Agent 暴露 upsert 工具；Plan → Apply 后的提示也只显示 capability 变化或 effective capabilities，没有展示具体工具差异。

## 问题链路

1. `ToolCapabilityDirective`、`ToolCapabilityReduction`、历史 `FlowCapabilities.excluded_tools` 等结构在不同层表达相近含义，导致“配置指令”“归约中间态”“运行态工具表面”边界不清。
2. `ToolCapabilityDirective` 归约时保留了工具级排除关系，但历史 `FlowCapabilities.excluded_tools` 是扁平工具名集合，丢失 capability/server 维度。
3. 内建 runtime tools 会消费扁平过滤，平台 MCP / 自定义 MCP 的 `discover_mcp_tools` / `discover_relay_mcp_tools` 不消费工具级过滤。
4. Workflow MCP server 一旦按 `workflow_management` 注入，就通过 `tools/list` 暴露全部工具，包含 upsert。
5. Capability Update Markdown 只按 capability key delta 生成，不展示运行态 tool surface 的变化。
6. 后端已持久化 `capability_surface_changed` 结构化事件，但前端系统事件白名单未展示该事件，用户无法看到真实工具表面变化。

## 目标

- 工具级 directive 必须在端到端工具 schema 暴露层生效。
- Plan 阶段 `workflow_management::upsert_*` 的 remove 必须真正移除 Agent 可见工具。
- Apply 阶段必须重新开放上述 upsert 工具。
- 能力更新提示必须表达工具级变化，避免“工具 schema 已同步”但不说明具体变化。
- system prompt 不再渲染 `Available Tools` 或工具参数摘要；可读工具 schema
  告知统一走 runtime notice，并在初始化与能力更新时使用同一套完整 schema
  formatter。
- 前端会话流必须能展示 `capability_surface_changed` 的关键工具表面 diff。
- 消除多份结构描述同一运行态工具表面的隐患：directive 只做输入，运行态只保留一个 canonical tool policy。

## 非目标

- 不引入兼容旧字段或回退逻辑。
- 不调整数据库 schema。
- 不重做 workflow editor 的完整能力配置 UI。
- 不改变 `ToolCapabilityPath` 的 JSON 语法。

## 方案

### 1. 明确三层边界

- `ToolCapabilityDirective`：配置层输入 DSL，表达 workflow/step 想要 add/remove 的动作。
- `ToolCapabilityReduction`：Resolver 内部归约中间态，不跨模块暴露为运行态。
- `FlowCapabilities.tool_filters`：运行态唯一工具过滤策略，按 capability key 分组回答“某 tool 是否可见”。

禁止再新增与 `tool_filters` 并行的 `excluded_tools` / `included_tools` / `*_paths` 字段作为状态存储。事件和 UI 所需 path 列表必须从 `tool_filters` 派生。

### 2. FlowCapabilities 使用 canonical tool policy

`FlowCapabilities` 保留：

```rust
enabled_clusters: BTreeSet<ToolCluster>
tool_filters: BTreeMap<String, ToolCapabilityFilter>
effective_capabilities: BTreeSet<ToolCapability>
```

`ToolCapabilityFilter` 表示单个 capability 下的工具策略：

```rust
include_only: BTreeSet<String>
exclude: BTreeSet<String>
```

### 3. Resolver 只输出 canonical policy

`CapabilityResolver` 从 `ToolCapabilityReduction` 编译出 `tool_filters`：

- `Add(cap::tool)` → `include_only += tool`
- `Remove(cap::tool)` → `exclude += tool`
- `Add(cap)` / `Remove(cap)` 仍影响 `effective_capabilities`

### 4. Runtime tools 和 MCP tools 统一消费 filtering helper

所有工具注入点都调用 `FlowCapabilities::is_capability_tool_enabled(capability, tool, cluster)`：

- `file_read::{mounts_list,fs_read,fs_glob,fs_grep}`
- `file_write::fs_apply_patch`
- `shell_execute::shell_exec`
- `workflow::complete_lifecycle_node`
- `collaboration::*`
- `canvas::*`

MCP 工具发现阶段在 namespacing 前根据 server → capability key 过滤：

- `agentdash-workflow-tools` → `workflow_management`
- `agentdash-relay-tools` → `relay_management`
- `agentdash-story-tools` → `story_management`
- `agentdash-task-tools` → `task_management`
- 其他 server → `mcp:<server_name>`

`workflow_management::upsert_workflow_tool` 应匹配 MCP 原始工具名，而不是 agent-facing namespaced name。

### 5. 通知展示工具表面 diff

Capability Update Markdown 保留 capability 段落，同时从 canonical `tool_filters` 派生工具表面段落：

- Newly available tools：从 excluded paths 中移除的工具
- Newly blocked tools：新增到 excluded paths 的工具
- MCP server 变化
- 当前仍被屏蔽的工具路径

无具体工具变化时，不再写会误导的“工具 schema 已同步更新，可直接调用上述能力”。

### 6. 前端展示结构化事件

前端将 `capability_surface_changed` 纳入系统事件白名单，并在系统事件卡片中展示：

- surface 是否变化
- capability added/removed
- newly available / newly blocked tool paths
- MCP servers

## 验收标准

- [x] Plan 阶段最终 Agent-facing 工具列表不包含：
  - `mcp_agentdash_workflow_tools_upsert_workflow_tool`
  - `mcp_agentdash_workflow_tools_upsert_lifecycle_tool`
- [x] Apply 阶段最终 Agent-facing 工具列表包含上述两个工具。
- [x] `CapabilityResolver` 测试覆盖 `tool_filters`，避免只断言扁平工具名。
- [x] MCP direct / relay discovery 测试覆盖工具级过滤。
- [x] Capability Update 测试覆盖“工具级变化但 capability key 不变”的提示。
- [x] 前端系统事件 guard / card 展示 `capability_surface_changed` 的关键 diff。
- [x] `cargo test` 覆盖相关 Rust crate 测试；前端相关检查通过。

## 实施结果

- 运行态工具级策略收敛为 `FlowCapabilities.tool_filters`，并新增
  `ToolCapabilityFilter { include_only, exclude }`。
- `ToolCapabilityDirective` 与 `ToolCapabilityReduction` 保留为配置输入和 Resolver
  内部中间态，不再作为运行态工具表面被消费。
- 本地 runtime tools、直连 MCP discovery、Relay MCP discovery 统一调用
  `is_capability_tool_enabled(capability, tool, cluster)`。
- MCP / 平台 MCP 工具没有 cluster 兜底；即使 session bootstrap 或 agent preset 已
  传入 MCP server，`CapabilityState.capabilities` 未授予对应 capability 时也不得发现工具。
- Owner bootstrap 的回归测试覆盖 `builtin_workflow_admin` Plan 阶段初始 state：
  `workflow_management` 只读工具可见，`upsert_workflow_tool` /
  `upsert_lifecycle_tool` 在流转到 Apply 前不可见。
- `Capability Update` Markdown 展示 tool path diff，不再输出无条件的“工具 schema 已同步更新”固定文案。
- PiAgent 的 system prompt 已移除 `## Available Tools` 工具说明段落；工具名称仅
  用于 skill 提示派生，不再作为工具 schema 的第二份上下文副本。
- 新增 runtime tool schema notice：owner bootstrap 初始化时注入当前
  `assembled_tools` 的完整 `ToolDefinition` schema；workflow/capability
  transition 后用同一 formatter 注入切换后的完整当前 schema。
- Provider request 的 `tools` 字段继续作为 tool/function calling 协议字段存在，
  但可读告知不再从 system prompt 或 capability update 文案各自拼一份，避免
  “文案显示解锁但模型早已见过 schema”的错觉。
- `capability_surface_changed` 事件进入前端可见系统事件，并展示 capability / tool path /
  MCP server 的结构化变化。
- Hook 生命周期触发点已收敛为 AgentLoop 核心节点；能力变化不再占用
  `HookTraceTrigger`，phase/capability 变化改为 `runtime_context_update`
  runtime event，通过 `HookTurnStartNotice` 在下一次 `UserPromptSubmit` 边界注入。
- Companion 回流不再作为 HookTrace；改为 `companion_result`
  runtime event，hook rule 仍可评估并生成 `HookPendingAction`，但消息消费统一走
  TurnStart 暂存事件队列。
- `HookTurnStartNotice` 与 `HookPendingAction` 明确区分：前者一次性告知并消费即清，
  后者是可处置事件，保留 pending/resolved 与 adopted/dismissed 状态，并可参与
  `BeforeStop` gate。

## 相关规范

- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/quality-guidelines.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/quality-guidelines.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
