# Lifecycle Journey VFS

## 目标

把 workflow lifecycle 中的 journey 暴露为统一 VFS 下的可浏览、部分可编辑的数据空间。Agent 和其它通用 VFS 消费者应能通过 `lifecycle://...` 文件系统视图读取当前 lifecycle node/session 以及跨生命周期历史上下文，包括 step 状态、session turn、tool call、终端输出、写入记录和人工/Agent 记录。

## 背景

旧任务 `04-15-workflow-dynamic-lifecycle-context` 已归档到 `.trellis/tasks/archive/2026-05/04-15-workflow-dynamic-lifecycle-context`。旧 PRD 的核心判断仍然成立：

* workflow locator 必须指向真实可读的 VFS 资源，不能回到 `context://execution_context` 这类语义型 key。
* resolver 应保持通用，只负责 URI 解析和读取，不理解 checklist、journal、review 等业务语义。
* 动态上下文应由 lifecycle、hook、workflow runtime 物化，workflow 只声明式引用。

但旧 PRD 的具体方案已经不适合原样推进。过去三周多的补充开发后，项目已经形成更完整的能力表面和 VFS 基础：

* `lifecycle_vfs` 已能暴露 `active`、`artifacts`、`nodes/{step_key}/session/*`、`runs`。
* `session_events` 已成为 session 历史、tool call、terminal delta 的 raw truth source；MCP 调用在 journey VFS 中也应视为 tool call 的一种，而不是另起一套分类。
* `CapabilitySurface` 已覆盖 FlowCapabilities、MCP、VFS/mount，并通过 `capability_config.mount_directives` 让 workflow step 能声明资源表面。
* 上层消费者已经遵循通用 VFS URI 读取规则，因此只要 lifecycle projection 提供稳定 URI，就不需要额外的数据通道或特殊绑定协议。
* 代码中已有多处 lifecycle mount 注入入口，但需要把“当前 agent 实际能看到 lifecycle mount”作为本任务首要验收：`SessionAssemblyBuilder::append_lifecycle_mount` 只在已有 VFS 时追加，`apply_lifecycle_activation` 当前会使用 `activation.lifecycle_vfs`，Task 运行时装配也只在启用 VFS 的 cloud-native 路径追加 active workflow mount。

因此新任务不再以 “给 locator 加模板变量和 `context/` 目录” 为主线，而是以 “Lifecycle Journey VFS” 为主线。

## 问题陈述

当前 lifecycle run 的上下文已经部分进入 VFS，但还没有成为完整的 journey 空间：

* `session_events` 中的 tool call、terminal output、写入记录仍需要上层知道事件结构才能使用。
* Agent 可以读取 `nodes/{step}/session/turns`，但对于当前正在执行的 node/session，路径过深且不够自然；当前 node 的投影应直接暴露在 lifecycle mount 根目录下。
* 通用 VFS 消费者缺少稳定的 lifecycle tool result URI，导致上层只能绑定粗粒度 turn 或自行理解事件结构。
* lifecycle mount 虽然已有空间模型，但需要确认所有 lifecycle agent session 都会自动绑定该 mount，而不是只在部分 Task/Workflow 入口可见。
* 可写边界还停留在 `artifacts/{port_key}`，缺少用于人工备注、Agent 记录、阶段结论的 overlay 空间。
* 旧 PRD 提议的 `context/<name>` 容易重新变成语义垃圾桶，无法表达 raw event projection、derived index、writable overlay 的边界。

## 设计原则

* 统一 VFS 优先：所有访问都走 `lifecycle://...` mount URI，不新增私有读取链路或 workflow 私有 resolver。
* Raw history 不可写：session event、turn、tool call、terminal/MCP 原始历史只读投影，不能被 overlay 或人工编辑污染。
* 当前 node 根目录优先：`session/*`、`tool-calls/*`、`writes`、`records/*` 默认指向当前正在执行的 node/session；`nodes/{node_key}` 仅用于浏览指定历史或非当前节点。
* 派生索引可重复生成：`tool-calls`、`writes`、terminal output 是从 `session_events` 派生的 VFS projection，不是新的事实源。
* MCP 是 tool：MCP 调用进入 `tool-calls` 索引，通过 kind/provider/name 字段区分，不提供独立 `mcp-calls` 路径族。
* 可写内容单独分层：人工备注、Agent 记录、阶段结论进入 `records/` 或明确 overlay 容器，不写回 raw session history。
* CapabilitySurface 对齐：lifecycle mount 的可见性、可写范围和未来 context overlay 都应进入 workflow `capability_config` / `mount_directives` / surface diff，而不是散落在工具代码里。
* 自动绑定必须可验证：处于 lifecycle node 内的 agent 通过标准 `mounts.list` / VFS surface 应能看到 `lifecycle` mount；热切换、续跑、Task owner、Workflow orchestrator 等入口不能各自漂移。

## 数据模型

### 1. Raw Event Projection

直接从 `session_events` 投影，保持只读。用途是审计、回放、精确恢复和问题排查。

典型路径：

```text
lifecycle://session/turns
lifecycle://session/turns/{turn_id}/events.json
lifecycle://session/events.json

lifecycle://nodes/{node_key}/session/turns
lifecycle://nodes/{node_key}/session/turns/{turn_id}/events.json
lifecycle://nodes/{node_key}/session/events.json
```

### 2. Derived Index Projection

从 raw events 派生出更适合 Agent 和通用 VFS 消费者使用的索引。索引内容可以随代码重算，不承担事实源职责。

典型路径：

```text
lifecycle://tool-calls
lifecycle://tool-calls/{tool_call_id}/raw.json
lifecycle://tool-calls/{tool_call_id}/request.json
lifecycle://tool-calls/{tool_call_id}/result.json
lifecycle://tool-calls/{tool_call_id}/stdout.txt

lifecycle://writes
lifecycle://session/terminal

lifecycle://nodes/{node_key}/session/tool-calls
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/raw.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/request.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/result.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/stdout.txt
```

### 3. Writable Overlay

用于 workflow journey 的补充记录，不覆盖 raw history。

典型路径：

```text
lifecycle://records/{name}
lifecycle://nodes/{node_key}/records/{name}
```

建议持久化到 `InlineFileOwnerKind::LifecycleRun` 下的新容器，例如 `journey_records`。根目录 `records/{name}` 默认绑定当前 node；`nodes/{node_key}/records/{name}` 用于显式访问历史或指定 node。

## 路径契约

首版稳定路径如下：

```text
lifecycle://active
lifecycle://active/steps
lifecycle://active/log

lifecycle://artifacts/{port_key}

lifecycle://state
lifecycle://session/meta
lifecycle://session/summary
lifecycle://session/turns
lifecycle://session/turns/{turn_id}/events.json
lifecycle://session/events.json

lifecycle://tool-calls
lifecycle://tool-calls/{tool_call_id}/raw.json
lifecycle://tool-calls/{tool_call_id}/request.json
lifecycle://tool-calls/{tool_call_id}/result.json
lifecycle://tool-calls/{tool_call_id}/stdout.txt

lifecycle://writes
lifecycle://records/{name}

lifecycle://nodes/{node_key}/state
lifecycle://nodes/{node_key}/session/meta
lifecycle://nodes/{node_key}/session/summary
lifecycle://nodes/{node_key}/session/turns
lifecycle://nodes/{node_key}/session/turns/{turn_id}/events.json

lifecycle://nodes/{node_key}/session/tool-calls
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/raw.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/request.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/result.json
lifecycle://nodes/{node_key}/session/tool-calls/{tool_call_id}/stdout.txt

lifecycle://nodes/{node_key}/session/writes
lifecycle://nodes/{node_key}/records/{name}

lifecycle://runs
lifecycle://runs/{run_id}
```

### 命名约定

* `artifacts/{port_key}`：workflow port 交付路径，继续按 `writable_port_keys` 控制写入。
* `state`：当前 node 的 step state 投影，等价于当前 node 的 `nodes/{node_key}/state`。
* `session/turns/*`：当前 node session 的只读 raw event projection。
* `tool-calls/*`：当前 node session 的 tool-call 索引，供 Agent、context 注入、可视化视图等通用 VFS 消费者直接引用；MCP 调用也在这里表达。
* `writes`：当前 node session 的文件写入、patch、artifact 写入等写操作索引。
* `session/terminal` 或 `stdout.txt`：shell/terminal 类工具的输出聚合视图。
* `records/{name}`：当前 node 的可写 overlay，用于备注、阶段结论、人工修正和 Agent 结构化记录。
* `nodes/{node_key}/...`：指定 node 的历史/显式投影；根目录当前 node 投影是它的便捷别名。

## 范围

### 首版包含

* 更新 lifecycle journey 的 PRD/spec 契约，替代旧动态 context PRD。
* 确认并补齐 lifecycle VFS 自动绑定给 agent 的链路，保证 active lifecycle node session 中 `lifecycle` mount 可见。
* 扩展 `LifecycleMountProvider` 的只读 session projection：
  * `turns/{turn_id}/events.json`
  * `tool-calls`
  * `tool-calls/{tool_call_id}/raw.json`
  * `tool-calls/{tool_call_id}/request.json`
  * `tool-calls/{tool_call_id}/result.json`
  * `tool-calls/{tool_call_id}/stdout.txt`
  * `writes`
* 增加 `records/{name}` 可写 overlay 设计和最小实现。
* 更新 lifecycle mount `directory_hint`，让 Agent 能发现新路径。
* 添加 focused tests，覆盖 list/read/write 边界与通用 VFS URI 读取闭环。

### 暂不包含

* 不实现 locator Mustache 模板。若未来需要，只在 context binding compose 阶段展开，底层 resolver 仍只接受最终 URI。
* 不实现完整前端 Journey Explorer 大 UI。首版只保证 VFS 资源契约，UI 和其它消费者可以随后基于同一 URI 契约做浏览入口。
* 不改写 raw `session_events` schema，除非后续发现已有字段不足以无损表达 tool result。
* 不引入任何专用 lifecycle 数据通道；上层消费者继续使用通用 VFS URI 规则。
* 不把 `records/` 写入 workflow port output，避免交付产物和备注 overlay 混淆。

## 技术方案

### PR1：契约锁定与任务启动

* 归档旧任务。
* 创建新任务和 PRD。
* 对齐 `.trellis/spec/backend/vfs/vfs-access.md` 的 lifecycle VFS 路径契约。

### PR2：只读 Projection

扩展 `crates/agentdash-application/src/vfs/provider_lifecycle.rs`：

* 增加 current node/session helper：根路径默认解析当前 active node 的 session；`nodes/{node_key}` 继续解析指定 node。
* 将 `turns/{turn_id}` 迁移或兼容为 `turns/{turn_id}/events.json`。
* 从 `PersistedSessionEvent.tool_call_id` 和 `session_update_type` 派生 tool call index。
* 对每个 tool call 提供 raw events、request、result、stdout 文本视图。
* MCP 调用不单独建 `mcp-calls`，而是在 `tool-calls` summary 中以 kind/provider 字段区分。
* 基于事件类型提供 `writes` 列表。
* `list()` 返回对应虚拟目录项，并标记 `is_virtual=true`。

### PR2a：Agent 自动挂载检查

在实现 projection 前后都要验证 lifecycle VFS 会自动进入当前 agent 的实际 VFS：

* Task cloud-native 路径：active workflow 存在时，runtime VFS 必须包含 `lifecycle` mount。
* Workflow orchestrator / lifecycle node session：`StepActivation.lifecycle_vfs` 或等价 surface 必须进入 agent turn 的 VFS，而不是只停留在 activation 结构里。
* PhaseNode / pending surface transition / next-turn apply：切换后不应丢失 lifecycle mount。
* Companion 或 follow-up 如果继承父 session VFS，应保留可见 lifecycle mount，除非 slice policy 明确裁剪。
* 增加测试或 debug surface 断言，证明 agent 可以通过标准 `mounts.list` / VFS snapshot 看到 `lifecycle`。

### PR3：Writable Records Overlay

扩展 `LifecycleMountProvider::write_text`：

* 支持 `nodes/{node_key}/records/{name}`。
* 支持根目录 `records/{name}`，默认写入当前 node 的 records。
* 写入 `InlineFileOwnerKind::LifecycleRun` + `journey_records`，path 为 `{node_key}/{name}`。
* `list()` 和 `read_text()` 支持列出和读取 records。
* raw session projection、state、turns、tool-calls 仍不可写。

### PR4：通用消费者闭环与入口

* 添加测试证明 lifecycle projection URI 能通过标准 VFS service 读取。
* 后续前端可在 session timeline tool card / lifecycle browser 中提供复制 URI、打开资源、绑定到其它视图等入口，但不作为首版后端契约的阻塞项。

## 验收标准

* [x] 旧 `04-15-workflow-dynamic-lifecycle-context` 任务已归档，新任务成为当前推进载体。
* [x] active lifecycle node session 中，agent 的实际 VFS surface / `mounts.list` 能看到 `lifecycle` mount。
* [x] `lifecycle://session/turns/{turn_id}/events.json` 能返回当前 node session 中该 turn 的 raw event JSON。
* [x] `lifecycle://nodes/{node}/session/turns/{turn_id}/events.json` 能返回指定 node session 中该 turn 的 raw event JSON。
* [x] `lifecycle://tool-calls` 能列出当前 node session 的 tool call summary，包含 `tool_call_id`、tool kind/name、turn、状态、事件范围。
* [x] MCP 调用作为 tool call 出现在 `tool-calls` 索引中，不提供独立 `mcp-calls` 路径族。
* [x] `tool-calls/{id}/raw.json` 返回该 tool call 的原始事件投影。
* [x] `tool-calls/{id}/request.json` 和 `result.json` 尽可能无损返回请求和结果结构；缺失时给出明确 `NotFound`。
* [x] shell / terminal 类 tool call 至少可读取命令、状态、stdout/stderr 或 output delta 聚合结果。
* [x] `writes` 能索引文件写入、patch、artifact 写入等写操作记录。
* [x] `artifacts/{port_key}` 仍按 `writable_port_keys` 控制写入。
* [x] raw session history、state projection、derived index projection 不可写。
* [x] `records/{name}` 可写入当前 node overlay，并可通过 VFS 读回。
* [x] `nodes/{node}/records/{name}` 可写入指定 node overlay，并可通过 VFS 读回。
* [x] lifecycle tool result URI 能被标准 VFS service 解析和读取，供上层通用消费者复用。
* [x] 新增或更新测试覆盖 VFS list/read/write 和通用 URI 读取闭环。

## 决策记录

**背景**：旧 PRD 试图通过 `context/<name>` 和 locator 模板解决 workflow 动态上下文问题。但项目现在已经有统一 VFS、lifecycle projection、session event persistence 和 CapabilitySurface。继续实现旧方案会把新能力压回“语义 context key”的窄模型里。

**决策**：将任务升级为 Lifecycle Journey VFS。用 raw event projection、derived index projection、writable overlay 三层模型表达 lifecycle 上下文；上层消费者只依赖稳定 URI；locator 模板延后。

**影响**：首版实现更偏基础设施，但会给 Agent、未来 Journey Explorer、可视化视图和 workflow context overlay 共用同一个数据平面，避免后续 UI、工具、context resolver 各自读取一套历史结构。

## 技术备注

关联文件：

* `crates/agentdash-application/src/vfs/provider_lifecycle.rs`
* `crates/agentdash-application/src/vfs/mount.rs`
* `crates/agentdash-application/src/session/persistence.rs`
* `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs`
* `crates/agentdash-infrastructure/src/persistence/sqlite/session_repository.rs`
* `crates/agentdash-application/src/session/continuation.rs`
* `crates/agentdash-agent-protocol/src/backbone/event.rs`

相关规范：

* `.trellis/spec/backend/vfs/vfs-access.md`
* `.trellis/spec/backend/session/session-startup-pipeline.md`
* `.trellis/spec/backend/session/runtime-execution-state.md`
* `.trellis/spec/backend/capability/tool-capability-pipeline.md`
* `.trellis/spec/backend/workflow/lifecycle-edge.md`

待后续补充：

* 前端 Journey Explorer 的交互规格。
* `CapabilitySurfaceDelta` 对 context overlay / record overlay 的表达方式。
* 非 JSON VFS 消费者的类型声明方式。
