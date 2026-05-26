# Companion interaction 持久化模型设计

## Goal

跟踪并讨论 companion request / response 独立持久化模型，厘清 session event、wait registry、pending action、human approval 与审计查询之间的事实源关系。

该任务是 `05-26-companion-interaction-capability-grant` 的后续设计任务。本期 companion 能力扩展先复用 session event、wait registry 和现有回流机制；独立持久化模型在本任务中单独讨论，避免扩大当前实现范围。

## Background

当前 companion 交互事实分布在多处：

- `companion_request(wait=true)` 通过 wait registry 等待人类回应。
- wait registry 当前是内存 `oneshot` sender map，只能在当前运行态内恢复悬挂 tool call，不能作为跨进程 / 跨 turn 的持久事实。
- `companion_request(target=human)` 通过 session meta event 发出 `companion_human_request`。
- `respond_companion_request` 通过 API 回传 response payload，并持久化 `companion_human_response`。
- `companion_respond` 同时可能 resolve pending action，也可能把 companion result 回流到 parent session。
- hook pending action、workflow approval、人类输入和 companion request 之间有相似交互语义，但事实源尚未统一。

## Requirements

### R1. 明确是否需要独立 interaction table

设计需要判断 companion request / response 是否应拥有独立持久化实体，还是继续以 session event + pending action + wait registry 组合表达。

### R2. 明确事实源边界

需要区分：

- 对话展示事实。
- 正在等待回应的运行态事实。
- 用户 / 平台 / agent 的响应事实。
- 审计查询事实。
- 可重放的 workflow / permission 决策事实。

### R3. 覆盖关键交互类型

至少评估：

- human approval / choice / text input。
- platform broker request。
- sub-session dispatch。
- parent review / result adoption。
- capability grant request。
- workflow human approval activity。

### R4. 明确运行态与持久态关系

需要说明 wait registry 是否只是 runtime waiter，持久表是否负责恢复等待状态，以及 session 重启 / backend 重连 / 页面刷新时如何展示 pending interaction。

### R5. 评估 interaction gate / resume 模型

设计需要重点评估将 `companion_request(target=human, wait=true)` 从“阻断等待用户输入的 tool call”提升为“暂停 Agent 流程的持久 gate”：

- Agent 调用 companion tool 后，runtime 创建 durable interaction gate。
- 当前 turn 进入 `waiting_on_user_input` / `waiting_on_companion` 状态，而不是长期依赖内存 future。
- 用户后续通过 API 提交 response。
- 系统根据 interaction record resume 原 session，可选择注入用户回应、继续原 tool result、或开启 follow-up turn。
- gate 的状态、response、resume 结果都可审计、可查询、可恢复。

### R6. 明确 API 与 UI 查询方式

设计需要给出最小查询模型：

- 按 session 查 pending / resolved interactions。
- 按 request id 查 interaction timeline。
- 前端卡片如何从 interaction record 渲染。
- 与 session event stream 的关系。

## Acceptance Criteria

- [ ] 明确独立 companion interaction 持久化是否必要，以及采用它的收益和代价。
- [ ] 明确 session event、wait registry、pending action、human approval、permission grant 之间的事实源关系。
- [ ] 明确 interaction 状态机草案。
- [ ] 明确 human companion wait 是否应从阻断 tool call 演进为 durable interaction gate + resume。
- [ ] 明确最小数据模型与 API 查询边界。
- [ ] 明确前端如何展示 pending / resolved interactions。
- [ ] 明确与 `05-26-companion-interaction-capability-grant` 的边界：当前任务不阻塞能力扩展 MVP。

## Out Of Scope

- 不在当前任务里实现能力 grant 链路。
- 不要求立即重做 workflow human approval。
- 不要求替换现有 session event stream；是否替换或并存需要在设计中判断。

## Notes

- Keep `prd.md` focused on requirements, constraints, and acceptance criteria.
- Lightweight tasks can remain PRD-only.
- For complex tasks, add `design.md` for technical design and `implement.md` for execution planning before `task.py start`.
