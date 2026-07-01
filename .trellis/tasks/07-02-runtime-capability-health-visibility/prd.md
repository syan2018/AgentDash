# 平台声明能力健康状态与不可用提示收束

## Goal

建立统一的“平台声明能力”健康状态与用户可见提示机制。平台已经声明、暴露或承诺可用的能力，如果在启动、发现、探测、连接或调用时被发现不健康，用户应能看到明确状态、影响范围和可操作入口，而不是能力静默缺失或只在日志中留下错误。

本任务不做泛化系统监控，也不收集所有外部依赖故障。只有已经进入平台能力声明面的对象才纳入健康状态。

## Background

- 当前 MCP 工具加载失败存在静默体验：平台声明了相关 MCP 能力或工具表面，但加载/连接失败时，用户可能只看到工具缺失或调用失败。
- 类似问题可能出现在其它“已声明能力”上，例如 runtime 上报的 MCP server、executor、extension action/channel、workspace module operation、hook capability、skill discovery 结果等。
- 项目已有若干局部机制：
  - `runtime_health` 表达 backend 级在线、离线、降级或错误状态。
  - Local Runtime diagnostics 已有 `healthy/degraded/unavailable` 等前端层级状态。
  - `diag!` 用于平台过程诊断，但不应替代用户可见的控制面状态。
  - ContextFrame、skill diagnostics、hook diagnostics、workspace module diagnostics 已能局部表达运行上下文或模块诊断。
- 需要把“声明了但不健康”的能力收束成可消费的状态投影，而不是新增孤立日志或只修某个 MCP 入口。

## Scope Boundary

- In scope：平台能力声明面中的能力项。
  - runtime 注册或 capability payload 声明的能力。
  - Project / Agent / Workflow / Session runtime surface 中声明要暴露的能力。
  - 已进入 tool catalog、capability catalog、extension runtime projection、workspace module projection、ContextFrame capability delta 的能力。
  - 已由用户配置并被平台纳入运行面声明的 MCP server。
- Out of scope：未被平台声明为能力的普通外部故障。
  - 例如任意第三方服务、未配置 provider、用户未启用的 OAuth、未安装的插件、未绑定的 workspace root。
  - 这些故障可以有局部错误处理，但不进入统一 capability health，除非平台已把它们声明为当前可用能力。

## Requirements

### R1. 统一能力健康语义

- 定义统一的 capability health 语义，用于表达”平台声明能力”的当前可用性。
- 用户可见状态 3 档：
  - `ready`：能力可用。
  - `degraded`：能力部分可用，存在非阻断故障。
  - `unavailable`：能力不可用。
- 惰性模型下，未触发交互的能力不主动出现在 health surface；只有经过 probe/connect/call 的能力才进入状态。
- 每个健康项携带：稳定 id、能力域、状态、用户可读名称、摘要、可操作入口。内部诊断元数据（归属、声明来源、severity）不进跨层 contract。

### R2. 健康状态只从声明能力派生

- 不因为某个外部系统理论上可能失败就创建健康项。
- 健康项必须能追溯到平台声明来源，例如 runtime capability、tool catalog、extension projection、workflow capability directive、session runtime surface、workspace module projection。
- 当能力从声明面移除时，对应健康项应被移除或标记为不再适用，不能长期残留为错误。

### R3. MCP 作为首批接入域

- 惰性模型：MCP server 在 probe/list_tools/call_tool 触发交互前不出现在 health surface。
- probe/list_tools/call_tool 成功后进入 `ready`。
- stdio spawn、握手、HTTP/SSE 连接、tools/list、tool call 失败时更新为 `unavailable` 或 `degraded`。
- UI 不能只显示 MCP server 数量；当 MCP server 已进入能力声明面时，应能展示每个 server 的可用状态。

### R4. 首批非 MCP 域保持克制

- 首批非 MCP 接入选择 Runtime/Runner executor。
- Runtime/Runner executor 的声明来源是 runtime 注册能力、relay `CapabilitiesPayload.executors` 与 backend runtime summary 中的 executor surface。
- executor 已声明但 runtime 离线、不可用或不可分配时，应进入同一 capability health 模型，并能被 backend/runtime 选择入口消费。
- Extension runtime projection action/channel 与 Workspace module projection operation 暂不进入首个实施切片；它们保留为后续接入域，接入时必须同样从已声明 projection 派生。
- Desktop API sidecar、OAuth、database migration、marketplace provider 等只有在它们被声明为某个当前能力的依赖并影响该能力可用性时，才通过该能力项体现，不作为独立 capability health 域泛化接入。

### R5. 不用日志替代状态

- `diag!` 和日志继续记录平台过程细节。
- 用户可见 UI 必须消费结构化健康状态，而不是解析日志或只依赖异常文本。
- diagnostics endpoint 可作为排障入口，但不是唯一提示机制。

### R6. 保持惰性能力模型

- 不要求启动时主动连接或探测所有声明能力。
- 启动期可上报声明态和基础 readiness。
- 实际 probe、连接、调用、activate、invoke 等路径应回写能力健康状态。
- 需要主动检查时，应通过明确的 Probe / Doctor / Retry 操作触发，而不是隐式拖慢所有启动流程。

### R7. 前端展示与操作阻断

- Local Runtime / Settings diagnostics 中应能看到声明能力健康快照。
- 与声明能力相关的操作入口应显示相关不可用状态，例如 MCP tool catalog、workflow capability panel、extension tab、workspace module operation 入口等。
- 依赖不可用能力的入口应明确禁用、警告或提示影响范围，不能让用户只能在点击后通过失败结果猜原因。

### R8. Session 内能力不完整提示

- 会话启动或运行上下文刷新时，如果本次 runtime surface 声明的能力存在 `degraded` 或 `unavailable`，Session 侧应有能力不完整提示。
- ContextFrame 可以消费健康事实展示本次会话上下文受影响情况，但不作为全局健康事实源。

## Acceptance Criteria

- [ ] 能力健康项只从平台声明面派生；未声明的外部故障不进入 capability health。
- [ ] MCP probe/list_tools/call_tool 失败后，用户能看到失败 server、错误摘要和可操作入口。
- [ ] Runtime/Runner executor 接入同一健康状态模型。
- [ ] Local Runtime diagnostics 展示声明能力健康列表，按 ready/degraded/unavailable 区分。
- [ ] Session 侧在本次 runtime surface 中有不健康能力时展示 inline notice。
- [ ] 用户可见状态消费结构化 health，不依赖解析日志。
- [ ] 跨层 contract DTO 精简（6 字段），内部诊断元数据不暴露到前端。

## Out Of Scope

- 不做全局系统监控、长期告警、健康趋势或任意外部依赖扫描。
- 不要求启动时自动 probe 全部 MCP server、extension、provider 或 OAuth 连接。
- 不把未声明、未启用、未配置的外部能力显示为健康错误。
- 不引入兼容性回退层来同时支持旧状态模型。

## Notes

- 一个 PR 交付完整设计（MCP + executor）。
- backend 级 `runtime_health` 表达整体连接健康；capability health 是细粒度能力可用性，互不替代。
