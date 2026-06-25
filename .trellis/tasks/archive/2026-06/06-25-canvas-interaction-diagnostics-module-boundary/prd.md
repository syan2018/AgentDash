# Canvas 交互诊断与模块边界实现

## Goal

实现 Canvas runtime 的 Agent 可观察与用户交互扩展能力，让 Agent 能围绕当前 Canvas 的真实渲染状态、用户交互状态和 Canvas 内触发的请求进行协作；同时将 Canvas 业务收束进 Workspace Module crate，Canvas 作为 workspace module 的子模块演进。

## User Value

- 用户在 Canvas 中填写表单、选择对象或点击操作按钮后，当前 Agent 能理解这些交互事实，并基于它们继续执行。
- Agent 能诊断用户实际看到的 Canvas 运行状态，包括渲染成功、运行时报错、空白页面、当前视口和关键 DOM 状态。
- Canvas 内按钮可以构造结构化请求并提交给当前 AgentRun 中关联的 Canvas 引用，不需要用户手动复制到聊天输入框。
- Canvas / Workspace Module 的边界归属更清晰，后续 extension、runtime surface、VFS 和 workspace presentation 能复用同一个 Workspace Module 业务边界，而不是继续在 application 层散落增长。

## Confirmed Facts

- Canvas preview 当前由前端 `CanvasRuntimePreview` 以 iframe `srcDoc` 运行，并通过 `postMessage` 支持 runtime action、VFS image asset、extension channel 三类桥接请求。
- Canvas runtime bridge 当前公开 `window.agentdash.invoke(...)`、`window.agentdash.assets.url(...)` 和 extension channel 能力；尚未提供用户交互状态上报或 Agent 输入提交 API。
- AgentRun 用户输入已有 canonical 路径：API 接收 `UserInputBlock`，写入 AgentRun Mailbox，再由 scheduler 决定 launch、queue 或 steer。
- Backbone Protocol 明确用户输入属于 turn/thread 事实，不属于普通 platform metadata。
- AgentFrame runtime surface 已有 Canvas visibility / binding update 收束方向，Canvas 可见性和 VFS/runtime action surface 应继续走 runtime surface 边界。
- Canvas 交互状态的业务归属应落在 AgentRun 到 Canvas 的可见/展示引用上；RuntimeSession 只作为后端 delivery/trace substrate，不进入 Canvas 前端 bridge、Canvas SDK 或 Canvas API 入参。
- Canvas 业务边界应归入 `agentdash-workspace-module::canvas`，不再保留独立 `agentdash-canvas` crate；HTTP route、Postgres adapter、RuntimeGateway 调用等技术适配仍由 API/infrastructure/application adapter 层承接。
- 当前已有活跃 Trellis 任务覆盖 AgentFrame/Canvas projection、AgentRun runtime surface projection、Canvas VFS/runtime binding 收束，本任务需要与这些方向对齐。

## Requirements

- 定义 Canvas render observation 能力，用于记录和查询当前 Canvas iframe 的真实运行状态、诊断摘要、运行错误和可选截图引用。
- 定义 Canvas interaction state 能力，让 Canvas source 能在 AgentRun↔Canvas 引用上显式声明 Agent 可见的表单值、选区、过滤器和近期用户事件。
- 定义 Canvas submit-to-Agent 能力，让 Canvas 内显式用户动作把请求转换为 canonical `UserInputBlock` 并进入对应 AgentRun Mailbox。
- 规划 Agent 可调用的 Canvas workspace module 操作，例如 inspect render state、get interaction state，并明确哪些操作只是查询状态，哪些操作会产生 Agent 输入。
- 将 Workspace Module 拆为独立 crate，并让 Canvas 作为其子模块承载 identity/helper、管理/runtime/VFS/visibility 业务服务和 operation contract；Canvas entity、value、repository contract、runtime state contract 与 embedded skill bundle 继续归属 `agentdash-domain::canvas`。
- 规划必要的 HTTP DTO、generated TypeScript contract、前端 bridge API、后端 application service、repository/migration 和测试覆盖。
- 保持项目未上线前提下的正确状态优先策略；若字段、enum 或 crate 边界需要调整，规划应以收束后的目标状态为准。

## Non-Goals

- MVP 不实现截图 artifact；`screenshot_ref` 仅作为后续增强预留。
- 不设计旧 Canvas 字段、旧 bridge API 或旧 crate 路径的兼容层。
- 不把 Canvas 交互状态自动写入模型历史；只有用户明确提交给 Agent 的请求进入 Mailbox。

## Acceptance Criteria

- [x] `agentdash-workspace-module::canvas` 承载 Canvas mount id、VFS mount id、module id、presentation URI、operation key 与 Canvas 管理/runtime/VFS/visibility 业务服务；Canvas entity/value/repository/runtime state contract 保留在 `agentdash-domain::canvas`，不保留独立 `agentdash-canvas` crate。
- [x] iframe runtime 上报 ready/error、viewport、DOM 摘要、root 是否空白、关键文本 preview 和 console/runtime diagnostics，父页面校验 `frame_id/generation` 后写入 AgentRun→Canvas latest observation。
- [x] Canvas SDK 提供 `window.agentdash.interaction.setState/clearState/emit/getState`，并将 latest interaction snapshot 写入 AgentRun→Canvas reference。
- [x] Agent/workspace module 操作支持 `canvas.inspect_render_state` 与 `canvas.get_interaction_state`，只查询 latest facts，不自动进入模型历史。
- [x] Canvas SDK 提供 `window.agentdash.agent.submit(...)`，后端 AgentRun-scoped route 将请求转换为 canonical `UserInputBlock` 并进入 `AgentRunMailboxService.accept_user_message`。
- [x] `MailboxMessageSource::CanvasAction` 与 generated TypeScript contract 同步更新，submit response 使用现有 `AgentRunMessageCommandResponse`。
- [ ] 普通 Canvas tab、extension `canvas_panel`、runtime action bridge、VFS asset bridge、AgentRun mailbox 展示协同工作；extension `canvas_panel` 缺少 live AgentRun bridge 时明确展示 bridge unavailable。
- [x] `window.agentdash.invoke(...)` 继续只代表 RuntimeGateway action，不承担 Agent 输入语义。

## Execution Notes

- 本任务按主任务直接推进，不再创建 Trellis child task。
- 数据库保存 AgentRun→Canvas latest state；后端可以在写入时记录当前 delivery trace ref 作为诊断字段，但 Canvas 前端与 iframe SDK 不传入 `sessionId`。
- Workspace Module 与运行中 Agent 的协作端口以 AgentRun bridge 命名；runtime session 只作为 API/application adapter 内部解析当前 delivery runtime 的 trace 坐标。
