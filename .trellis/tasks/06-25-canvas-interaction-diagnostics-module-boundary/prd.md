# Canvas 交互诊断与模块边界预研

## Goal

评估并规划 Canvas runtime 的 Agent 可观察与用户交互扩展能力，让 Agent 能围绕当前 Canvas 的真实渲染状态、用户交互状态和 Canvas 内触发的请求进行协作；同时评估 Canvas / Workspace Module 后端能力是否应从当前 application/domain/api 分散实现中收束为独立 crate 边界。

## User Value

- 用户在 Canvas 中填写表单、选择对象或点击操作按钮后，当前 Agent 能理解这些交互事实，并基于它们继续执行。
- Agent 能诊断用户实际看到的 Canvas 运行状态，包括渲染成功、运行时报错、空白页面、当前视口和关键 DOM 状态。
- Canvas 内按钮可以构造结构化请求并提交给当前 AgentRun 中关联的 Canvas 引用，不需要用户手动复制到聊天输入框。
- Canvas / Workspace Module 的边界归属更清晰，后续 extension、runtime surface、VFS 和 workspace presentation 能复用窄接口，而不是继续在 application 层散落增长。

## Confirmed Facts

- Canvas preview 当前由前端 `CanvasRuntimePreview` 以 iframe `srcDoc` 运行，并通过 `postMessage` 支持 runtime action、VFS image asset、extension channel 三类桥接请求。
- Canvas runtime bridge 当前公开 `window.agentdash.invoke(...)`、`window.agentdash.assets.url(...)` 和 extension channel 能力；尚未提供用户交互状态上报或 Agent 输入提交 API。
- AgentRun 用户输入已有 canonical 路径：API 接收 `UserInputBlock`，写入 AgentRun Mailbox，再由 scheduler 决定 launch、queue 或 steer。
- Backbone Protocol 明确用户输入属于 turn/thread 事实，不属于普通 platform metadata。
- AgentFrame runtime surface 已有 Canvas visibility / binding update 收束方向，Canvas 可见性和 VFS/runtime action surface 应继续走 runtime surface 边界。
- Canvas 交互状态的业务归属应落在 AgentRun 到 Canvas 的可见/展示引用上；RuntimeSession 只作为 delivery/trace substrate，不作为交互状态事实源。
- 当前已有活跃 Trellis 任务覆盖 AgentFrame/Canvas projection、AgentRun runtime surface projection、Canvas VFS/runtime binding 收束，本任务需要与这些方向对齐。

## Requirements

- 定义 Canvas render observation 能力，用于记录和查询当前 Canvas iframe 的真实运行状态、诊断摘要、运行错误和可选截图引用。
- 定义 Canvas interaction state 能力，让 Canvas source 能在 AgentRun↔Canvas 引用上显式声明 Agent 可见的表单值、选区、过滤器和近期用户事件。
- 定义 Canvas submit-to-Agent 能力，让 Canvas 内显式用户动作把请求转换为 canonical `UserInputBlock` 并进入对应 AgentRun Mailbox。
- 规划 Agent 可调用的 Canvas workspace module 操作，例如 inspect render state、get interaction state，并明确哪些操作只是查询状态，哪些操作会产生 Agent 输入。
- 评估 Canvas / Workspace Module 是否拆为独立 crate，并给出推荐 crate 边界、依赖方向、迁移顺序和验证方式。
- 规划必要的 HTTP DTO、generated TypeScript contract、前端 bridge API、后端 application service、repository/migration 和测试覆盖。
- 保持项目未上线前提下的正确状态优先策略；若字段、enum 或 crate 边界需要调整，规划应以收束后的目标状态为准。

## Non-Goals

- 本 planning task 不直接实现 Canvas bridge、后端 API 或 crate 拆分。
- 本 planning task 不设计旧 Canvas 字段、旧 bridge API 或旧 crate 路径的兼容层。
- 本 planning task 不把 Canvas 交互状态自动写入模型历史；只有用户明确提交给 Agent 的请求进入 Mailbox。

## Acceptance Criteria

- [ ] `design.md` 描述 Canvas render observation、interaction state、submit-to-Agent 三条通道的前后端数据流、事实源和 Agent 消费方式。
- [ ] `design.md` 明确 `window.agentdash.invoke` 与 Canvas submit-to-Agent 的边界差异，并将 Agent 输入路径收束到 AgentRun Mailbox。
- [ ] `design.md` 给出 Canvas / Workspace Module crate 拆分评估，包含推荐拆分、可延后项、依赖方向、迁移风险和数据库/contract 影响。
- [ ] `implement.md` 给出可分阶段执行清单，每个阶段都有验证命令、主要文件范围和回滚/评审点。
- [ ] 规划结果标出与既有 Canvas VFS/runtime surface/AgentFrame 收束任务的依赖关系，避免并行任务重叠修改同一事实源。
- [ ] 规划提出是否需要拆分子任务，并说明每个子任务的独立验收范围。

## Open Questions

- Canvas / Workspace Module crate 拆分应在本轮 feature 实现前先做，还是作为同一父任务下的后置子任务评估并分批推进？推荐答案：先完成设计评估与边界图，再把实际 crate 拆分作为独立子任务排在 Canvas bridge MVP 之前或并行进行小步迁移。
