# 收束 Workflow Runtime Context 单一路径

## 背景

`complete_lifecycle_node` 推进 Workflow/Lifecycle phase 后，系统会同时触发多类运行期变化：

- active workflow step / effective contract 改变；
- tool capability / MCP / VFS capability surface 改变；
- `CapabilityChanged` hook 重新计算 injection；
- `SessionContextBundle.turn_delta` 需要记录运行期增量；
- live Agent 需要在当前 running turn 中实际收到动态 steering / notification；
- session profile 与持久化模板需要保证下一轮不回退。

上一轮修复已经把当前具体 bug 打通，但暴露出更深的问题：这些路径散落在 orchestrator、step activation、session hub、hook dispatch、prompt pipeline、connector routing、前端模板编辑等多个位置，缺少一个可命名、可测试、可审计的统一事务边界。只要任一入口只更新其中一部分，就会再次出现“trace 看起来生效，但 Agent 实际没收到”“当前 turn 生效但下一轮回退”“模板展示和运行契约字段漂移”等问题。

## 目标

1. 收束 workflow phase/context/tool/hook 变化的统一数据流，避免各实现点自行拼接半条路径。
2. 明确命名边界：先盘清现有 `CapabilitySurface` / `runtime_surface` / `RuntimeSurface` 等概念是否占据了错误含义，再为真正的运行期上下文过渡模型命名。
3. 明确 system prompt 的边界：运行期动态内容不得通过重设 system prompt 表达，必须走 Bundle `turn_delta`、steering、pending action 或 live notification 等动态输入面。
4. 补齐 builtin workflow template 的持久化/回放/前后端契约防线，避免编辑器、模板、仓储各自理解字段。
5. 建立端到端测试，证明真实 Agent 在 plan → apply 后能拿到新工具、新 hook 指令和新上下文，而不仅是后端状态对象更新。

## 非目标

- 不为旧 API、旧数据库字段或旧模板语义保留兼容层；项目仍处于预研期，应直接迁到最正确状态。
- 不把动态 workflow/hook 内容塞回 system prompt 来换取短期可见性。
- 不把所有 connector 一次性改造成同等 Bundle 原生消费者；但统一数据流必须给每类 connector 明确消费边界。

## 命名收束

### 问题

`RuntimeSurface` 这个词本身过宽：它既可能被理解为 UI/debug 展示面，也可能被理解为 VFS resolved surface，还可能被误用成“当前 Agent 完整运行上下文”。如果已有结构占用了错误名称，应先重命名旧结构，把更准确的名称腾出来，而不是继续叠加含混名词。

### 候选分层

- `ToolAccessSurface` / `SessionToolSurface`：仅描述工具、MCP、FlowCapabilities、excluded tools 等可调用工具面。
- `CapabilitySurface`：保留给由 `CapabilityConfig` 解析出的多维生效表面，当前至少覆盖 tool/MCP/VFS，未来可扩展 context/policy/resource budget。
- `RuntimeContextTransition` / `ActiveContextTransition`：描述一次 workflow phase 推进导致的运行期上下文事务，包含 active workflow、effective contract、surface delta、hook resolution、Bundle delta、live delivery 结果。
- `ResolvedVfsSurface`：仅用于 VFS resolved view/API surface，不应泛化为 Agent 全运行态。

### 决策要求

- 先通过代码搜索列出所有 `surface` 命名的实际语义，再决定保留、重命名或迁移。
- 如果某个类型只描述工具能力，不应叫 `RuntimeSurface` 或完整 `CapabilitySurface`。
- 如果某个事件表达 phase/context/tool/hook 的组合变化，应使用 transition/change 语义，而不是单纯 surface 语义。

## 目标数据流

一次 PhaseNode 激活或 lifecycle node 推进应走同一条事务路径：

```text
Lifecycle advance / Phase activation
→ 解析 active workflow step 与 effective contract
→ 计算目标 CapabilitySurface 与 surface delta
→ 原子应用到 SessionRuntime：
  - active turn session_frame
  - session_profile
  - pending transitions / durable event
→ 刷新 HookSessionRuntime snapshot
→ 触发统一 hook evaluation（CapabilityChanged 或更准确命名）
→ HookResolution.injections 进入 SessionRuntimeHookInjectionSink
→ Bundle.turn_delta + ContextAuditBus
→ 需要当前 running turn 立即消费的内容额外转成 live notification / steering / pending action
→ 顶层 connector 路由到真实 live child connector
→ 前端事件流展示同一份结构化结果
```

## 需要统一的散落点

- `workflow/orchestrator.rs` 与 `workflow/step_activation.rs`：phase 激活、effective contract、运行期 target surface。
- `session/hub/tool_builder.rs`：surface apply、session profile 同步、event payload。
- `session/hub/hook_dispatch.rs`：out-of-band hook trigger、trace、live notification。
- `session/hook_delegate.rs`：HookResolution injection → Bundle turn_delta 的唯一 sink。
- `session/prompt_pipeline.rs`：下一轮 prompt 对 pending transition、session profile、Bundle 的消费。
- connector routing：`CompositeConnector` 必须把 live notification/update 路由到真正持有 session 的 connector。
- 前端 workflow editor/API DTO：只能使用 `contract.capability_config.tool_directives`，不得恢复旧 `capability_directives`。

## Builtin 模板与仓储收束

本次 bug 还暴露出 builtin template 和项目内已持久化副本之间可能漂移：

- builtin seed 更新后，已存在 Project 的 workflow/lifecycle 定义不一定同步；
- 前端编辑器曾使用旧字段，导致保存后的定义丢失能力配置；
- template、bootstrap、repository、editor 之间缺少 roundtrip 测试。

后续需要明确：

- builtin template 是否应带版本/checksum；
- bootstrap 是否允许覆盖项目中同 key 的 builtin-managed 定义；
- UI 是否能提示“项目内定义落后于内建模板”；
- workflow definition roundtrip 是否覆盖 `capability_config.tool_directives`。

## 验收标准

- 有一个统一的 apply/transition 入口或等价服务，phase/context/tool/hook 变化不再由多个调用点各自实现半条路径。
- 代码命名清晰区分 VFS surface、tool access surface、capability surface、runtime context transition。
- 任意 Workflow phase 切换后：
  - active step/effective contract 已更新；
  - hook snapshot 基于新 step 重新加载；
  - Bundle `turn_delta` 有结构化 fragment；
  - live Agent 收到动态 notification/steering；
  - session profile 不回退；
  - 前端事件流展示与实际运行一致。
- builtin workflow admin 的 Plan 阶段只暴露读取工具，Apply 阶段才暴露写入工具。
- 测试覆盖 plan → apply 的真实链路，至少包含 fake connector 或集成级断言：工具列表、hook injection、turn_delta、live notification、事件流不重复。

## 建议实施顺序

1. 先完成命名盘点，形成 rename map：哪些保留，哪些重命名，哪些迁入 transition 语义。
2. 抽出 workflow runtime context transition 的统一服务/入口，收口现有散落调用点。
3. 补齐 builtin template 版本/回放/roundtrip 防线。
4. 用端到端测试覆盖真实 plan → apply 切换，断言 Agent 运行面而不只是仓储状态。
5. 更新 `.trellis/spec/backend/*` 中的 capability、hook、bundle、workflow 文档，确保未来实现沿同一路径扩展。
